// Field-level docs are intentionally omitted on data structs — the field
// names are self-explanatory and the struct-level docs cover semantics.
// We'll tighten this when the API surface stabilises (after E5 GraphQL/WS).
#![allow(missing_docs)]

//! HTTP request execution engine.
//!
//! Pure REST for now (covers MVP F3). GraphQL, gRPC, WebSocket, SSE and MQTT
//! land in their own modules in later epics (E5, E10).
//!
//! Design notes:
//! - The public API is **pure data** ([`HttpRequest`] / [`HttpResponse`]).
//!   No `reqwest` types leak across the crate boundary, which keeps the WASM
//!   bridge and the IPC contract narrow.
//! - Timing collection is deliberately coarse for v0.1: we record `total_ms`,
//!   `ttfb_ms` (time from `send` returning headers) and `download_ms` (time to
//!   drain the body). Per-phase DNS / connect / TLS will be wired in via a
//!   custom [`reqwest::Connector`] in a follow-up to T1.1.
//! - Bodies are buffered in full into [`ResponseBody`]. Streaming variants
//!   (for SSE / WebSocket / large file downloads) live behind a separate
//!   `execute_streaming` API in a later increment.

use std::time::{Duration, Instant};

use bytes::Bytes;
use reqwest::{Client, Method as ReqwestMethod};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, instrument};

/// HTTP method.
///
/// Only the canonical RFC-7231 methods plus PATCH (RFC-5789) are exposed.
/// Custom methods can be added later via a fallback variant if needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    #[default]
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

impl HttpMethod {
    /// Returns the canonical uppercase method name.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
        }
    }
}

impl From<HttpMethod> for ReqwestMethod {
    fn from(m: HttpMethod) -> Self {
        match m {
            HttpMethod::Get => Self::GET,
            HttpMethod::Post => Self::POST,
            HttpMethod::Put => Self::PUT,
            HttpMethod::Patch => Self::PATCH,
            HttpMethod::Delete => Self::DELETE,
            HttpMethod::Head => Self::HEAD,
            HttpMethod::Options => Self::OPTIONS,
        }
    }
}

/// A single HTTP header (case-insensitive name, raw string value).
///
/// We deliberately preserve insertion order via [`Vec`] rather than a map
/// because users frequently care about header order (e.g. multiple
/// `Set-Cookie`, signed-request headers).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HttpHeader {
    pub name: String,
    pub value: String,
}

impl HttpHeader {
    /// Convenience constructor.
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

/// Request body variants supported in v0.1.
///
/// `Multipart` is intentionally omitted for now — it lands together with the
/// file-upload UI in E3.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HttpBody {
    /// Plain-text body. The caller picks the content type explicitly.
    Text {
        content: String,
        content_type: String,
    },
    /// JSON body — serialised at send time, `Content-Type: application/json`
    /// is added if not already present.
    Json { value: serde_json::Value },
    /// `application/x-www-form-urlencoded` body.
    FormUrlEncoded { fields: Vec<(String, String)> },
    /// Raw byte body.
    Raw {
        bytes: Vec<u8>,
        content_type: String,
    },
}

/// An HTTP request as understood by the Argos engine.
///
/// All variable substitution (`{{baseUrl}}`, `{{token}}`) must happen
/// **before** a request reaches [`HttpClient::execute`]. The engine treats
/// the URL, headers and body verbatim.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<HttpHeader>,
    /// Query parameters appended to the URL. Multiple entries with the same
    /// key are preserved (e.g. `?tag=a&tag=b`).
    #[serde(default)]
    pub query: Vec<(String, String)>,
    #[serde(default)]
    pub body: Option<HttpBody>,
    /// Total request timeout. `None` means use the client default.
    #[serde(default, with = "serde_duration_opt")]
    pub timeout: Option<Duration>,
}

impl Default for HttpRequest {
    fn default() -> Self {
        Self {
            method: HttpMethod::Get,
            url: String::new(),
            headers: Vec::new(),
            query: Vec::new(),
            body: None,
            timeout: None,
        }
    }
}

