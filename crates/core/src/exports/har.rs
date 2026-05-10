//! Export a single Argos run as a HAR 1.2 archive.
//!
//! HAR (HTTP Archive) is the JSON format used by browser devtools,
//! Charles, Insomnia, k6, and most network-analysis tools. The
//! spec lives at <http://www.softwareishard.com/blog/har-12-spec/>.
//!
//! We emit one `log.entries[]` element per run. The shape is
//! intentionally minimal but spec-conformant: any field we don't have
//! data for gets the documented "unknown" sentinel (`-1` for sizes /
//! times, `""` for strings).

// HAR uses doubles for milliseconds and i64 for byte sizes. Our
// internal types are u64/usize, so converting precision-lossy casts
// would just clutter the call sites without changing behaviour for
// realistic API responses (< 2^53 bytes / ms).
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::http::{HttpBody, HttpHeader, HttpRequest, HttpResponse};

/// Build a single-entry HAR 1.2 archive for one request/response
/// exchange.
///
/// `started_at_iso8601` is the run's start time in ISO 8601 form (HAR
/// requires it on every entry). The caller should pass the same
/// timestamp the UI rendered for the run so the archive matches what
/// the user saw.
///
/// # Panics
///
/// Never in practice — the only failure mode of [`serde_json::to_value`]
/// is non-string object keys or non-finite floats, neither of which
/// our struct graph contains.
#[must_use]
pub fn to_har(req: &HttpRequest, res: &HttpResponse, started_at_iso8601: &str) -> Value {
    serde_json::to_value(Har {
        log: Log {
            version: "1.2".into(),
            creator: Creator {
                name: "Argos".into(),
                version: crate::version().to_string(),
            },
            entries: vec![entry(req, res, started_at_iso8601)],
        },
    })
    .expect("HAR struct serialises")
}

/// Same as [`to_har`] but returns a pretty-printed string ready to
/// write to a `.har` file.
///
/// # Errors
///
/// Returns [`serde_json::Error`] if the value graph fails to
/// serialise; in practice this is infallible for the structs we build.
pub fn to_har_string(
    req: &HttpRequest,
    res: &HttpResponse,
    started_at_iso8601: &str,
) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&to_har(req, res, started_at_iso8601))
}

fn entry(req: &HttpRequest, res: &HttpResponse, started_at: &str) -> Entry {
    let total = res.timing.total_ms as f64;
    let ttfb = res.timing.ttfb_ms.unwrap_or(0) as f64;
    let download = res
        .timing
        .download_ms
        .map_or((total - ttfb).max(0.0), |v| v as f64);
    let wait = (total - download).max(0.0);

    Entry {
        started_date_time: started_at.to_string(),
        time: total,
        request: req_to_har(req),
        response: res_to_har(res),
        cache: Cache {},
        timings: Timings {
            send: 0.0,
            wait,
            receive: download,
            blocked: -1.0,
            dns: res.timing.dns_ms.map_or(-1.0, |v| v as f64),
            connect: res.timing.connect_ms.map_or(-1.0, |v| v as f64),
            ssl: res.timing.tls_ms.map_or(-1.0, |v| v as f64),
        },
    }
}

fn req_to_har(req: &HttpRequest) -> RequestEntry {
    let url = compose_url(&req.url, &req.query);
    let (post_data, body_size) = req_body(req);
    RequestEntry {
        method: req.method.as_str().to_string(),
        url,
        http_version: "HTTP/1.1".into(),
        headers: headers_to_pairs(&req.headers),
        query_string: req
            .query
            .iter()
            .map(|(k, v)| Pair {
                name: k.clone(),
                value: v.clone(),
            })
            .collect(),
        cookies: Vec::new(),
        headers_size: -1,
        body_size,
        post_data,
    }
}

