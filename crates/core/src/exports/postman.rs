//! Export an Argos workspace tree as a Postman v2.1 collection JSON.
//!
//! The goal is round-trip: an Argos workspace → Postman JSON →
//! re-importable through Argos's own `imports::postman` module. We
//! emit only the subset of the Postman schema we faithfully understand
//! (the same one we accept on import); anything richer that the user
//! adds in Postman itself is preserved if they import-and-re-export
//! without editing.

#![allow(clippy::match_wildcard_for_single_variants)]

use serde_json::{json, Map, Value};

use crate::format::request::{AuthConfig, BodyDraft, KeyValue, RequestVariant, ScriptHooks};
use crate::workspace::TreeNode;

const SCHEMA_URL: &str = "https://schema.getpostman.com/json/collection/v2.1.0/collection.json";

/// Render a [`TreeNode`] as a Postman v2.1 collection JSON value.
///
/// `name` becomes the collection's `info.name`. `tree` is typically
/// the workspace's `collections` folder; nested folders and requests
/// are mapped 1:1.
#[must_use]
pub fn to_postman_v21(name: &str, tree: &TreeNode) -> Value {
    let items = match tree {
        TreeNode::Folder { children, .. } => children.iter().map(node_to_item).collect::<Vec<_>>(),
        TreeNode::Request { .. } => vec![node_to_item(tree)],
    };

    json!({
        "info": {
            "name": name,
            "schema": SCHEMA_URL,
            "_exporter_id": "argos",
        },
        "item": items,
    })
}

/// Serialise the value at the canonical 2-space indentation Postman
/// itself uses.
///
/// # Errors
///
/// Returns [`serde_json::Error`] if serialisation fails. In practice
/// the value graph we construct never contains non-string keys or
/// non-finite numbers, so this is essentially infallible.
pub fn to_postman_v21_string(name: &str, tree: &TreeNode) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&to_postman_v21(name, tree))
}

fn node_to_item(node: &TreeNode) -> Value {
    match node {
        TreeNode::Folder {
            name,
            meta,
            children,
            ..
        } => {
            let mut obj = Map::new();
            obj.insert("name".into(), Value::String(name.clone()));
            if let Some(desc) = meta.as_ref().and_then(|m| m.description.clone()) {
                obj.insert("description".into(), Value::String(desc));
            }
            obj.insert(
                "item".into(),
                Value::Array(children.iter().map(node_to_item).collect()),
            );
            Value::Object(obj)
        }
        TreeNode::Request { draft, .. } => {
            let mut obj = Map::new();
            obj.insert("name".into(), Value::String(draft.name.clone()));
            if let Some(desc) = draft.description.clone() {
                obj.insert("description".into(), Value::String(desc));
            }
            obj.insert("request".into(), request_to_value(&draft.variant));
            let events = scripts_to_events(&draft.scripts);
            if !events.is_empty() {
                obj.insert("event".into(), Value::Array(events));
            }
            Value::Object(obj)
        }
    }
}

fn request_to_value(variant: &RequestVariant) -> Value {
    let RequestVariant::Rest(rest) = variant;
    let mut req = Map::new();
    req.insert(
        "method".into(),
        Value::String(rest.method.as_str().to_string()),
    );
    req.insert("url".into(), url_to_value(&rest.url, &rest.query));

    if !rest.headers.is_empty() {
        req.insert(
            "header".into(),
            Value::Array(rest.headers.iter().map(kv_to_header).collect()),
        );
    }
    if let Some(auth) = &rest.auth {
        if let Some(v) = auth_to_value(auth) {
            req.insert("auth".into(), v);
        }
    }
    if let Some(body) = &rest.body {
        req.insert("body".into(), body_to_value(body));
    }
    Value::Object(req)
}

fn url_to_value(raw: &str, query: &[KeyValue]) -> Value {
    // We always emit the object form so Postman can render the
    // structured URL editor — but the `raw` field keeps the user's
    // templating (`{{baseUrl}}`) intact. Query params live inside the
    // structured `query` array, exactly mirroring import.
    let mut url = Map::new();
    let raw_with_query = if query.is_empty() {
        raw.to_string()
    } else {
        let enabled: Vec<String> = query
            .iter()
            .filter(|q| q.enabled)
            .map(|q| {
                if q.value.is_empty() {
                    q.name.clone()
                } else {
                    format!("{}={}", q.name, q.value)
                }
            })
            .collect();
        if enabled.is_empty() {
            raw.to_string()
        } else {
            let sep = if raw.contains('?') { '&' } else { '?' };
            format!("{raw}{sep}{}", enabled.join("&"))
        }
    };
    url.insert("raw".into(), Value::String(raw_with_query));
    if !query.is_empty() {
        url.insert(
            "query".into(),
            Value::Array(
                query
                    .iter()
                    .map(|q| {
                        let mut m = Map::new();
                        m.insert("key".into(), Value::String(q.name.clone()));
                        m.insert("value".into(), Value::String(q.value.clone()));
                        if !q.enabled {
                            m.insert("disabled".into(), Value::Bool(true));
                        }
                        Value::Object(m)
                    })
                    .collect(),
            ),
        );
    }
    Value::Object(url)
}