/// Response body — fully buffered for v0.1.
///
/// Stored as raw bytes plus the negotiated content type so the UI can choose
/// whether to render as JSON, text, image, hex etc. without re-parsing the
/// `Content-Type` header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseBody {
    pub bytes: Vec<u8>,
    pub size_bytes: usize,
    /// Mime-type as parsed from the response `Content-Type` header (lower-case
    /// `type/subtype`, parameters stripped). `None` if missing or invalid.
    pub content_type: Option<String>,
}

impl ResponseBody {
    /// Decode the body as UTF-8 text. Returns `None` if the bytes are not valid UTF-8.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.bytes).ok()
    }

    /// Parse the body as JSON.
    ///
    /// # Errors
    ///
    /// Returns [`serde_json::Error`] if the body bytes are not valid JSON.
    pub fn as_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::from_slice(&self.bytes)
    }
}

/// Coarse-grained timing breakdown for one request.
///
/// All values are milliseconds. `Option` fields are populated when reqwest /
/// hyper expose the corresponding event; the rest will be filled as we wire a
/// custom connector in a follow-up.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Timing {
    /// Total wall-clock time from `execute()` entry to response fully drained.
    pub total_ms: u64,
    /// Time from `send` start until headers came back (TTFB).
    pub ttfb_ms: Option<u64>,
    /// Time spent draining the response body after headers arrived.
    pub download_ms: Option<u64>,
    /// DNS resolution time. Currently unset (TODO via custom connector).
    pub dns_ms: Option<u64>,
    /// TCP connect time. Currently unset (TODO via custom connector).
    pub connect_ms: Option<u64>,
    /// TLS handshake time. Currently unset (TODO via custom connector).
    pub tls_ms: Option<u64>,
}

/// An HTTP response as understood by the Argos engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpResponse {
    /// HTTP status code (e.g. 200, 404).
    pub status: u16,
    /// Canonical reason phrase (e.g. "OK", "Not Found"). Empty if reqwest
    /// could not derive one.
    pub status_text: String,
    pub headers: Vec<HttpHeader>,
    pub body: ResponseBody,
    pub timing: Timing,
    /// URL the engine actually fetched. Differs from the request URL when a
    /// redirect chain was followed.
    pub final_url: String,
}

impl HttpResponse {
    /// Quick predicate for the 2xx range.
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.status >= 200 && self.status < 300
    }

    /// Quick predicate for the 4xx range.
    #[must_use]
    pub fn is_client_error(&self) -> bool {
        self.status >= 400 && self.status < 500
    }

    /// Quick predicate for the 5xx range.
    #[must_use]
    pub fn is_server_error(&self) -> bool {
        self.status >= 500 && self.status < 600
    }

    /// Look up a response header by case-insensitive name.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case(name))
            .map(|h| h.value.as_str())
    }
}

/// Errors returned by [`HttpClient::execute`].
#[derive(Debug, Error)]
pub enum HttpError {
    #[error("invalid URL: {0}")]
    InvalidUrl(String),
    #[error("invalid header `{name}`: {error}")]
    InvalidHeader { name: String, error: String },
    #[error("request timed out")]
    Timeout,
    #[error("network error: {0}")]
    Network(String),
    #[error("body serialisation failed: {0}")]
    Body(String),
}

impl From<reqwest::Error> for HttpError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            Self::Timeout
        } else {
            Self::Network(e.to_string())
        }
    }
}

/// Argos HTTP client.
///
/// Wraps a [`reqwest::Client`] with an Argos-shaped API. Cheap to clone — the
/// underlying `Client` uses `Arc` internally for the connection pool.
#[derive(Debug, Clone)]
pub struct HttpClient {
    inner: Client,
}

