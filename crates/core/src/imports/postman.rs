//! Postman v2.1 collection importer.
//!
//! We don't generate strongly-typed Postman structs — the schema has too
//! many optional fields and quirks. Instead we walk the JSON via
//! [`serde_json::Value`] and pick out the bits we care about. Anything
//! we don't understand is silently ignored so a partially-broken
//! collection still imports the requests we *do* understand.

#![allow(clippy::match_wildcard_for_single_variants)]

use serde_json::Value;

use crate::format::request::{
    ApiKeyLocation, AuthConfig, BodyDraft, FormField, KeyValue, RequestDraft, RequestVariant,
    RestRequest, ScriptHooks,
};
use crate::http::HttpMethod;

use super::{ImportItem, ImportedCollection};

/// Errors produced by [`from_json`].
#[derive(Debug, thiserror::Error)]
pub enum PostmanImportError {
    /// Input is not valid JSON.
    #[error("invalid JSON: {0}")]
    InvalidJson(String),
    /// Top-level shape doesn't look like a Postman v2.1 collection.
    #[error(
        "not a Postman v2.1 collection: missing or unsupported `info.schema` (expected v2.1.0)"
    )]
    NotPostmanV21,
}

/// Parse a Postman v2.1 JSON collection into an [`ImportedCollection`].
///
/// Older v2.0 collections are *not* supported — we error out so the
/// host can offer to upgrade rather than silently produce a broken
/// import.
///
/// # Errors
///
/// [`PostmanImportError::InvalidJson`] for malformed JSON;
/// [`PostmanImportError::NotPostmanV21`] when the input isn't a
/// recognisable v2.1 collection.
pub fn from_json(input: &str) -> Result<ImportedCollection, PostmanImportError> {
    let v: Value =
        serde_json::from_str(input).map_err(|e| PostmanImportError::InvalidJson(e.to_string()))?;

    let info = v.get("info").and_then(Value::as_object);
    let schema = info
        .and_then(|m| m.get("schema"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    // Strict-ish: must mention "v2.1" somewhere. v2.0.0 schemas use a
    // different URL and v2.1 alphas use ".../collection.json".
    if !schema.contains("v2.1") {
        return Err(PostmanImportError::NotPostmanV21);
    }

    let name = info
        .and_then(|m| m.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("Imported collection")
        .to_string();
    let description = info
        .and_then(|m| m.get("description"))
        .map(stringify_description);

    let items = v
        .get("item")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(map_item).collect::<Vec<_>>())
        .unwrap_or_default();

    let variables = v
        .get("variable")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    let key = v.get("key").and_then(Value::as_str).map(str::to_string)?;
                    let val = v.get("value").map(stringify_scalar).unwrap_or_default();
                    Some((key, val))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(ImportedCollection {
        name,
        description,
        items,
        variables,
    })
}

fn stringify_description(v: &Value) -> String {
    // Postman descriptions can be a plain string OR `{ content, type }`.
    if let Some(s) = v.as_str() {
        return s.to_string();
    }
    if let Some(s) = v.get("content").and_then(Value::as_str) {
        return s.to_string();
    }
    String::new()
}

fn stringify_scalar(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn map_item(item: &Value) -> Option<ImportItem> {
    let name = item
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Untitled")
        .to_string();

    // Folder: has `item: [...]` and no `request`.
    if let Some(children) = item.get("item").and_then(Value::as_array) {
        let description = item.get("description").map(stringify_description);
        return Some(ImportItem::Folder {
            name,
            description,
            items: children.iter().filter_map(map_item).collect(),
        });
    }

    let req_value = item.get("request")?;
    let draft = map_request(&name, item, req_value)?;
    Some(ImportItem::Request { draft })
}

fn map_request(name: &str, item: &Value, req: &Value) -> Option<RequestDraft> {
    let (method, url, headers, body, auth) = match req {
        Value::String(url) => (HttpMethod::Get, url.clone(), Vec::new(), None, None),
        Value::Object(_) => {
            let method = req
                .get("method")
                .and_then(Value::as_str)
                .map_or(HttpMethod::Get, parse_method);
            let (raw_url, query) = parse_url(req.get("url"));
            let mut headers: Vec<KeyValue> = req
                .get("header")
                .and_then(Value::as_array)
                .map(|arr| arr.iter().filter_map(map_header).collect())
                .unwrap_or_default();
            let body = map_body(req.get("body"));
            let auth = map_auth(req.get("auth"));

            // Merge URL-level query params back into the draft as `query`.
            let mut q_kvs: Vec<KeyValue> = query;
            // Postman header entries with `disabled: true` become enabled=false.
            // (already handled in map_header via the `disabled` flag.)
            if !q_kvs.is_empty() {
                // No-op, kept for symmetry / future extension.
                q_kvs = std::mem::take(&mut q_kvs);
            }
            // Headers attached at the request level only (folder-level
            // inheritance is left to the host).
            headers.retain(|h| !h.name.is_empty());

            return Some(RequestDraft {
                kind: crate::format::Kind::Request,
                name: name.to_string(),
                description: item.get("description").map(stringify_description),
                variant: RequestVariant::Rest(RestRequest {
                    method,
                    url: raw_url,
                    query: q_kvs,
                    headers,
                    auth,
                    body,
                }),
                scripts: extract_scripts(item),
                schema_ref: None,
            });
        }
        _ => return None,
    };

    Some(RequestDraft {
        kind: crate::format::Kind::Request,
        name: name.to_string(),
        description: item.get("description").map(stringify_description),
        variant: RequestVariant::Rest(RestRequest {
            method,
            url,
            query: Vec::new(),
            headers,
            auth,
            body,
        }),
        scripts: extract_scripts(item),
        schema_ref: None,
    })
}

fn parse_method(s: &str) -> HttpMethod {
    match s.to_ascii_uppercase().as_str() {
        "POST" => HttpMethod::Post,
        "PUT" => HttpMethod::Put,
        "PATCH" => HttpMethod::Patch,
        "DELETE" => HttpMethod::Delete,
        "HEAD" => HttpMethod::Head,
        "OPTIONS" => HttpMethod::Options,
        _ => HttpMethod::Get,
    }
}

/// Returns the resolved URL string + a list of key-value query entries.
///
/// Postman's URL field comes in two flavours:
/// - A bare string (we use it verbatim, query params stay inline).
/// - An object with `raw`, `host`, `path`, `query`, `variable`. We
///   prefer `raw` so user-typed templating like `{{baseUrl}}` survives;
///   the `query` array is lifted out into our `KeyValue` representation
///   (matches what the editor expects).
fn parse_url(v: Option<&Value>) -> (String, Vec<KeyValue>) {
    let Some(v) = v else {
        return (String::new(), Vec::new());
    };
    if let Some(s) = v.as_str() {
        return (s.to_string(), Vec::new());
    }
    let raw = v
        .get("raw")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| reconstruct_url(v))
        .unwrap_or_default();
    let mut url = raw;
    let mut query: Vec<KeyValue> = v
        .get("query")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|q| {
                    let key = q.get("key").and_then(Value::as_str)?.to_string();
                    if key.is_empty() {
                        return None;
                    }
                    let value = q.get("value").map(stringify_scalar).unwrap_or_default();
                    let enabled = q
                        .get("disabled")
                        .and_then(Value::as_bool)
                        .map_or(true, |d| !d);
                    Some(KeyValue {
                        name: key,
                        value,
                        enabled,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // If we lifted query into our list, strip them from the raw URL so
    // we don't double-send.
    if !query.is_empty() {
        if let Some(idx) = url.find('?') {
            url.truncate(idx);
        }
    }

    // Path-level `:variable` placeholders → `{{var}}` so they resolve
    // through Argos's templating instead of being sent literally.
    if let Some(vars) = v.get("variable").and_then(Value::as_array) {
        for var in vars {
            if let Some(key) = var.get("key").and_then(Value::as_str) {
                let needle = format!(":{key}");
                let replacement = format!("{{{{{key}}}}}");
                if url.contains(&needle) {
                    url = url.replace(&needle, &replacement);
                }
                // Also seed a query var if Postman provided a default.
                let value = var.get("value").map(stringify_scalar).unwrap_or_default();
                if !value.is_empty()
                    && !query.iter().any(|q| q.name == key)
                    && !url.contains(&format!("{{{{{key}}}}}"))
                {
                    query.push(KeyValue {
                        name: key.to_string(),
                        value,
                        enabled: true,
                    });
                }
            }
        }
    }

    (url, query)
}

fn reconstruct_url(v: &Value) -> Option<String> {
    let host = v
        .get("host")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(".")
        })
        .filter(|s| !s.is_empty())?;
    let path = v
        .get("path")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("/")
        })
        .unwrap_or_default();
    let scheme = v.get("protocol").and_then(Value::as_str).unwrap_or("https");
    if path.is_empty() {
        Some(format!("{scheme}://{host}"))
    } else {
        Some(format!("{scheme}://{host}/{path}"))
    }
}

fn map_header(h: &Value) -> Option<KeyValue> {
    let key = h.get("key").and_then(Value::as_str)?.to_string();
    if key.is_empty() {
        return None;
    }
    let value = h.get("value").map(stringify_scalar).unwrap_or_default();
    let enabled = h
        .get("disabled")
        .and_then(Value::as_bool)
        .map_or(true, |d| !d);
    Some(KeyValue {
        name: key,
        value,
        enabled,
    })
}

fn map_body(v: Option<&Value>) -> Option<BodyDraft> {
    let body = v?;
    let mode = body.get("mode").and_then(Value::as_str)?;
    match mode {
        "raw" => {
            let content = body
                .get("raw")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            // Postman tags raw bodies with options.raw.language = "json"|"text"|"javascript"|...
            let language = body
                .get("options")
                .and_then(|o| o.get("raw"))
                .and_then(|r| r.get("language"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if language == "json" {
                if let Ok(parsed) = serde_json::from_str::<Value>(&content) {
                    return Some(BodyDraft::Json { value: parsed });
                }
            }
            Some(BodyDraft::Text {
                content,
                content_type: match language {
                    "json" => "application/json".to_string(),
                    "xml" => "application/xml".to_string(),
                    "html" => "text/html".to_string(),
                    "javascript" => "application/javascript".to_string(),
                    _ => "text/plain".to_string(),
                },
            })
        }
        "urlencoded" => {
            let fields = body
                .get("urlencoded")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|f| {
                            let name = f.get("key").and_then(Value::as_str)?.to_string();
                            if name.is_empty() {
                                return None;
                            }
                            let value = f.get("value").map(stringify_scalar).unwrap_or_default();
                            let enabled = f
                                .get("disabled")
                                .and_then(Value::as_bool)
                                .map_or(true, |d| !d);
                            Some(FormField {
                                name,
                                value,
                                enabled,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some(BodyDraft::FormUrlEncoded { fields })
        }
        "formdata" => {
            // Argos doesn't support multipart yet (HttpBody::Multipart is
            // explicitly deferred). Fall back to a urlencoded shape so
            // the import is still useful for text-only forms — file
            // entries are dropped with a placeholder note in the value.
            let fields = body
                .get("formdata")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|f| {
                            let name = f.get("key").and_then(Value::as_str)?.to_string();
                            if name.is_empty() {
                                return None;
                            }
                            let kind = f.get("type").and_then(Value::as_str).unwrap_or("text");
                            let value = if kind == "file" {
                                "<file upload not yet supported>".to_string()
                            } else {
                                f.get("value").map(stringify_scalar).unwrap_or_default()
                            };
                            Some(FormField {
                                name,
                                value,
                                enabled: true,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some(BodyDraft::FormUrlEncoded { fields })
        }
        "graphql" => {
            // Wrap the GraphQL body as JSON `{ query, variables }` so a
            // REST send still produces a sensible request — once the
            // GraphQL request type lands (E5) the importer can prefer it.
            let body_obj = body.get("graphql").cloned().unwrap_or(Value::Null);
            Some(BodyDraft::Json { value: body_obj })
        }
        _ => None,
    }
}

fn map_auth(v: Option<&Value>) -> Option<AuthConfig> {
    let auth = v?;
    let kind = auth.get("type").and_then(Value::as_str)?;
    match kind {
        "bearer" => {
            let token = pick_first(auth.get("bearer"), "token");
            Some(AuthConfig::Bearer { token })
        }
        "basic" => {
            let username = pick_first(auth.get("basic"), "username");
            let password = pick_first(auth.get("basic"), "password");
            Some(AuthConfig::Basic { username, password })
        }
        "apikey" => {
            let name = pick_first(auth.get("apikey"), "key");
            let value = pick_first(auth.get("apikey"), "value");
            let location = pick_first(auth.get("apikey"), "in");
            let location = match location.as_str() {
                "query" => ApiKeyLocation::Query,
                "cookie" => ApiKeyLocation::Cookie,
                _ => ApiKeyLocation::Header,
            };
            Some(AuthConfig::ApiKey {
                name,
                value,
                location,
            })
        }
        _ => None,
    }
}

/// Postman stores auth fields as `[{key, value, type}]` arrays. Find
/// the entry whose `key` matches and return its `value` as a string.
fn pick_first(v: Option<&Value>, key: &str) -> String {
    let Some(arr) = v.and_then(Value::as_array) else {
        return String::new();
    };
    for entry in arr {
        if entry.get("key").and_then(Value::as_str) == Some(key) {
            return entry.get("value").map(stringify_scalar).unwrap_or_default();
        }
    }
    String::new()
}

fn extract_scripts(item: &Value) -> ScriptHooks {
    let Some(events) = item.get("event").and_then(Value::as_array) else {
        return ScriptHooks::default();
    };
    let mut hooks = ScriptHooks::default();
    for ev in events {
        let listen = ev.get("listen").and_then(Value::as_str).unwrap_or_default();
        let script = ev.get("script").and_then(Value::as_object);
        let lines = script
            .and_then(|m| m.get("exec"))
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        if lines.trim().is_empty() {
            continue;
        }
        match listen {
            "prerequest" => hooks.pre_request = Some(lines),
            "test" => hooks.tests = Some(lines),
            _ => {}
        }
    }
    hooks
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn collection(items: &Value) -> String {
        json!({
            "info": {
                "name": "Demo",
                "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
            },
            "item": items
        })
        .to_string()
    }

    #[test]
    fn rejects_v20_collection() {
        let s = json!({
            "info": {
                "name": "Old",
                "schema": "https://schema.getpostman.com/json/collection/v2.0.0/collection.json"
            },
            "item": []
        })
        .to_string();
        let err = from_json(&s).unwrap_err();
        assert!(matches!(err, PostmanImportError::NotPostmanV21));
    }

    #[test]
    fn rejects_garbage_input() {
        let err = from_json("not json").unwrap_err();
        assert!(matches!(err, PostmanImportError::InvalidJson(_)));
    }

    #[test]
    fn imports_simple_get_with_string_url() {
        let s = collection(&json!([{
            "name": "List users",
            "request": "https://api.example.com/users"
        }]));
        let c = from_json(&s).unwrap();
        assert_eq!(c.items.len(), 1);
        match &c.items[0] {
            ImportItem::Request { draft } => {
                assert_eq!(draft.name, "List users");
                let RequestVariant::Rest(rest) = &draft.variant else { panic!("expected REST variant"); };
                assert_eq!(rest.method, HttpMethod::Get);
                assert_eq!(rest.url, "https://api.example.com/users");
            }
            _ => panic!("expected request"),
        }
    }

    #[test]
    fn imports_request_with_object_url_and_query() {
        let s = collection(&json!([{
            "name": "Search",
            "request": {
                "method": "GET",
                "header": [
                    { "key": "Accept", "value": "application/json" },
                    { "key": "X-Off", "value": "yes", "disabled": true }
                ],
                "url": {
                    "raw": "https://api.example.com/search?q=widgets&page=2",
                    "host": ["api","example","com"],
                    "path": ["search"],
                    "query": [
                        { "key": "q", "value": "widgets" },
                        { "key": "page", "value": "2" },
                        { "key": "expired", "value": "1", "disabled": true }
                    ]
                }
            }
        }]));
        let c = from_json(&s).unwrap();
        let ImportItem::Request { draft } = &c.items[0] else {
            panic!("expected request");
        };
        let RequestVariant::Rest(rest) = &draft.variant else { panic!("expected REST variant"); };
        assert_eq!(rest.url, "https://api.example.com/search");
        assert_eq!(rest.query.len(), 3);
        assert_eq!(rest.query[0].name, "q");
        assert!(rest.query[0].enabled);
        assert!(!rest.query[2].enabled);
        assert_eq!(rest.headers.len(), 2);
        assert!(rest.headers.iter().any(|h| h.name == "Accept" && h.enabled));
        assert!(rest.headers.iter().any(|h| h.name == "X-Off" && !h.enabled));
    }

    #[test]
    fn imports_path_variables_as_template_placeholders() {
        let s = collection(&json!([{
            "name": "Get user",
            "request": {
                "method": "GET",
                "url": {
                    "raw": "https://api.example.com/users/:userId",
                    "host": ["api","example","com"],
                    "path": ["users", ":userId"],
                    "variable": [{ "key": "userId", "value": "42" }]
                }
            }
        }]));
        let c = from_json(&s).unwrap();
        let ImportItem::Request { draft } = &c.items[0] else {
            panic!();
        };
        let RequestVariant::Rest(rest) = &draft.variant else { panic!("expected REST variant"); };
        assert_eq!(rest.url, "https://api.example.com/users/{{userId}}");
    }

    #[test]
    fn imports_json_body_via_options_language() {
        let s = collection(&json!([{
            "name": "Create user",
            "request": {
                "method": "POST",
                "url": "https://api.example.com/users",
                "body": {
                    "mode": "raw",
                    "raw": "{\"name\":\"Alice\",\"n\":3}",
                    "options": { "raw": { "language": "json" } }
                }
            }
        }]));
        let c = from_json(&s).unwrap();
        let ImportItem::Request { draft } = &c.items[0] else {
            panic!();
        };
        let RequestVariant::Rest(rest) = &draft.variant else { panic!("expected REST variant"); };
        assert_eq!(rest.method, HttpMethod::Post);
        match &rest.body {
            Some(BodyDraft::Json { value }) => {
                assert_eq!(value, &json!({"name": "Alice", "n": 3}));
            }
            other => panic!("expected json body, got {other:?}"),
        }
    }

    #[test]
    fn imports_urlencoded_body() {
        let s = collection(&json!([{
            "name": "Login",
            "request": {
                "method": "POST",
                "url": "https://api.example.com/login",
                "body": {
                    "mode": "urlencoded",
                    "urlencoded": [
                        { "key": "user", "value": "alice" },
                        { "key": "pass", "value": "s3cret" },
                        { "key": "remember", "value": "1", "disabled": true }
                    ]
                }
            }
        }]));
        let c = from_json(&s).unwrap();
        let ImportItem::Request { draft } = &c.items[0] else {
            panic!();
        };
        let RequestVariant::Rest(rest) = &draft.variant else { panic!("expected REST variant"); };
        match &rest.body {
            Some(BodyDraft::FormUrlEncoded { fields }) => {
                assert_eq!(fields.len(), 3);
                assert!(fields.iter().any(|f| f.name == "user" && f.enabled));
                assert!(fields.iter().any(|f| f.name == "remember" && !f.enabled));
            }
            other => panic!("expected form body, got {other:?}"),
        }
    }

    #[test]
    fn imports_bearer_auth() {
        let s = collection(&json!([{
            "name": "Me",
            "request": {
                "method": "GET",
                "url": "https://api.example.com/me",
                "auth": {
                    "type": "bearer",
                    "bearer": [{ "key": "token", "value": "{{token}}" }]
                }
            }
        }]));
        let c = from_json(&s).unwrap();
        let ImportItem::Request { draft } = &c.items[0] else {
            panic!();
        };
        let RequestVariant::Rest(rest) = &draft.variant else { panic!("expected REST variant"); };
        match &rest.auth {
            Some(AuthConfig::Bearer { token }) => assert_eq!(token, "{{token}}"),
            other => panic!("expected bearer, got {other:?}"),
        }
    }

    #[test]
    fn imports_basic_and_apikey_auth() {
        let s = collection(&json!([
            {
                "name": "Basic",
                "request": {
                    "method": "GET",
                    "url": "https://x",
                    "auth": {
                        "type": "basic",
                        "basic": [
                            { "key": "username", "value": "u" },
                            { "key": "password", "value": "p" }
                        ]
                    }
                }
            },
            {
                "name": "Apikey",
                "request": {
                    "method": "GET",
                    "url": "https://x",
                    "auth": {
                        "type": "apikey",
                        "apikey": [
                            { "key": "key", "value": "X-Key" },
                            { "key": "value", "value": "abc" },
                            { "key": "in", "value": "query" }
                        ]
                    }
                }
            }
        ]));
        let c = from_json(&s).unwrap();
        let ImportItem::Request { draft: a } = &c.items[0] else {
            panic!();
        };
        let RequestVariant::Rest(ra) = &a.variant else { panic!("expected REST variant"); };
        assert!(matches!(
            ra.auth,
            Some(AuthConfig::Basic { ref username, ref password }) if username == "u" && password == "p"
        ));
        let ImportItem::Request { draft: b } = &c.items[1] else {
            panic!();
        };
        let RequestVariant::Rest(rb) = &b.variant else { panic!("expected REST variant"); };
        assert!(matches!(
            rb.auth,
            Some(AuthConfig::ApiKey { location: ApiKeyLocation::Query, ref name, ref value })
                if name == "X-Key" && value == "abc"
        ));
    }

    #[test]
    fn imports_pre_request_and_test_scripts() {
        let s = collection(&json!([{
            "name": "Hooked",
            "request": "https://x/y",
            "event": [
                {
                    "listen": "prerequest",
                    "script": { "exec": ["pm.environment.set('ts', Date.now());"], "type": "text/javascript" }
                },
                {
                    "listen": "test",
                    "script": {
                        "exec": [
                            "pm.test('ok', () => {",
                            "  pm.expect(pm.response.code).to.equal(200);",
                            "});"
                        ]
                    }
                }
            ]
        }]));
        let c = from_json(&s).unwrap();
        let ImportItem::Request { draft } = &c.items[0] else {
            panic!();
        };
        let pre = draft.scripts.pre_request.as_deref().unwrap();
        let tests = draft.scripts.tests.as_deref().unwrap();
        assert!(pre.contains("pm.environment.set"));
        assert!(tests.contains("pm.test('ok'"));
        assert!(tests.contains("pm.expect"));
    }

    #[test]
    fn imports_nested_folders() {
        let s = collection(&json!([
            {
                "name": "Users",
                "item": [
                    { "name": "List", "request": "https://x/users" },
                    {
                        "name": "Admins",
                        "item": [
                            { "name": "Promote", "request": "https://x/users/promote" }
                        ]
                    }
                ]
            },
            { "name": "Health", "request": "https://x/health" }
        ]));
        let c = from_json(&s).unwrap();
        assert_eq!(c.items.len(), 2);
        match &c.items[0] {
            ImportItem::Folder { name, items, .. } => {
                assert_eq!(name, "Users");
                assert_eq!(items.len(), 2);
                match &items[1] {
                    ImportItem::Folder {
                        name: inner,
                        items: deep,
                        ..
                    } => {
                        assert_eq!(inner, "Admins");
                        assert_eq!(deep.len(), 1);
                    }
                    _ => panic!("expected nested folder"),
                }
            }
            _ => panic!("expected folder"),
        }
        match &c.items[1] {
            ImportItem::Request { draft } => assert_eq!(draft.name, "Health"),
            _ => panic!("expected request"),
        }
    }

    #[test]
    fn imports_collection_variables() {
        let s = json!({
            "info": {
                "name": "Vars",
                "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
            },
            "item": [],
            "variable": [
                { "key": "baseUrl", "value": "https://api.example.com" },
                { "key": "version", "value": "v2" }
            ]
        })
        .to_string();
        let c = from_json(&s).unwrap();
        assert_eq!(c.variables.len(), 2);
        assert!(c
            .variables
            .iter()
            .any(|(k, v)| k == "baseUrl" && v == "https://api.example.com"));
    }
}
