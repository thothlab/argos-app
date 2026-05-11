//! Request file — `<slug>.argos.yaml`. Captures everything needed to
//! recreate a single HTTP request: method, URL, params/headers, body, auth
//! and the optional pre-request / post-response scripts.
//!
//! v0.1 covers REST. GraphQL / gRPC / WebSocket / SSE / MQTT add new
//! variants of [`RequestDraft`] in their respective epics (E5, E10).

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{read_yaml, write_yaml_atomic, FormatError, Kind};
use crate::http::HttpMethod;

/// `kind: request`. The "draft" name distinguishes this from the wire-shape
/// [`crate::HttpRequest`] inside [`crate::http`] — drafts carry editor
/// metadata (enabled flags, schema refs) that the engine doesn't need.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequestDraft {
    #[serde(default = "default_kind")]
    pub kind: Kind,

    pub name: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Type of the request. Tagged enum.
    #[serde(flatten)]
    pub variant: RequestVariant,

    #[serde(default, skip_serializing_if = "ScriptHooks::is_empty")]
    pub scripts: ScriptHooks,

    /// Optional reference to a schema fragment (`openapi/users.yaml#/paths/~1users/get`)
    /// used by future schema-aware validation (F15).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_ref: Option<String>,
}

impl RequestDraft {
    #[must_use]
    pub fn new_rest(name: impl Into<String>, method: HttpMethod, url: impl Into<String>) -> Self {
        Self {
            kind: Kind::Request,
            name: name.into(),
            description: None,
            variant: RequestVariant::Rest(RestRequest {
                method,
                url: url.into(),
                query: Vec::new(),
                headers: Vec::new(),
                auth: None,
                body: None,
            }),
            scripts: ScriptHooks::default(),
            schema_ref: None,
        }
    }

    /// New GraphQL request stub — empty query + no variables. The
    /// editor materialises this when the user clicks "New GraphQL".
    #[must_use]
    pub fn new_graphql(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            kind: Kind::Request,
            name: name.into(),
            description: None,
            variant: RequestVariant::Graphql(GraphqlRequest {
                url: url.into(),
                query: String::new(),
                variables: None,
                operation_name: None,
                headers: Vec::new(),
                auth: None,
            }),
            scripts: ScriptHooks::default(),
            schema_ref: None,
        }
    }

    /// New WebSocket request stub.
    #[must_use]
    pub fn new_websocket(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            kind: Kind::Request,
            name: name.into(),
            description: None,
            variant: RequestVariant::Websocket(WebsocketRequest {
                url: url.into(),
                subprotocols: Vec::new(),
                headers: Vec::new(),
                auth: None,
                messages: Vec::new(),
            }),
            scripts: ScriptHooks::default(),
            schema_ref: None,
        }
    }

    /// Read a request file.
    ///
    /// # Errors
    ///
    /// I/O or YAML parse errors.
    pub fn load(path: &Path) -> Result<Self, FormatError> {
        read_yaml(path, "request")
    }

    /// Write a request file.
    ///
    /// # Errors
    ///
    /// I/O or YAML serialisation failures.
    pub fn save(&self, path: &Path) -> Result<(), FormatError> {
        write_yaml_atomic(path, self)
    }
}

/// Discriminated union over the protocols Argos can speak.
///
/// New variants land alongside REST as their epics complete. Each
/// arm's body lives in its own struct so the YAML stays
/// human-readable — `type: graphql` / `type: websocket` flattened at
/// the [`RequestDraft`] level surfaces protocol-specific keys
/// directly (`query:`, `variables:` for GraphQL; `subprotocols:` for
/// WS), not nested under an opaque variant blob.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RequestVariant {
    Rest(RestRequest),
    /// GraphQL request — operation + variables, sent as a JSON POST
    /// over the existing HTTP engine. Wired up in E5 chunk 2.
    Graphql(GraphqlRequest),
    /// Persistent WebSocket connection. Wired up in E5 chunk 3 — for
    /// now the variant exists so we can save/load the request file
    /// and switch protocols in the editor.
    Websocket(WebsocketRequest),
    // gRPC / SSE / MQTT land in P2 (E10).
}

impl RequestVariant {
    /// Short identifier used by the UI and CLI for status pills.
    #[must_use]
    pub fn protocol_tag(&self) -> &'static str {
        match self {
            Self::Rest(_) => "rest",
            Self::Graphql(_) => "graphql",
            Self::Websocket(_) => "websocket",
        }
    }

    /// Borrow as a REST request, returning `None` for non-REST
    /// protocols. Lets call sites that only handle REST stay terse:
    /// `let Some(rest) = draft.variant.as_rest() else { … }`.
    #[must_use]
    pub fn as_rest(&self) -> Option<&RestRequest> {
        match self {
            Self::Rest(r) => Some(r),
            _ => None,
        }
    }
}

