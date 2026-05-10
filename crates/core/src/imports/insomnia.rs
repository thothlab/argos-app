//! Insomnia v4 export importer.
//!
//! Insomnia exports are flat: a single `resources` array where each
//! entry carries a `_type` (`workspace`, `request_group`, `request`,
//! `environment`) and a `parentId` that links it to its container. We
//! pivot the flat array into a tree on the fly.
//!
//! Only the request types Argos currently models (REST) are
//! materialised; GraphQL / WebSocket / gRPC entries are skipped with
//! a silent drop — they land properly when those request variants
//! exist.

#![allow(clippy::match_wildcard_for_single_variants)]

use std::collections::HashMap;

use serde_json::Value;

use crate::format::request::{
    ApiKeyLocation, AuthConfig, BodyDraft, FormField, KeyValue, RequestDraft, RequestVariant,
    RestRequest, ScriptHooks,
};
use crate::http::HttpMethod;

use super::{ImportItem, ImportedCollection};

/// Errors produced by [`from_json`].
#[derive(Debug, thiserror::Error)]
pub enum InsomniaImportError {
    /// Input is not valid JSON.
    #[error("invalid JSON: {0}")]
    InvalidJson(String),
    /// Missing `_type: "export"` or `__export_format` not understood.
    #[error("not an Insomnia v4 export (expected `_type: export`, `__export_format: 4`)")]
    NotInsomniaV4,
}

/// Parse an Insomnia v4 export JSON into an [`ImportedCollection`].
///
/// # Errors
///
/// [`InsomniaImportError::InvalidJson`] for malformed JSON;
/// [`InsomniaImportError::NotInsomniaV4`] when the envelope doesn't
/// declare itself as v4.
pub fn from_json(input: &str) -> Result<ImportedCollection, InsomniaImportError> {
    let v: Value =
        serde_json::from_str(input).map_err(|e| InsomniaImportError::InvalidJson(e.to_string()))?;

    let ty = v.get("_type").and_then(Value::as_str).unwrap_or_default();
    let fmt = v
        .get("__export_format")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    if ty != "export" || fmt != 4 {
        return Err(InsomniaImportError::NotInsomniaV4);
    }

    let resources = v
        .get("resources")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    // Find the workspace (or fall back to the first folder).
    let workspace = resources
        .iter()
        .find(|r| r.get("_type").and_then(Value::as_str) == Some("workspace"))
        .cloned();
    let collection_name = workspace
        .as_ref()
        .and_then(|w| w.get("name").and_then(Value::as_str))
        .unwrap_or("Imported collection")
        .to_string();
    let description = workspace
        .as_ref()
        .and_then(|w| w.get("description").and_then(Value::as_str))
        .map(str::to_string)
        .filter(|s| !s.is_empty());
    let workspace_id = workspace
        .as_ref()
        .and_then(|w| w.get("_id").and_then(Value::as_str))
        .map(str::to_string);

    // Build a child-of map: parentId -> Vec<resource index>.
    let mut by_parent: HashMap<String, Vec<usize>> = HashMap::new();
    for (idx, r) in resources.iter().enumerate() {
        let parent = r
            .get("parentId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        by_parent.entry(parent).or_default().push(idx);
    }

    let roots: Vec<&Value> = workspace_id
        .as_deref()
        .and_then(|id| by_parent.get(id))
        .map(|idxs| idxs.iter().map(|&i| &resources[i]).collect())
        .unwrap_or_default();

    // Preserve `metaSortKey` order (Insomnia uses it for drag-n-drop
    // ordering); fall back to source order otherwise.
    let items = roots
        .into_iter()
        .flat_map(|r| materialise(r, &resources, &by_parent))
        .collect();

    let variables = workspace_id
        .as_deref()
        .map(|id| extract_environment_vars(&resources, id))
        .unwrap_or_default();

    Ok(ImportedCollection {
        name: collection_name,
        description,
        items,
        variables,
    })
}

fn extract_environment_vars(resources: &[Value], workspace_id: &str) -> Vec<(String, String)> {
    // The "base environment" is parented directly under the workspace
    // and is the canonical place for collection variables.
    let env = resources.iter().find(|r| {
        r.get("_type").and_then(Value::as_str) == Some("environment")
            && r.get("parentId").and_then(Value::as_str) == Some(workspace_id)
    });
    let Some(env) = env else {
        return Vec::new();
    };
    let Some(map) = env.get("data").and_then(Value::as_object) else {
        return Vec::new();
    };
    map.iter()
        .map(|(k, v)| (k.clone(), scalar_to_string(v)))
        .collect()
}

fn scalar_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn materialise(
    res: &Value,
    all: &[Value],
    by_parent: &HashMap<String, Vec<usize>>,
) -> Vec<ImportItem> {
    let kind = res.get("_type").and_then(Value::as_str).unwrap_or_default();
    match kind {
        "request_group" => {
            let id = res.get("_id").and_then(Value::as_str).unwrap_or_default();
            let name = res
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Untitled")
                .to_string();
            let description = res
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string)
                .filter(|s| !s.is_empty());
            let children = by_parent
                .get(id)
                .map(|idxs| {
                    let mut sorted: Vec<&Value> = idxs.iter().map(|&i| &all[i]).collect();
                    sort_by_meta(&mut sorted);
                    sorted
                        .into_iter()
                        .flat_map(|r| materialise(r, all, by_parent))
                        .collect()
                })
                .unwrap_or_default();
            vec![ImportItem::Folder {
                name,
                description,
                items: children,
            }]
        }
        "request" => {
            let draft = map_request(res);
            vec![ImportItem::Request { draft }]
        }
        _ => Vec::new(),
    }
}