fn req_body(req: &HttpRequest) -> (Option<PostData>, i64) {
    match &req.body {
        None => (None, 0),
        Some(HttpBody::Json { value }) => {
            let text = value.to_string();
            let size = text.len() as i64;
            (
                Some(PostData {
                    mime_type: "application/json".into(),
                    text,
                    params: Vec::new(),
                }),
                size,
            )
        }
        Some(HttpBody::Text {
            content,
            content_type,
        }) => {
            let size = content.len() as i64;
            (
                Some(PostData {
                    mime_type: content_type.clone(),
                    text: content.clone(),
                    params: Vec::new(),
                }),
                size,
            )
        }
        Some(HttpBody::FormUrlEncoded { fields }) => {
            // Replay-friendly: text + structured params per the HAR spec.
            let text = fields
                .iter()
                .map(|(k, v)| format!("{}={}", urlencode(k), urlencode(v)))
                .collect::<Vec<_>>()
                .join("&");
            let size = text.len() as i64;
            (
                Some(PostData {
                    mime_type: "application/x-www-form-urlencoded".into(),
                    text,
                    params: fields
                        .iter()
                        .map(|(k, v)| PostParam {
                            name: k.clone(),
                            value: v.clone(),
                        })
                        .collect(),
                }),
                size,
            )
        }
        Some(HttpBody::Raw {
            bytes,
            content_type,
        }) => {
            let text = String::from_utf8_lossy(bytes).into_owned();
            let size = bytes.len() as i64;
            (
                Some(PostData {
                    mime_type: content_type.clone(),
                    text,
                    params: Vec::new(),
                }),
                size,
            )
        }
    }
}

fn res_to_har(res: &HttpResponse) -> ResponseEntry {
    let content_text = String::from_utf8_lossy(&res.body.bytes).into_owned();
    let mime_type = res
        .body
        .content_type
        .clone()
        .unwrap_or_else(|| "application/octet-stream".to_string());
    ResponseEntry {
        status: i64::from(res.status),
        status_text: res.status_text.clone(),
        http_version: "HTTP/1.1".into(),
        headers: headers_to_pairs(&res.headers),
        cookies: Vec::new(),
        content: Content {
            size: res.body.size_bytes as i64,
            mime_type,
            text: content_text,
        },
        // HAR redirect_url is the absolute URL we got 30x'd to. We
        // don't track that yet (`final_url` is the eventual landing
        // URL, not the Location header), so emit empty per spec.
        redirect_url: String::new(),
        headers_size: -1,
        body_size: res.body.size_bytes as i64,
    }
}

fn headers_to_pairs(h: &[HttpHeader]) -> Vec<Pair> {
    h.iter()
        .map(|h| Pair {
            name: h.name.clone(),
            value: h.value.clone(),
        })
        .collect()
}

fn compose_url(base: &str, query: &[(String, String)]) -> String {
    if query.is_empty() {
        return base.to_string();
    }
    if let Ok(mut u) = url::Url::parse(base) {
        {
            let mut pairs = u.query_pairs_mut();
            for (k, v) in query {
                pairs.append_pair(k, v);
            }
        }
        return u.to_string();
    }
    let sep = if base.contains('?') { '&' } else { '?' };
    let parts: Vec<String> = query
        .iter()
        .map(|(k, v)| format!("{}={}", urlencode(k), urlencode(v)))
        .collect();
    format!("{base}{sep}{}", parts.join("&"))
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            other => {
                use std::fmt::Write as _;
                let _ = write!(out, "%{other:02X}");
            }
        }
    }
    out
}

// ---- HAR struct definitions ---------------------------------------------

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Har {
    log: Log,
}

#[derive(Serialize, Deserialize)]
struct Log {
    version: String,
    creator: Creator,
    entries: Vec<Entry>,
}

#[derive(Serialize, Deserialize)]
struct Creator {
    name: String,
    version: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    started_date_time: String,
    time: f64,
    request: RequestEntry,
    response: ResponseEntry,
    cache: Cache,
    timings: Timings,
}