fn kv_to_header(h: &KeyValue) -> Value {
    let mut m = Map::new();
    m.insert("key".into(), Value::String(h.name.clone()));
    m.insert("value".into(), Value::String(h.value.clone()));
    if !h.enabled {
        m.insert("disabled".into(), Value::Bool(true));
    }
    Value::Object(m)
}

fn auth_to_value(auth: &AuthConfig) -> Option<Value> {
    match auth {
        AuthConfig::Inherit => None,
        AuthConfig::Bearer { token } => Some(json!({
            "type": "bearer",
            "bearer": [{ "key": "token", "value": token, "type": "string" }],
        })),
        AuthConfig::Basic { username, password } => Some(json!({
            "type": "basic",
            "basic": [
                { "key": "username", "value": username, "type": "string" },
                { "key": "password", "value": password, "type": "string" },
            ],
        })),
        AuthConfig::ApiKey {
            name,
            value,
            location,
        } => {
            let loc = match location {
                crate::format::request::ApiKeyLocation::Header => "header",
                crate::format::request::ApiKeyLocation::Query => "query",
                crate::format::request::ApiKeyLocation::Cookie => "cookie",
            };
            Some(json!({
                "type": "apikey",
                "apikey": [
                    { "key": "key", "value": name, "type": "string" },
                    { "key": "value", "value": value, "type": "string" },
                    { "key": "in", "value": loc, "type": "string" },
                ],
            }))
        }
    }
}

fn body_to_value(body: &BodyDraft) -> Value {
    match body {
        BodyDraft::Text {
            content,
            content_type,
        } => {
            let lang = match content_type.as_str() {
                "application/json" => "json",
                "application/xml" => "xml",
                "text/html" => "html",
                "application/javascript" => "javascript",
                _ => "text",
            };
            json!({
                "mode": "raw",
                "raw": content,
                "options": { "raw": { "language": lang } },
            })
        }
        BodyDraft::Json { value } => {
            let raw = serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string());
            json!({
                "mode": "raw",
                "raw": raw,
                "options": { "raw": { "language": "json" } },
            })
        }
        BodyDraft::FormUrlEncoded { fields } => {
            let arr: Vec<Value> = fields
                .iter()
                .map(|f| {
                    let mut m = Map::new();
                    m.insert("key".into(), Value::String(f.name.clone()));
                    m.insert("value".into(), Value::String(f.value.clone()));
                    m.insert("type".into(), Value::String("text".into()));
                    if !f.enabled {
                        m.insert("disabled".into(), Value::Bool(true));
                    }
                    Value::Object(m)
                })
                .collect();
            json!({ "mode": "urlencoded", "urlencoded": arr })
        }
    }
}