impl RequestDraft {
    /// Convenience: borrow the inner [`RestRequest`] if this draft is
    /// REST-shaped, otherwise `None`.
    #[must_use]
    pub fn as_rest(&self) -> Option<&RestRequest> {
        self.variant.as_rest()
    }
}

/// GraphQL request body. Sent as `POST <url>` with
/// `Content-Type: application/json` and the body
/// `{ "query": <query>, "variables": <variables>, "operationName": ... }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphqlRequest {
    pub url: String,

    /// The GraphQL document — `query`, `mutation`, or `subscription`.
    /// Subscriptions are accepted in the file format but execution
    /// support lands with E5/E10 streaming.
    pub query: String,

    /// JSON object holding variable bindings. `null` is fine for
    /// query documents that take no variables.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<serde_json::Value>,

    /// `operationName` field — used when a document declares multiple
    /// named operations so the server knows which to run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_name: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KeyValue>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthConfig>,
}

/// WebSocket request — a persistent connection plus a library of
/// message templates the editor can send. The actual lifecycle is
/// driven by [`crate::http`] in chunk 3.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebsocketRequest {
    /// `ws://` or `wss://` URL.
    pub url: String,

    /// Subprotocols advertised in the handshake. Most servers expect
    /// an empty list; GraphQL-WS uses `graphql-transport-ws`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subprotocols: Vec<String>,

    /// Connection-time headers (e.g. `Authorization`). Not all WS
    /// servers honour these — depends on the implementation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KeyValue>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthConfig>,

    /// Pre-canned outgoing messages — picked from the UI / replayed
    /// by `argos run --ws-send`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<WsMessageTemplate>,
}

/// One outgoing message template. Bodies are stored as text — JSON
/// messages keep their formatting; binary payloads aren't supported
/// in v1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WsMessageTemplate {
    pub name: String,
    pub body: String,
}

/// REST-shaped request — mirrors the wire shape with editor-friendly
/// `enabled` flags on multi-value entries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RestRequest {
    pub method: HttpMethod,
    pub url: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub query: Vec<KeyValue>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<KeyValue>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthConfig>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<BodyDraft>,
}

/// Editor-shaped key-value pair with an `enabled` flag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyValue {
    pub name: String,
    pub value: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// Auth configuration. Concrete variants land in E3.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthConfig {
    /// Inherit from the parent folder. The default for new requests inside
    /// a folder that already has an `auth` block.
    Inherit,
    /// Bearer token in the `Authorization` header.
    Bearer { token: String },
    /// Basic auth with username + password.
    Basic { username: String, password: String },
    /// Custom API key in a header / query / cookie.
    ApiKey {
        name: String,
        value: String,
        #[serde(default = "default_apikey_in")]
        location: ApiKeyLocation,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiKeyLocation {
    Header,
    Query,
    Cookie,
}

fn default_apikey_in() -> ApiKeyLocation {
    ApiKeyLocation::Header
}

/// Body variants supported by the editor format. Mirrors `HttpBody` in
/// shape but distinguishes JSON from arbitrary text — JSON bodies survive
/// round-trip without losing their structure when the user manually
/// reformats them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BodyDraft {
    Text {
        content: String,
        content_type: String,
    },
    Json {
        value: serde_json::Value,
    },
    FormUrlEncoded {
        fields: Vec<FormField>,
    },
}

/// Form-urlencoded field, with the same `enabled` flag pattern as headers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormField {
    pub name: String,
    pub value: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// Pre-request and post-response JS hooks. Bodies are stored as raw strings
/// to preserve formatting (comments, blank lines) the user wrote.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptHooks {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_request: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tests: Option<String>,
}

impl ScriptHooks {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pre_request.is_none() && self.tests.is_none()
    }
}

fn default_kind() -> Kind {
    Kind::Request
}