#[derive(Serialize, Deserialize)]
struct Cache {}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Timings {
    send: f64,
    wait: f64,
    receive: f64,
    blocked: f64,
    dns: f64,
    connect: f64,
    ssl: f64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RequestEntry {
    method: String,
    url: String,
    http_version: String,
    headers: Vec<Pair>,
    query_string: Vec<Pair>,
    cookies: Vec<Pair>,
    headers_size: i64,
    body_size: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    post_data: Option<PostData>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResponseEntry {
    status: i64,
    status_text: String,
    http_version: String,
    headers: Vec<Pair>,
    cookies: Vec<Pair>,
    content: Content,
    redirect_url: String,
    headers_size: i64,
    body_size: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Content {
    size: i64,
    mime_type: String,
    text: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PostData {
    mime_type: String,
    text: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    params: Vec<PostParam>,
}

#[derive(Serialize, Deserialize)]
struct PostParam {
    name: String,
    value: String,
}

#[derive(Serialize, Deserialize)]
struct Pair {
    name: String,
    value: String,
}

/// Unused for now — keeps the file's public surface forward-compatible
/// if we ever need to merge multiple runs into one HAR.
#[must_use]
pub fn to_har_value_with_entries(entries: Vec<Value>) -> Value {
    let mut log = Map::new();
    log.insert("version".into(), Value::String("1.2".into()));
    log.insert(
        "creator".into(),
        serde_json::json!({ "name": "Argos", "version": crate::version() }),
    );
    log.insert("entries".into(), Value::Array(entries));
    let mut root = Map::new();
    root.insert("log".into(), Value::Object(log));
    Value::Object(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{HttpMethod, ResponseBody, Timing};

    fn sample_req() -> HttpRequest {
        HttpRequest {
            method: HttpMethod::Post,
            url: "https://api.example.com/x".into(),
            headers: vec![HttpHeader::new("Accept", "application/json")],
            query: vec![("q".into(), "widgets".into())],
            body: Some(HttpBody::Json {
                value: serde_json::json!({"a": 1}),
            }),
            timeout: None,
        }
    }

    fn sample_res() -> HttpResponse {
        HttpResponse {
            status: 201,
            status_text: "Created".into(),
            headers: vec![HttpHeader::new("Content-Type", "application/json")],
            body: ResponseBody {
                bytes: br#"{"ok":true}"#.to_vec(),
                size_bytes: 11,
                content_type: Some("application/json".into()),
            },
            timing: Timing {
                total_ms: 240,
                ttfb_ms: Some(180),
                download_ms: Some(60),
                dns_ms: Some(5),
                connect_ms: Some(20),
                tls_ms: Some(30),
            },
            final_url: "https://api.example.com/x".into(),
        }
    }

    #[test]
    fn emits_har_12_log() {
        let har = to_har(&sample_req(), &sample_res(), "2026-05-10T12:00:00Z");
        assert_eq!(har["log"]["version"], "1.2");
        assert_eq!(har["log"]["creator"]["name"], "Argos");
    }

    #[test]
    fn entry_carries_request_and_response_bodies() {
        let har = to_har(&sample_req(), &sample_res(), "2026-05-10T12:00:00Z");
        let entry = &har["log"]["entries"][0];

        assert_eq!(entry["request"]["method"], "POST");
        assert!(entry["request"]["url"]
            .as_str()
            .unwrap()
            .contains("q=widgets"));
        assert_eq!(entry["request"]["postData"]["mimeType"], "application/json");
        assert_eq!(
            entry["request"]["postData"]["text"],
            "{\"a\":1}".to_string()
        );

        assert_eq!(entry["response"]["status"], 201);
        assert_eq!(entry["response"]["content"]["text"], r#"{"ok":true}"#);
        assert_eq!(entry["response"]["content"]["size"], 11);
    }

    #[test]
    fn timings_block_uses_minus_one_for_unknowns() {
        let mut res = sample_res();
        res.timing.dns_ms = None;
        res.timing.connect_ms = None;
        res.timing.tls_ms = None;
        let har = to_har(&sample_req(), &res, "2026-05-10T12:00:00Z");
        let t = &har["log"]["entries"][0]["timings"];
        assert_eq!(t["dns"], -1.0);
        assert_eq!(t["connect"], -1.0);
        assert_eq!(t["ssl"], -1.0);
    }

    #[test]
    fn pretty_string_is_valid_json() {
        let s = to_har_string(&sample_req(), &sample_res(), "2026-05-10T12:00:00Z").unwrap();
        // Round-trips through serde_json.
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["log"]["version"], "1.2");
    }
}