fn sort_by_meta(list: &mut [&Value]) {
    list.sort_by_key(|r| {
        r.get("metaSortKey")
            .and_then(Value::as_i64)
            .unwrap_or(i64::MAX)
    });
}

fn map_request(req: &Value) -> RequestDraft {
    let name = req
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Untitled")
        .to_string();
    let method = req
        .get("method")
        .and_then(Value::as_str)
        .map_or(HttpMethod::Get, parse_method);

    // Insomnia stores URL + parameters separately. We append enabled
    // params to the URL via the structured `query` list, matching how
    // Argos's own editor surfaces them.
    let url = req
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    // Convert ${{var}} or {{var}} — Insomnia uses {{ _.varName }}
    // canonically. Strip the leading "_." so our resolver finds the
    // variable directly.
    let url = normalise_template(&url);

    let query: Vec<KeyValue> = req
        .get("parameters")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|p| {
                    let name = p.get("name").and_then(Value::as_str)?.to_string();
                    if name.is_empty() {
                        return None;
                    }
                    let value = p.get("value").map(scalar_to_string).unwrap_or_default();
                    let enabled = p
                        .get("disabled")
                        .and_then(Value::as_bool)
                        .map_or(true, |d| !d);
                    Some(KeyValue {
                        name,
                        value: normalise_template(&value),
                        enabled,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let headers: Vec<KeyValue> = req
        .get("headers")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|h| {
                    let name = h.get("name").and_then(Value::as_str)?.to_string();
                    if name.is_empty() {
                        return None;
                    }
                    let value = h.get("value").map(scalar_to_string).unwrap_or_default();
                    let enabled = h
                        .get("disabled")
                        .and_then(Value::as_bool)
                        .map_or(true, |d| !d);
                    Some(KeyValue {
                        name,
                        value: normalise_template(&value),
                        enabled,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let body = map_body(req.get("body"));
    let auth = map_auth(req.get("authentication"));

    RequestDraft {
        kind: crate::format::Kind::Request,
        name,
        description: req
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|s| !s.is_empty()),
        variant: RequestVariant::Rest(RestRequest {
            method,
            url,
            query,
            headers,
            auth,
            body,
        }),
        scripts: ScriptHooks::default(),
        schema_ref: None,
    }
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

/// `{{ _.foo }}` → `{{foo}}` so Argos's resolver matches the variable.
/// Plain `{{foo}}` and arbitrary text pass through unchanged.
fn normalise_template(input: &str) -> String {
    if !input.contains("_.") && !input.contains("{{") {
        return input.to_string();
    }
    // Cheap two-pass replace: handle `{{ _.name }}` → `{{name}}` and
    // also the no-space variant `{{_.name}}`. Anything else passes
    // through.
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            if let Some(end) = find_close(bytes, i + 2) {
                let inner = std::str::from_utf8(&bytes[i + 2..end]).unwrap_or("").trim();
                let stripped = inner.strip_prefix("_.").unwrap_or(inner).trim();
                out.push_str("{{");
                out.push_str(stripped);
                out.push_str("}}");
                i = end + 2;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn find_close(bytes: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    while i + 1 < bytes.len() {
        if bytes[i] == b'}' && bytes[i + 1] == b'}' {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn map_body(v: Option<&Value>) -> Option<BodyDraft> {
    let body = v?;
    if body.is_null() {
        return None;
    }
    let obj = body.as_object()?;
    if obj.is_empty() {
        return None;
    }
    let mime = obj
        .get("mimeType")
        .and_then(Value::as_str)
        .unwrap_or_default();

    if mime == "application/x-www-form-urlencoded" {
        let fields = obj
            .get("params")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|p| {
                        let name = p.get("name").and_then(Value::as_str)?.to_string();
                        if name.is_empty() {
                            return None;
                        }
                        let value = p.get("value").map(scalar_to_string).unwrap_or_default();
                        let enabled = p
                            .get("disabled")
                            .and_then(Value::as_bool)
                            .map_or(true, |d| !d);
                        Some(FormField {
                            name,
                            value: normalise_template(&value),
                            enabled,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        return Some(BodyDraft::FormUrlEncoded { fields });
    }

    if mime.starts_with("multipart/") {
        // Multipart isn't first-class yet — downgrade to form fields
        // and replace file entries with a placeholder so the user
        // notices.
        let fields = obj
            .get("params")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|p| {
                        let name = p.get("name").and_then(Value::as_str)?.to_string();
                        if name.is_empty() {
                            return None;
                        }
                        let file_name = p.get("fileName").and_then(Value::as_str);
                        let value = if let Some(f) = file_name {
                            format!("<file upload: {f}>")
                        } else {
                            p.get("value").map(scalar_to_string).unwrap_or_default()
                        };
                        Some(FormField {
                            name,
                            value: normalise_template(&value),
                            enabled: true,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        return Some(BodyDraft::FormUrlEncoded { fields });
    }

    let text = obj.get("text").and_then(Value::as_str).unwrap_or_default();
    if mime == "application/json" {
        if let Ok(value) = serde_json::from_str::<Value>(text) {
            return Some(BodyDraft::Json { value });
        }
    }
    let resolved = normalise_template(text);
    let ct = if mime.is_empty() {
        "text/plain".to_string()
    } else {
        mime.to_string()
    };
    Some(BodyDraft::Text {
        content: resolved,
        content_type: ct,
    })
}

fn map_auth(v: Option<&Value>) -> Option<AuthConfig> {
    let auth = v?;
    let obj = auth.as_object()?;
    let kind = obj.get("type").and_then(Value::as_str)?;
    // Insomnia uses `disabled: true` to keep an auth config around
    // without applying it.
    if obj.get("disabled").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    match kind {
        "bearer" => {
            let token = obj.get("token").map(scalar_to_string).unwrap_or_default();
            Some(AuthConfig::Bearer {
                token: normalise_template(&token),
            })
        }
        "basic" => {
            let username = obj
                .get("username")
                .map(scalar_to_string)
                .unwrap_or_default();
            let password = obj
                .get("password")
                .map(scalar_to_string)
                .unwrap_or_default();
            Some(AuthConfig::Basic {
                username: normalise_template(&username),
                password: normalise_template(&password),
            })
        }
        "apikey" => {
            let name = obj.get("key").map(scalar_to_string).unwrap_or_default();
            let value = obj.get("value").map(scalar_to_string).unwrap_or_default();
            let add_to = obj.get("addTo").and_then(Value::as_str).unwrap_or("header");
            let location = match add_to {
                "queryParams" => ApiKeyLocation::Query,
                "cookie" => ApiKeyLocation::Cookie,
                _ => ApiKeyLocation::Header,
            };
            Some(AuthConfig::ApiKey {
                name,
                value: normalise_template(&value),
                location,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn export(resources: &Value) -> String {
        json!({
            "_type": "export",
            "__export_format": 4,
            "__export_source": "insomnia",
            "resources": resources,
        })
        .to_string()
    }

    #[test]
    fn rejects_non_export_envelope() {
        let err = from_json("{\"_type\":\"workspace\"}").unwrap_err();
        assert!(matches!(err, InsomniaImportError::NotInsomniaV4));
    }

    #[test]
    fn rejects_other_format_versions() {
        let s = json!({"_type":"export","__export_format":3,"resources":[]}).to_string();
        let err = from_json(&s).unwrap_err();
        assert!(matches!(err, InsomniaImportError::NotInsomniaV4));
    }

    #[test]
    fn imports_flat_workspace_with_requests() {
        let s = export(&json!([
            { "_id": "wrk_1", "_type": "workspace", "name": "Demo" },
            { "_id": "r1", "_type": "request", "parentId": "wrk_1",
              "name": "Ping", "method": "GET", "url": "https://x/health" },
            { "_id": "r2", "_type": "request", "parentId": "wrk_1",
              "name": "Echo", "method": "POST",
              "url": "https://x/echo",
              "headers": [{ "name": "Accept", "value": "application/json" }] }
        ]));
        let c = from_json(&s).unwrap();
        assert_eq!(c.name, "Demo");
        assert_eq!(c.items.len(), 2);
        match &c.items[0] {
            ImportItem::Request { draft } => {
                assert_eq!(draft.name, "Ping");
                let RequestVariant::Rest(rest) = &draft.variant;
                assert_eq!(rest.method, HttpMethod::Get);
            }
            _ => panic!("expected request"),
        }
    }

    #[test]
    fn imports_nested_folder_under_request_group() {
        let s = export(&json!([
            { "_id": "wrk_1", "_type": "workspace", "name": "Demo" },
            { "_id": "g_users", "_type": "request_group", "parentId": "wrk_1", "name": "Users" },
            { "_id": "r_list", "_type": "request", "parentId": "g_users",
              "name": "List", "method": "GET", "url": "https://x/users" },
            { "_id": "g_admins", "_type": "request_group", "parentId": "g_users", "name": "Admins" },
            { "_id": "r_promote", "_type": "request", "parentId": "g_admins",
              "name": "Promote", "method": "POST", "url": "https://x/users/promote" }
        ]));
        let c = from_json(&s).unwrap();
        assert_eq!(c.items.len(), 1);
        match &c.items[0] {
            ImportItem::Folder { name, items, .. } => {
                assert_eq!(name, "Users");
                assert_eq!(items.len(), 2);
                let nested = items
                    .iter()
                    .find(|i| matches!(i, ImportItem::Folder { name, .. } if name == "Admins"))
                    .unwrap();
                if let ImportItem::Folder { items, .. } = nested {
                    assert_eq!(items.len(), 1);
                }
            }
            _ => panic!("expected folder"),
        }
    }

    #[test]
    fn meta_sort_key_orders_children() {
        let s = export(&json!([
            { "_id": "wrk_1", "_type": "workspace", "name": "Demo" },
            { "_id": "g", "_type": "request_group", "parentId": "wrk_1", "name": "G" },
            { "_id": "r2", "_type": "request", "parentId": "g",
              "name": "Second", "method": "GET", "url": "https://x", "metaSortKey": -50 },
            { "_id": "r1", "_type": "request", "parentId": "g",
              "name": "First", "method": "GET", "url": "https://x", "metaSortKey": -100 }
        ]));
        let c = from_json(&s).unwrap();
        let ImportItem::Folder { items, .. } = &c.items[0] else {
            panic!()
        };
        match (&items[0], &items[1]) {
            (ImportItem::Request { draft: a }, ImportItem::Request { draft: b }) => {
                assert_eq!(a.name, "First");
                assert_eq!(b.name, "Second");
            }
            _ => panic!("expected two requests"),
        }
    }

    #[test]
    fn templates_strip_underscore_prefix() {
        let s = export(&json!([
            { "_id": "wrk_1", "_type": "workspace", "name": "Demo" },
            { "_id": "r1", "_type": "request", "parentId": "wrk_1",
              "name": "Auth me", "method": "GET",
              "url": "{{ _.baseUrl }}/me",
              "headers": [
                  { "name": "Authorization", "value": "Bearer {{ _.token }}" }
              ] }
        ]));
        let c = from_json(&s).unwrap();
        let ImportItem::Request { draft } = &c.items[0] else {
            panic!()
        };
        let RequestVariant::Rest(rest) = &draft.variant;
        assert_eq!(rest.url, "{{baseUrl}}/me");
        assert_eq!(rest.headers[0].value, "Bearer {{token}}");
    }

    #[test]
    fn imports_form_urlencoded_body() {
        let s = export(&json!([
            { "_id": "wrk_1", "_type": "workspace", "name": "Demo" },
            { "_id": "r", "_type": "request", "parentId": "wrk_1",
              "name": "Login", "method": "POST", "url": "https://x/login",
              "body": {
                  "mimeType": "application/x-www-form-urlencoded",
                  "params": [
                      { "name": "u", "value": "alice" },
                      { "name": "p", "value": "secret", "disabled": true }
                  ]
              } }
        ]));
        let c = from_json(&s).unwrap();
        let ImportItem::Request { draft } = &c.items[0] else {
            panic!()
        };
        let RequestVariant::Rest(rest) = &draft.variant;
        match &rest.body {
            Some(BodyDraft::FormUrlEncoded { fields }) => {
                assert_eq!(fields.len(), 2);
                assert!(fields.iter().any(|f| f.name == "u" && f.enabled));
                assert!(fields.iter().any(|f| f.name == "p" && !f.enabled));
            }
            other => panic!("expected form, got {other:?}"),
        }
    }

    #[test]
    fn imports_json_body_when_mime_is_json() {
        let s = export(&json!([
            { "_id": "wrk_1", "_type": "workspace", "name": "Demo" },
            { "_id": "r", "_type": "request", "parentId": "wrk_1",
              "name": "Create", "method": "POST", "url": "https://x/x",
              "body": {
                  "mimeType": "application/json",
                  "text": "{\"name\":\"Alice\",\"n\":3}"
              } }
        ]));
        let c = from_json(&s).unwrap();
        let ImportItem::Request { draft } = &c.items[0] else {
            panic!()
        };
        let RequestVariant::Rest(rest) = &draft.variant;
        match &rest.body {
            Some(BodyDraft::Json { value }) => assert_eq!(value, &json!({"name":"Alice","n":3})),
            other => panic!("expected json body, got {other:?}"),
        }
    }

    #[test]
    fn imports_bearer_basic_apikey_auth() {
        let s = export(&json!([
            { "_id": "wrk_1", "_type": "workspace", "name": "Demo" },
            { "_id": "a", "_type": "request", "parentId": "wrk_1",
              "name": "Bearer", "method": "GET", "url": "https://x",
              "authentication": { "type": "bearer", "token": "{{ _.tok }}" } },
            { "_id": "b", "_type": "request", "parentId": "wrk_1",
              "name": "Basic", "method": "GET", "url": "https://x",
              "authentication": { "type": "basic", "username": "u", "password": "p" } },
            { "_id": "c", "_type": "request", "parentId": "wrk_1",
              "name": "ApiKey", "method": "GET", "url": "https://x",
              "authentication": {
                  "type": "apikey", "key": "X-Key", "value": "v", "addTo": "queryParams"
              } }
        ]));
        let c = from_json(&s).unwrap();
        assert_eq!(c.items.len(), 3);

        let ImportItem::Request { draft: a } = &c.items[0] else {
            panic!()
        };
        let RequestVariant::Rest(ra) = &a.variant;
        assert!(matches!(ra.auth, Some(AuthConfig::Bearer { ref token }) if token == "{{tok}}"));

        let ImportItem::Request { draft: b } = &c.items[1] else {
            panic!()
        };
        let RequestVariant::Rest(rb) = &b.variant;
        assert!(matches!(
            rb.auth,
            Some(AuthConfig::Basic { ref username, ref password }) if username == "u" && password == "p"
        ));

        let ImportItem::Request { draft: c2 } = &c.items[2] else {
            panic!()
        };
        let RequestVariant::Rest(rc) = &c2.variant;
        assert!(matches!(
            rc.auth,
            Some(AuthConfig::ApiKey { location: ApiKeyLocation::Query, ref name, ref value })
                if name == "X-Key" && value == "v"
        ));
    }

    #[test]
    fn auth_disabled_flag_drops_credentials() {
        let s = export(&json!([
            { "_id": "wrk_1", "_type": "workspace", "name": "Demo" },
            { "_id": "r", "_type": "request", "parentId": "wrk_1",
              "name": "x", "method": "GET", "url": "https://x",
              "authentication": { "type": "bearer", "token": "tok", "disabled": true } }
        ]));
        let c = from_json(&s).unwrap();
        let ImportItem::Request { draft } = &c.items[0] else {
            panic!()
        };
        let RequestVariant::Rest(rest) = &draft.variant;
        assert!(rest.auth.is_none());
    }

    #[test]
    fn imports_workspace_base_environment_variables() {
        let s = export(&json!([
            { "_id": "wrk_1", "_type": "workspace", "name": "Demo" },
            { "_id": "env_base", "_type": "environment", "parentId": "wrk_1",
              "name": "Base", "data": { "baseUrl": "https://api.example.com", "v": 2 } }
        ]));
        let c = from_json(&s).unwrap();
        let map: HashMap<_, _> = c.variables.iter().cloned().collect();
        assert_eq!(
            map.get("baseUrl").map(String::as_str),
            Some("https://api.example.com")
        );
        assert_eq!(map.get("v").map(String::as_str), Some("2"));
    }

    #[test]
    fn skips_non_request_resource_types() {
        // GraphQL / WebSocket subtypes use _type = "request" too but
        // they're still REST-shaped enough to import as REST. Types we
        // truly don't know — e.g. "proto_file" — silently drop.
        let s = export(&json!([
            { "_id": "wrk_1", "_type": "workspace", "name": "Demo" },
            { "_id": "p", "_type": "proto_file", "parentId": "wrk_1", "name": "x.proto" },
            { "_id": "r", "_type": "request", "parentId": "wrk_1",
              "name": "Real", "method": "GET", "url": "https://x" }
        ]));
        let c = from_json(&s).unwrap();
        assert_eq!(c.items.len(), 1);
    }

    #[test]
    fn disabled_query_param_keeps_entry_flagged_off() {
        let s = export(&json!([
            { "_id": "wrk_1", "_type": "workspace", "name": "Demo" },
            { "_id": "r", "_type": "request", "parentId": "wrk_1",
              "name": "Search", "method": "GET", "url": "https://x/search",
              "parameters": [
                  { "name": "q", "value": "widgets" },
                  { "name": "expired", "value": "1", "disabled": true }
              ] }
        ]));
        let c = from_json(&s).unwrap();
        let ImportItem::Request { draft } = &c.items[0] else {
            panic!()
        };
        let RequestVariant::Rest(rest) = &draft.variant;
        assert_eq!(rest.query.len(), 2);
        assert!(rest.query.iter().any(|q| q.name == "q" && q.enabled));
        assert!(rest.query.iter().any(|q| q.name == "expired" && !q.enabled));
    }
}