impl HttpClient {
    /// Build the default Argos HTTP client.
    ///
    /// Defaults:
    /// - User-Agent: `argos/<crate-version>`
    /// - TLS: rustls (built into reqwest via the workspace feature flags)
    /// - Redirects: followed up to 10 hops
    /// - Default timeout: none (caller specifies per request)
    ///
    /// # Errors
    ///
    /// Returns [`HttpError::Network`] if the underlying TLS / DNS resolver
    /// could not be initialised.
    pub fn new() -> Result<Self, HttpError> {
        Self::with_user_agent(format!("argos/{}", crate::VERSION))
    }

    /// Build a client with a custom User-Agent — handy for integration tests.
    ///
    /// # Errors
    ///
    /// Same as [`HttpClient::new`].
    pub fn with_user_agent(ua: impl Into<String>) -> Result<Self, HttpError> {
        let client = Client::builder()
            .user_agent(ua.into())
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .map_err(HttpError::from)?;
        Ok(Self { inner: client })
    }

    /// Execute one HTTP request and buffer the full response.
    ///
    /// # Errors
    ///
    /// Returns [`HttpError::InvalidUrl`] if the URL or the merged query
    /// parameters cannot be parsed; [`HttpError::Timeout`] if the request
    /// exceeded its budget; [`HttpError::Network`] for transport-level
    /// failures.
    #[instrument(skip(self, req), fields(method = %req.method.as_str(), url = %req.url))]
    pub async fn execute(&self, req: &HttpRequest) -> Result<HttpResponse, HttpError> {
        let started = Instant::now();
        let url = build_url(&req.url, &req.query)?;

        let mut builder = self.inner.request(req.method.into(), url);

        for h in &req.headers {
            builder = builder.header(&h.name, &h.value);
        }

        if let Some(body) = &req.body {
            builder = apply_body(builder, body);
        }

        if let Some(timeout) = req.timeout {
            builder = builder.timeout(timeout);
        }

        let send_started = Instant::now();
        let resp = builder.send().await?;
        let ttfb_ms = duration_ms(send_started.elapsed());

        let status = resp.status().as_u16();
        let status_text = resp
            .status()
            .canonical_reason()
            .unwrap_or_default()
            .to_string();
        let final_url = resp.url().to_string();
        let headers = collect_headers(&resp);
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<mime::Mime>().ok())
            .map(|m| format!("{}/{}", m.type_(), m.subtype()).to_lowercase());

        let download_started = Instant::now();
        let body_bytes: Bytes = resp.bytes().await?;
        let download_ms = duration_ms(download_started.elapsed());

        let total_ms = duration_ms(started.elapsed());

        debug!(status, total_ms, ttfb_ms, "request complete");

        Ok(HttpResponse {
            status,
            status_text,
            headers,
            body: ResponseBody {
                size_bytes: body_bytes.len(),
                bytes: body_bytes.to_vec(),
                content_type,
            },
            timing: Timing {
                total_ms,
                ttfb_ms: Some(ttfb_ms),
                download_ms: Some(download_ms),
                ..Timing::default()
            },
            final_url,
        })
    }
}

// ---------- internals ----------

fn build_url(base: &str, query: &[(String, String)]) -> Result<url::Url, HttpError> {
    let mut url = url::Url::parse(base).map_err(|e| HttpError::InvalidUrl(e.to_string()))?;
    if !query.is_empty() {
        let mut pairs = url.query_pairs_mut();
        for (k, v) in query {
            pairs.append_pair(k, v);
        }
    }
    Ok(url)
}

fn apply_body(builder: reqwest::RequestBuilder, body: &HttpBody) -> reqwest::RequestBuilder {
    match body {
        HttpBody::Text {
            content,
            content_type,
        } => builder
            .header(reqwest::header::CONTENT_TYPE, content_type)
            .body(content.clone()),
        HttpBody::Json { value } => builder.json(value),
        HttpBody::FormUrlEncoded { fields } => builder.form(fields),
        HttpBody::Raw {
            bytes,
            content_type,
        } => builder
            .header(reqwest::header::CONTENT_TYPE, content_type)
            .body(bytes.clone()),
    }
}