fn default_enabled() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tempfile::tempdir;

    use super::*;

    fn sample_dir() -> tempfile::TempDir {
        tempdir().unwrap()
    }

    #[test]
    fn round_trip_minimal_get() {
        let dir = sample_dir();
        let path = dir.path().join("list-users.argos.yaml");

        let r = RequestDraft::new_rest("List users", HttpMethod::Get, "{{baseUrl}}/users");
        r.save(&path).unwrap();
        let loaded = RequestDraft::load(&path).unwrap();
        assert_eq!(loaded, r);
    }

    #[test]
    fn round_trip_post_with_json_and_headers() {
        let dir = sample_dir();
        let path = dir.path().join("create-user.argos.yaml");

        let r = RequestDraft {
            kind: Kind::Request,
            name: "Create user".into(),
            description: Some("Creates a new user.".into()),
            variant: RequestVariant::Rest(RestRequest {
                method: HttpMethod::Post,
                url: "{{baseUrl}}/users".into(),
                query: vec![],
                headers: vec![KeyValue {
                    name: "X-Trace-Id".into(),
                    value: "{{trace}}".into(),
                    enabled: true,
                }],
                auth: Some(AuthConfig::Bearer {
                    token: "{{token}}".into(),
                }),
                body: Some(BodyDraft::Json {
                    value: json!({ "name": "Alice", "role": "admin" }),
                }),
            }),
            scripts: ScriptHooks {
                pre_request: Some("bru.env.set('ts', Date.now());".into()),
                tests: Some("expect(response.status).toBe(201);".into()),
            },
            schema_ref: Some("openapi/users.yaml#/paths/~1users/post".into()),
        };

        r.save(&path).unwrap();
        let loaded = RequestDraft::load(&path).unwrap();
        assert_eq!(loaded, r);
    }

    #[test]
    fn round_trip_graphql_request() {
        let dir = sample_dir();
        let path = dir.path().join("list-posts.argos.yaml");
        let mut r = RequestDraft::new_graphql("List posts", "{{baseUrl}}/graphql");
        let RequestVariant::Graphql(g) = &mut r.variant else { panic!() };
        g.query = "query ListPosts($limit: Int) { posts(limit: $limit) { id title } }".into();
        g.variables = Some(json!({ "limit": 10 }));
        g.operation_name = Some("ListPosts".into());
        g.headers.push(KeyValue {
            name: "X-Apollo".into(),
            value: "true".into(),
            enabled: true,
        });
        g.auth = Some(AuthConfig::Bearer {
            token: "{{token}}".into(),
        });

        r.save(&path).unwrap();
        let loaded = RequestDraft::load(&path).unwrap();
        assert_eq!(loaded, r);
        assert_eq!(loaded.variant.protocol_tag(), "graphql");
    }

    #[test]
    fn round_trip_websocket_request() {
        let dir = sample_dir();
        let path = dir.path().join("chat.argos.yaml");
        let mut r = RequestDraft::new_websocket("Chat socket", "wss://chat.example.com/socket");
        let RequestVariant::Websocket(w) = &mut r.variant else { panic!() };
        w.subprotocols.push("graphql-transport-ws".into());
        w.messages.push(WsMessageTemplate {
            name: "Ping".into(),
            body: r#"{"type":"ping"}"#.into(),
        });
        w.auth = Some(AuthConfig::Bearer {
            token: "{{token}}".into(),
        });
        r.save(&path).unwrap();
        let loaded = RequestDraft::load(&path).unwrap();
        assert_eq!(loaded, r);
        assert_eq!(loaded.variant.protocol_tag(), "websocket");
    }

    #[test]
    fn as_rest_returns_none_for_non_rest() {
        let g = RequestDraft::new_graphql("g", "https://x");
        assert!(g.as_rest().is_none());
        let r = RequestDraft::new_rest("r", HttpMethod::Get, "https://x");
        assert!(r.as_rest().is_some());
    }

    #[test]
    fn round_trip_form_body() {
        let dir = sample_dir();
        let path = dir.path().join("login.argos.yaml");
        let r = RequestDraft {
            kind: Kind::Request,
            name: "Login".into(),
            description: None,
            variant: RequestVariant::Rest(RestRequest {
                method: HttpMethod::Post,
                url: "{{baseUrl}}/login".into(),
                query: vec![],
                headers: vec![],
                auth: None,
                body: Some(BodyDraft::FormUrlEncoded {
                    fields: vec![
                        FormField {
                            name: "user".into(),
                            value: "alice".into(),
                            enabled: true,
                        },
                        FormField {
                            name: "pass".into(),
                            value: "{{password}}".into(),
                            enabled: true,
                        },
                    ],
                }),
            }),
            scripts: ScriptHooks::default(),
            schema_ref: None,
        };
        r.save(&path).unwrap();
        let loaded = RequestDraft::load(&path).unwrap();
        assert_eq!(loaded, r);
    }

    #[test]
    fn yaml_omits_empty_optional_fields() {
        let r = RequestDraft::new_rest("Get user", HttpMethod::Get, "/x");
        let s = serde_yaml::to_string(&r).unwrap();
        assert!(!s.contains("description"));
        assert!(!s.contains("schema_ref"));
        assert!(!s.contains("scripts"));
    }
}