fn scripts_to_events(scripts: &ScriptHooks) -> Vec<Value> {
    let mut out = Vec::new();
    if let Some(pre) = scripts.pre_request.as_deref() {
        out.push(json!({
            "listen": "prerequest",
            "script": {
                "type": "text/javascript",
                "exec": pre.split('\n').collect::<Vec<_>>(),
            },
        }));
    }
    if let Some(tests) = scripts.tests.as_deref() {
        out.push(json!({
            "listen": "test",
            "script": {
                "type": "text/javascript",
                "exec": tests.split('\n').collect::<Vec<_>>(),
            },
        }));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::folder::Folder;
    use crate::format::request::{KeyValue, RequestDraft};
    use crate::http::HttpMethod;
    use crate::imports::postman::from_json;
    use crate::imports::ImportItem;

    fn folder(name: &str, children: Vec<TreeNode>) -> TreeNode {
        TreeNode::Folder {
            path: std::path::PathBuf::from("/tmp"),
            name: name.to_string(),
            meta: Some(Folder::new(name)),
            children,
        }
    }

    fn request(name: &str, method: HttpMethod, url: &str) -> TreeNode {
        let draft = RequestDraft::new_rest(name, method, url);
        TreeNode::Request {
            path: std::path::PathBuf::from("/tmp/x.argos.yaml"),
            draft,
        }
    }

    #[test]
    fn emits_v21_schema_marker() {
        let tree = folder("Demo", vec![request("Ping", HttpMethod::Get, "https://x")]);
        let s = to_postman_v21_string("Demo", &tree).unwrap();
        assert!(s.contains("v2.1.0"));
        assert!(s.contains("\"name\": \"Demo\""));
    }

    #[test]
    fn nested_folders_become_nested_items() {
        let tree = folder(
            "root",
            vec![
                folder(
                    "Users",
                    vec![
                        request("List", HttpMethod::Get, "https://x/users"),
                        folder(
                            "Admins",
                            vec![request("Promote", HttpMethod::Post, "https://x/promote")],
                        ),
                    ],
                ),
                request("Health", HttpMethod::Get, "https://x/health"),
            ],
        );
        let v = to_postman_v21("Demo", &tree);
        let items = v.get("item").unwrap().as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].get("name").unwrap(), "Users");
        let inner = items[0].get("item").unwrap().as_array().unwrap();
        assert_eq!(inner.len(), 2);
        assert_eq!(inner[1].get("name").unwrap(), "Admins");
    }

    #[test]
    fn round_trip_through_import_preserves_shape() {
        let mut draft = RequestDraft::new_rest(
            "Create user",
            HttpMethod::Post,
            "https://api.example.com/users",
        );
        let RequestVariant::Rest(rest) = &mut draft.variant;
        rest.headers.push(KeyValue {
            name: "Accept".into(),
            value: "application/json".into(),
            enabled: true,
        });
        rest.body = Some(BodyDraft::Json {
            value: serde_json::json!({"name": "Alice"}),
        });
        rest.auth = Some(AuthConfig::Bearer {
            token: "{{token}}".into(),
        });
        draft.scripts.tests = Some("pm.expect(pm.response.code).to.equal(201);".to_string());

        let tree = folder(
            "API",
            vec![TreeNode::Request {
                path: std::path::PathBuf::from("/tmp/x.argos.yaml"),
                draft: draft.clone(),
            }],
        );
        let exported = to_postman_v21_string("API", &tree).unwrap();
        let reimported = from_json(&exported).unwrap();
        assert_eq!(reimported.name, "API");
        assert_eq!(reimported.items.len(), 1);
        match &reimported.items[0] {
            ImportItem::Request { draft: round } => {
                assert_eq!(round.name, "Create user");
                let RequestVariant::Rest(rr) = &round.variant;
                assert_eq!(rr.method, HttpMethod::Post);
                assert_eq!(rr.url, "https://api.example.com/users");
                assert!(rr.headers.iter().any(|h| h.name == "Accept"));
                match &rr.body {
                    Some(BodyDraft::Json { value }) => {
                        assert_eq!(value, &serde_json::json!({"name": "Alice"}));
                    }
                    other => panic!("expected json body, got {other:?}"),
                }
                match &rr.auth {
                    Some(AuthConfig::Bearer { token }) => assert_eq!(token, "{{token}}"),
                    other => panic!("expected bearer, got {other:?}"),
                }
                assert!(round
                    .scripts
                    .tests
                    .as_deref()
                    .unwrap_or("")
                    .contains("pm.expect"));
            }
            _ => panic!("expected request after round-trip"),
        }
    }

    #[test]
    fn query_params_round_trip_via_url_object() {
        let mut draft = RequestDraft::new_rest("Search", HttpMethod::Get, "https://x/search");
        let RequestVariant::Rest(rest) = &mut draft.variant;
        rest.query.push(KeyValue {
            name: "q".into(),
            value: "widgets".into(),
            enabled: true,
        });
        rest.query.push(KeyValue {
            name: "expired".into(),
            value: "1".into(),
            enabled: false,
        });
        let tree = folder(
            "root",
            vec![TreeNode::Request {
                path: std::path::PathBuf::from("/tmp/y.argos.yaml"),
                draft,
            }],
        );
        let s = to_postman_v21_string("root", &tree).unwrap();
        let reimported = from_json(&s).unwrap();
        let ImportItem::Request { draft: r } = &reimported.items[0] else {
            panic!();
        };
        let RequestVariant::Rest(rr) = &r.variant;
        assert_eq!(rr.query.len(), 2);
        assert!(rr.query.iter().any(|q| q.name == "q" && q.enabled));
        assert!(rr.query.iter().any(|q| q.name == "expired" && !q.enabled));
    }
}