fn collect_headers(resp: &reqwest::Response) -> Vec<HttpHeader> {
    // reqwest iterates header pairs in insertion order, including duplicates
    // (e.g. multiple `Set-Cookie`), so we just clone the sequence.
    resp.headers()
        .iter()
        .map(|(k, v)| HttpHeader::new(k.as_str(), v.to_str().unwrap_or("")))
        .collect()
}

#[allow(clippy::cast_possible_truncation)]
fn duration_ms(d: Duration) -> u64 {
    d.as_millis() as u64
}

// `Option<Duration>` doesn't have a built-in serde impl that round-trips
// gracefully, so we use a thin module that serialises as floating-point seconds.
mod serde_duration_opt {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    // `&Option<T>` is mandated by serde's #[serde(with = ...)] interface.
    #[allow(clippy::ref_option)]
    pub fn serialize<S: Serializer>(d: &Option<Duration>, s: S) -> Result<S::Ok, S::Error> {
        d.map(|d| d.as_secs_f64()).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Duration>, D::Error> {
        Option::<f64>::deserialize(d).map(|opt| opt.map(Duration::from_secs_f64))
    }
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use httpmock::prelude::*;
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn get_returns_200_and_body() {
        let server = MockServer::start_async().await;
        let _m = server
            .mock_async(|when, then| {
                when.method(GET).path("/users");
                then.status(200)
                    .header("content-type", "application/json")
                    .body(r#"{"users":[]}"#);
            })
            .await;

        let client = HttpClient::new().expect("client builds");
        let req = HttpRequest {
            url: server.url("/users"),
            ..Default::default()
        };
        let resp = client.execute(&req).await.expect("request succeeds");

        assert_eq!(resp.status, 200);
        assert!(resp.is_success());
        assert_eq!(resp.body.size_bytes, 12);
        assert_eq!(resp.body.content_type.as_deref(), Some("application/json"));
        assert_eq!(resp.body.as_str(), Some(r#"{"users":[]}"#));
        assert!(resp.timing.total_ms < 5_000, "test response should be fast");
        assert!(resp.timing.ttfb_ms.is_some());
    }

    #[tokio::test]
    async fn query_params_are_appended() {
        let server = MockServer::start_async().await;
        let _m = server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/items")
                    .query_param("page", "2")
                    .query_param("limit", "50");
                then.status(200).body("ok");
            })
            .await;

        let client = HttpClient::new().unwrap();
        let req = HttpRequest {
            url: server.url("/items"),
            query: vec![("page".into(), "2".into()), ("limit".into(), "50".into())],
            ..Default::default()
        };
        let resp = client.execute(&req).await.unwrap();
        assert_eq!(resp.status, 200);
    }

    #[tokio::test]
    async fn json_post_body_is_serialised() {
        let server = MockServer::start_async().await;
        let _m = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/users")
                    .header("content-type", "application/json")
                    .json_body(json!({ "name": "Alice", "role": "admin" }));
                then.status(201).header("location", "/users/42").body("");
            })
            .await;

        let client = HttpClient::new().unwrap();
        let req = HttpRequest {
            method: HttpMethod::Post,
            url: server.url("/users"),
            body: Some(HttpBody::Json {
                value: json!({ "name": "Alice", "role": "admin" }),
            }),
            ..Default::default()
        };
        let resp = client.execute(&req).await.unwrap();
        assert_eq!(resp.status, 201);
        assert_eq!(resp.header("Location"), Some("/users/42"));
    }

    #[tokio::test]
    async fn custom_headers_propagate() {
        let server = MockServer::start_async().await;
        let _m = server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/me")
                    .header("authorization", "Bearer abc");
                then.status(200);
            })
            .await;

        let client = HttpClient::new().unwrap();
        let req = HttpRequest {
            url: server.url("/me"),
            headers: vec![HttpHeader::new("Authorization", "Bearer abc")],
            ..Default::default()
        };
        let resp = client.execute(&req).await.unwrap();
        assert_eq!(resp.status, 200);
    }

    #[tokio::test]
    async fn form_url_encoded_body_works() {
        let server = MockServer::start_async().await;
        let _m = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/login")
                    .header_exists("content-type")
                    .body_contains("user=alice")
                    .body_contains("pass=secret");
                then.status(200);
            })
            .await;

        let client = HttpClient::new().unwrap();
        let req = HttpRequest {
            method: HttpMethod::Post,
            url: server.url("/login"),
            body: Some(HttpBody::FormUrlEncoded {
                fields: vec![
                    ("user".into(), "alice".into()),
                    ("pass".into(), "secret".into()),
                ],
            }),
            ..Default::default()
        };
        let resp = client.execute(&req).await.unwrap();
        assert_eq!(resp.status, 200);
    }

    #[tokio::test]
    async fn server_5xx_is_returned_as_response_not_error() {
        let server = MockServer::start_async().await;
        let _m = server
            .mock_async(|when, then| {
                when.method(GET).path("/oops");
                then.status(503).body("nope");
            })
            .await;

        let client = HttpClient::new().unwrap();
        let req = HttpRequest {
            url: server.url("/oops"),
            ..Default::default()
        };
        let resp = client.execute(&req).await.unwrap();
        assert_eq!(resp.status, 503);
        assert!(resp.is_server_error());
        assert!(!resp.is_success());
        assert_eq!(resp.body.as_str(), Some("nope"));
    }

    #[tokio::test]
    async fn redirect_is_followed_and_final_url_reported() {
        let server = MockServer::start_async().await;
        let _redirect = server
            .mock_async(|when, then| {
                when.method(GET).path("/old");
                then.status(302).header("location", "/new");
            })
            .await;
        let _final = server
            .mock_async(|when, then| {
                when.method(GET).path("/new");
                then.status(200).body("here");
            })
            .await;

        let client = HttpClient::new().unwrap();
        let req = HttpRequest {
            url: server.url("/old"),
            ..Default::default()
        };
        let resp = client.execute(&req).await.unwrap();
        assert_eq!(resp.status, 200);
        assert!(resp.final_url.ends_with("/new"));
        assert_eq!(resp.body.as_str(), Some("here"));
    }

    #[tokio::test]
    async fn timeout_is_reported_as_timeout_error() {
        let server = MockServer::start_async().await;
        let _m = server
            .mock_async(|when, then| {
                when.method(GET).path("/slow");
                then.status(200)
                    .delay(std::time::Duration::from_millis(500));
            })
            .await;

        let client = HttpClient::new().unwrap();
        let req = HttpRequest {
            url: server.url("/slow"),
            timeout: Some(Duration::from_millis(100)),
            ..Default::default()
        };
        let err = client.execute(&req).await.expect_err("should timeout");
        assert!(
            matches!(err, HttpError::Timeout),
            "expected Timeout, got {err:?}"
        );
    }

    #[tokio::test]
    async fn invalid_url_is_caught_before_send() {
        let client = HttpClient::new().unwrap();
        let req = HttpRequest {
            url: "not a url".into(),
            ..Default::default()
        };
        let err = client.execute(&req).await.expect_err("should reject URL");
        assert!(matches!(err, HttpError::InvalidUrl(_)));
    }

    #[test]
    fn http_method_round_trips_via_serde() {
        let m = HttpMethod::Patch;
        let s = serde_json::to_string(&m).unwrap();
        assert_eq!(s, r#""PATCH""#);
        let back: HttpMethod = serde_json::from_str(&s).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn http_request_default_is_get_empty() {
        let r = HttpRequest::default();
        assert_eq!(r.method, HttpMethod::Get);
        assert!(r.url.is_empty());
        assert!(r.headers.is_empty());
        assert!(r.body.is_none());
    }
}
