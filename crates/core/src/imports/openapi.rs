//! OpenAPI / Swagger importer (Swagger 2.0, OpenAPI 3.0, OpenAPI 3.1).
//!
//! Matches the strategy of `postman.rs` — we walk the spec via
//! [`serde_json::Value`] instead of pulling a typed crate. The OpenAPI
//! schema is huge and full of optional fields; a hand-rolled walker
//! handles 3.0 and 3.1 with the same code and skips bits we don't
//! understand. Input may be JSON *or* YAML — we try JSON first and
//! fall back to YAML.
//!
//! **Swagger 2.0** (`swagger: "2.0"`) is accepted via an in-memory
//! rewrite into 3.0 shape — see [`swagger_2_to_oas3`]. The conversion
//! covers what matters for request reproduction: host / basePath /
//! schemes → servers, body / formData parameters → `requestBody`,
//! definitions → `components.schemas`, securityDefinitions →
//! `components.securitySchemes`, and `$ref` paths. Anything we don't
//! understand is left alone for the 3.x walker to ignore.
//!
//! Mapping:
//!   - `servers[0].url` → URL prefix (paths join onto it).
//!   - `paths.{path}.{method}` → one `RequestDraft` per operation.
//!   - `tags[0]` on the operation → folder name; no tag → root.
//!   - Path params `{id}` → `{{id}}` so Argos's templating resolves them.
//!   - `parameters` (path / query / header) → KV entries seeded with
//!     `example` → schema `example` → schema `default` → "".
//!   - `requestBody.content["application/json"]` → `BodyDraft::Json`
//!     (example, or a one-level stub from schema). Other media types
//!     map to `Text` with the raw content type.
//!   - `application/x-www-form-urlencoded` → `BodyDraft::FormUrlEncoded`
//!     from schema properties.
//!   - `security` + `components.securitySchemes`: HTTP bearer / Basic
//!     and apiKey (header/query/cookie) → `AuthConfig`. OAuth2 / OIDC
//!     are noted in the description and otherwise skipped.
//!   - `$ref` is resolved one hop against `components.*` (parameters,
//!     requestBodies, schemas, securitySchemes). Unresolved refs are
//!     left as empty values rather than failing the whole import.

#![allow(clippy::match_wildcard_for_single_variants)]

use serde_json::Value;

use crate::format::request::{
    ApiKeyLocation, AuthConfig, BodyDraft, FormField, KeyValue, RequestDraft, RequestVariant,
    RestRequest, ScriptHooks,
};
use crate::http::HttpMethod;

use super::{ImportItem, ImportedCollection};

/// Errors produced by [`from_str`].
#[derive(Debug, thiserror::Error)]
pub enum OpenApiImportError {
    /// Input is neither valid JSON nor valid YAML.
    #[error("invalid OpenAPI document: {0}")]
    InvalidDocument(String),
    /// Top-level shape doesn't look like an OpenAPI / Swagger spec we
    /// support. Carries whatever we saw in `openapi` / `swagger` for
    /// diagnostic display.
    #[error("not an OpenAPI 3.x or Swagger 2.0 document (got {0:?})")]
    NotOpenApi3(String),
}

/// Parse an OpenAPI 3.x or Swagger 2.0 document (JSON or YAML) into an
/// [`ImportedCollection`]. JSON is tried first; on parse failure we
/// fall back to YAML. Swagger 2.0 documents are converted to a 3.0
/// shape in memory before the rest of the walker runs — see
/// [`swagger_2_to_oas3`] for the rewrite rules.
///
/// # Errors
///
/// [`OpenApiImportError::InvalidDocument`] if neither parser accepts
/// the input; [`OpenApiImportError::NotOpenApi3`] if the version
/// discriminator doesn't match a supported spec.
pub fn from_str(input: &str) -> Result<ImportedCollection, OpenApiImportError> {
    let mut spec: Value = parse_any(input)?;

    // Swagger 2.0 path: rewrite the document shape before the rest of
    // the walker sees it. After this call the document carries
    // `openapi: "3.0.3"` and the `paths` / `components` structure the
    // 3.x branch already understands.
    if spec.get("swagger").and_then(Value::as_str) == Some("2.0") {
        spec = swagger_2_to_oas3(spec);
    }

    let version = spec
        .get("openapi")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !version.starts_with("3.") {
        // Surface whatever discriminator the document carried — helps
        // users figure out why their file was rejected (e.g. swagger
        // 1.2, OpenAPI 4.x, or a totally unrelated JSON).
        let seen = if version.is_empty() {
            spec.get("swagger")
                .and_then(Value::as_str)
                .unwrap_or("<no version field>")
                .to_string()
        } else {
            version.to_string()
        };
        return Err(OpenApiImportError::NotOpenApi3(seen));
    }

    let info = spec.get("info").and_then(Value::as_object);
    let name = info
        .and_then(|m| m.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("Imported OpenAPI")
        .to_string();
    let description = info
        .and_then(|m| m.get("description"))
        .and_then(Value::as_str)
        .map(str::to_string);

    let base_url = spec
        .get("servers")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(|s| s.get("url"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim_end_matches('/')
        .to_string();

    let global_security = spec.get("security").cloned();

    // Bucket operations by tag → preserved insertion order.
    let mut buckets: Vec<(String, Vec<ImportItem>)> = Vec::new();
    let mut root_items: Vec<ImportItem> = Vec::new();

    let paths = spec.get("paths").and_then(Value::as_object);
    if let Some(paths) = paths {
        for (path, path_item) in paths {
            if path.starts_with("x-") {
                continue;
            }
            for (method_key, op) in path_item.as_object().into_iter().flatten() {
                let Some(method) = parse_method(method_key) else {
                    continue;
                };
                let draft = build_operation(
                    &spec,
                    &base_url,
                    path,
                    method,
                    op,
                    path_item,
                    global_security.as_ref(),
                );
                let tag = op
                    .get("tags")
                    .and_then(Value::as_array)
                    .and_then(|t| t.first())
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let item = ImportItem::Request { draft };
                match tag {
                    Some(t) => {
                        if let Some((_, bucket)) = buckets.iter_mut().find(|(n, _)| n == &t) {
                            bucket.push(item);
                        } else {
                            buckets.push((t, vec![item]));
                        }
                    }
                    None => root_items.push(item),
                }
            }
        }
    }

    let mut items = Vec::with_capacity(buckets.len() + root_items.len());
    for (name, children) in buckets {
        items.push(ImportItem::Folder {
            name,
            description: None,
            items: children,
        });
    }
    items.extend(root_items);

    // `servers[0].url` exposed as a `baseUrl` variable so users can
    // switch envs without touching every request.
    let variables = if base_url.is_empty() {
        Vec::new()
    } else {
        vec![("baseUrl".to_string(), base_url)]
    };

    Ok(ImportedCollection {
        name,
        description,
        items,
        variables,
    })
}

fn parse_any(input: &str) -> Result<Value, OpenApiImportError> {
    match serde_json::from_str::<Value>(input) {
        Ok(v) => Ok(v),
        Err(json_err) => match serde_yaml::from_str::<Value>(input) {
            Ok(v) => Ok(v),
            Err(yaml_err) => Err(OpenApiImportError::InvalidDocument(format!(
                "JSON: {json_err}; YAML: {yaml_err}"
            ))),
        },
    }
}

fn parse_method(s: &str) -> Option<HttpMethod> {
    match s.to_ascii_lowercase().as_str() {
        "get" => Some(HttpMethod::Get),
        "post" => Some(HttpMethod::Post),
        "put" => Some(HttpMethod::Put),
        "patch" => Some(HttpMethod::Patch),
        "delete" => Some(HttpMethod::Delete),
        "head" => Some(HttpMethod::Head),
        "options" => Some(HttpMethod::Options),
        _ => None,
    }
}

fn build_operation(
    spec: &Value,
    base_url: &str,
    path: &str,
    method: HttpMethod,
    op: &Value,
    path_item: &Value,
    global_security: Option<&Value>,
) -> RequestDraft {
    let name = op
        .get("operationId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            op.get("summary")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| format!("{} {path}", method.as_str()));

    // Path-level parameters apply to every method on that path; the
    // operation can override by (name, in).
    let mut params: Vec<Value> = Vec::new();
    for src in [path_item.get("parameters"), op.get("parameters")] {
        if let Some(arr) = src.and_then(Value::as_array) {
            for p in arr {
                let resolved = resolve_ref(spec, p);
                let name = resolved.get("name").and_then(Value::as_str).unwrap_or("");
                let loc = resolved.get("in").and_then(Value::as_str).unwrap_or("");
                params.retain(|existing| {
                    let en = existing.get("name").and_then(Value::as_str).unwrap_or("");
                    let el = existing.get("in").and_then(Value::as_str).unwrap_or("");
                    !(en == name && el == loc)
                });
                params.push(resolved);
            }
        }
    }

    // Rewrite OpenAPI path templates `{name}` → Argos `{{name}}` *before*
    // joining with the base URL, so we don't accidentally rewrite the
    // `{{baseUrl}}` prefix into `{{{baseUrl}}}`.
    let templated_path = rewrite_path_params(path);
    let is_absolute = path.starts_with("http://") || path.starts_with("https://");
    let url = if is_absolute || base_url.is_empty() {
        templated_path
    } else {
        format!("{{{{baseUrl}}}}{templated_path}")
    };

    let mut headers: Vec<KeyValue> = Vec::new();
    let mut query: Vec<KeyValue> = Vec::new();
    for p in &params {
        let pname = p.get("name").and_then(Value::as_str).unwrap_or("");
        if pname.is_empty() {
            continue;
        }
        let loc = p.get("in").and_then(Value::as_str).unwrap_or("");
        let value = parameter_example(p);
        let kv = KeyValue {
            name: pname.to_string(),
            value,
            enabled: true,
        };
        match loc {
            "query" => query.push(kv),
            "header" => headers.push(kv),
            // "path" is already rewritten into the URL; we don't surface
            // it as a separate field but we leave it for the user to
            // override via env vars.
            _ => {}
        }
    }

    let body = op
        .get("requestBody")
        .map(|rb| resolve_ref(spec, rb))
        .and_then(|rb| build_body(spec, &rb));

    let auth = resolve_security(spec, op.get("security").or(global_security));

    let description = op
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            op.get("summary")
                .and_then(Value::as_str)
                .map(str::to_string)
        });

    RequestDraft {
        kind: crate::format::Kind::Request,
        name,
        description,
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

fn rewrite_path_params(url: &str) -> String {
    // Replace each `{name}` with `{{name}}`. Single-pass scan; balanced
    // braces only — anything malformed is left alone.
    let mut out = String::with_capacity(url.len() + 8);
    let bytes = url.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' && i + 1 < bytes.len() && bytes[i + 1] != b'{' {
            if let Some(close) = url[i..].find('}') {
                let inner = &url[i + 1..i + close];
                if !inner.contains('{') && !inner.is_empty() {
                    out.push_str("{{");
                    out.push_str(inner);
                    out.push_str("}}");
                    i += close + 1;
                    continue;
                }
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn parameter_example(p: &Value) -> String {
    if let Some(s) = scalar_to_string(p.get("example")) {
        return s;
    }
    if let Some(examples) = p.get("examples").and_then(Value::as_object) {
        if let Some(first) = examples.values().next() {
            if let Some(s) = scalar_to_string(first.get("value")) {
                return s;
            }
        }
    }
    if let Some(schema) = p.get("schema") {
        if let Some(s) = scalar_to_string(schema.get("example")) {
            return s;
        }
        if let Some(s) = scalar_to_string(schema.get("default")) {
            return s;
        }
        // Enum first entry is a useful default for path/query.
        if let Some(arr) = schema.get("enum").and_then(Value::as_array) {
            if let Some(first) = arr.first() {
                if let Some(s) = scalar_to_string(Some(first)) {
                    return s;
                }
            }
        }
    }
    String::new()
}

fn scalar_to_string(v: Option<&Value>) -> Option<String> {
    let v = v?;
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Null => None,
        // Non-scalar (object/array) → JSON-encode for query/header use.
        other => Some(other.to_string()),
    }
}

fn build_body(spec: &Value, request_body: &Value) -> Option<BodyDraft> {
    let content = request_body.get("content").and_then(Value::as_object)?;

    // Prefer application/json; otherwise first text-ish entry; otherwise
    // first available.
    let prefer = [
        "application/json",
        "application/x-www-form-urlencoded",
        "text/plain",
        "application/xml",
        "text/xml",
    ];
    let mut chosen: Option<(&String, &Value)> = None;
    for mt in prefer {
        if let Some(v) = content.get(mt) {
            chosen = Some((content.keys().find(|k| k.as_str() == mt).unwrap(), v));
            break;
        }
    }
    if chosen.is_none() {
        chosen = content.iter().next();
    }
    let (media_type, media_obj) = chosen?;
    let media_obj = resolve_ref(spec, media_obj);

    if media_type == "application/x-www-form-urlencoded" {
        let schema = media_obj.get("schema").map(|s| resolve_ref(spec, s));
        let fields = if let Some(schema) = schema {
            schema
                .get("properties")
                .and_then(Value::as_object)
                .map(|props| {
                    props
                        .iter()
                        .map(|(k, v)| FormField {
                            name: k.clone(),
                            value: scalar_to_string(v.get("example"))
                                .or_else(|| scalar_to_string(v.get("default")))
                                .unwrap_or_default(),
                            enabled: true,
                        })
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        return Some(BodyDraft::FormUrlEncoded { fields });
    }

    // application/json (or any other media type that wants JSON shape).
    if media_type.contains("json") {
        let example = extract_example(spec, &media_obj);
        let value = example.unwrap_or(Value::Object(serde_json::Map::default()));
        return Some(BodyDraft::Json { value });
    }

    // Generic text body — try example first, otherwise empty.
    let content_str = extract_example(spec, &media_obj)
        .map(|v| {
            if let Value::String(s) = &v {
                s.clone()
            } else {
                v.to_string()
            }
        })
        .unwrap_or_default();
    Some(BodyDraft::Text {
        content: content_str,
        content_type: media_type.clone(),
    })
}

fn extract_example(spec: &Value, media_obj: &Value) -> Option<Value> {
    if let Some(e) = media_obj.get("example") {
        if !e.is_null() {
            return Some(e.clone());
        }
    }
    if let Some(examples) = media_obj.get("examples").and_then(Value::as_object) {
        if let Some(first) = examples.values().next() {
            let first = resolve_ref(spec, first);
            if let Some(v) = first.get("value") {
                return Some(v.clone());
            }
        }
    }
    if let Some(schema) = media_obj.get("schema") {
        let schema = resolve_ref(spec, schema);
        if let Some(e) = schema.get("example") {
            if !e.is_null() {
                return Some(e.clone());
            }
        }
        // Last-resort stub generated from the schema's `properties`.
        return Some(stub_from_schema(spec, &schema, 0));
    }
    None
}

/// Build a one-or-two-level stub value from a schema. We don't try to
/// model the full JSON Schema dialect — just enough to get a useful
/// request body the user can edit. Recursion is bounded so cyclic
/// `$ref` graphs (a common OpenAPI footgun) don't blow the stack.
fn stub_from_schema(spec: &Value, schema: &Value, depth: usize) -> Value {
    if depth > 4 {
        return Value::Null;
    }
    let schema = resolve_ref(spec, schema);

    if let Some(e) = schema.get("example") {
        if !e.is_null() {
            return e.clone();
        }
    }
    if let Some(d) = schema.get("default") {
        if !d.is_null() {
            return d.clone();
        }
    }
    if let Some(arr) = schema.get("enum").and_then(Value::as_array) {
        if let Some(first) = arr.first() {
            return first.clone();
        }
    }

    // Compose-of-keywords: prefer first variant.
    for key in ["allOf", "oneOf", "anyOf"] {
        if let Some(arr) = schema.get(key).and_then(Value::as_array) {
            if let Some(first) = arr.first() {
                return stub_from_schema(spec, first, depth + 1);
            }
        }
    }

    let ty = schema.get("type").and_then(Value::as_str);
    match ty {
        Some("object") => {
            let mut map = serde_json::Map::new();
            if let Some(props) = schema.get("properties").and_then(Value::as_object) {
                for (k, v) in props {
                    map.insert(k.clone(), stub_from_schema(spec, v, depth + 1));
                }
            }
            Value::Object(map)
        }
        Some("array") => {
            let item = schema
                .get("items")
                .map_or(Value::Null, |i| stub_from_schema(spec, i, depth + 1));
            Value::Array(vec![item])
        }
        Some("integer" | "number") => Value::Number(0.into()),
        Some("boolean") => Value::Bool(false),
        Some("string") => Value::String(String::new()),
        _ => {
            // No type at all: if there are properties, treat as object.
            if schema.get("properties").is_some() {
                let mut map = serde_json::Map::new();
                if let Some(props) = schema.get("properties").and_then(Value::as_object) {
                    for (k, v) in props {
                        map.insert(k.clone(), stub_from_schema(spec, v, depth + 1));
                    }
                }
                Value::Object(map)
            } else {
                Value::Null
            }
        }
    }
}

fn resolve_security(spec: &Value, security: Option<&Value>) -> Option<AuthConfig> {
    let arr = security?.as_array()?;
    let first = arr.first()?;
    let obj = first.as_object()?;
    // Empty object means "no security" — OpenAPI uses `[{}]` to opt out.
    let (scheme_name, _scopes) = obj.iter().next()?;
    let scheme = spec
        .get("components")
        .and_then(|c| c.get("securitySchemes"))
        .and_then(|s| s.get(scheme_name))
        .map(|s| resolve_ref(spec, s))?;
    let kind = scheme.get("type").and_then(Value::as_str)?;
    match kind {
        "http" => {
            let raw_scheme = scheme
                .get("scheme")
                .and_then(Value::as_str)
                .unwrap_or_default();
            // OpenAPI requires `scheme` to be a case-insensitive HTTP
            // auth scheme name (RFC 7235); the registry doesn't
            // promise lowercase.
            match raw_scheme.to_ascii_lowercase().as_str() {
                "bearer" => Some(AuthConfig::Bearer {
                    token: format!("{{{{{scheme_name}}}}}"),
                }),
                "basic" => Some(AuthConfig::Basic {
                    username: String::new(),
                    password: String::new(),
                }),
                _ => None,
            }
        }
        "apiKey" => {
            let name = scheme
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(scheme_name)
                .to_string();
            let location = match scheme.get("in").and_then(Value::as_str).unwrap_or("header") {
                "query" => ApiKeyLocation::Query,
                "cookie" => ApiKeyLocation::Cookie,
                _ => ApiKeyLocation::Header,
            };
            Some(AuthConfig::ApiKey {
                name,
                value: format!("{{{{{scheme_name}}}}}"),
                location,
            })
        }
        // oauth2 / openIdConnect: deferred — return None, user can wire
        // a Bearer token manually using the env var of their choice.
        _ => None,
    }
}

/// Resolve a `{ "$ref": "#/components/.../X" }` one hop. Returns the
/// referenced node (cloned) or the input unchanged if it isn't a ref.
/// External / non-local refs are not supported.
fn resolve_ref(spec: &Value, node: &Value) -> Value {
    let Some(reference) = node.get("$ref").and_then(Value::as_str) else {
        return node.clone();
    };
    let Some(rest) = reference.strip_prefix("#/") else {
        return node.clone();
    };
    let mut cur = spec;
    for seg in rest.split('/') {
        // JSON Pointer escaping: ~1 → /, ~0 → ~
        let seg = seg.replace("~1", "/").replace("~0", "~");
        cur = match cur.get(&seg) {
            Some(v) => v,
            None => return node.clone(),
        };
    }
    cur.clone()
}

// ---- Swagger 2.0 → OpenAPI 3.0 in-memory rewrite ------------------------

/// Rewrite a Swagger 2.0 document into an OpenAPI 3.0 shape. The result
/// is what the rest of this module (built for 3.x) can consume without
/// further changes.
///
/// What gets rewritten:
///   - `swagger: "2.0"` → `openapi: "3.0.3"`.
///   - `host` + `basePath` + `schemes[0]` → `servers[0].url`.
///   - `definitions` → `components.schemas`.
///   - top-level `parameters` → `components.parameters`.
///   - top-level `responses` → `components.responses`.
///   - `securityDefinitions` → `components.securitySchemes`
///     (with the 2.0 `type: basic` → `type: http, scheme: basic` patch).
///   - Per-operation parameters with `in: body` → `requestBody`,
///     with `in: formData` → `requestBody` using
///     `application/x-www-form-urlencoded`.
///   - All `$ref` strings re-prefixed from `#/definitions/...` etc.
///     to `#/components/...`.
///
/// What stays unchanged:
///   - Operation `parameters` with `in: path / query / header` —
///     identical shape between 2.0 and 3.0.
///   - `tags`, `paths` keys, response status codes.
///   - `security` lists — name lookup against the now-migrated
///     `components.securitySchemes` works transparently.
fn swagger_2_to_oas3(mut doc: Value) -> Value {
    let consumes_root: Vec<String> = doc
        .get("consumes")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    // ---- 1. discriminator + servers ----
    if let Some(obj) = doc.as_object_mut() {
        obj.remove("swagger");
        obj.insert("openapi".into(), Value::String("3.0.3".into()));

        let host = obj
            .get("host")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let base_path = obj
            .get("basePath")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let scheme = obj
            .get("schemes")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
            .and_then(Value::as_str)
            .unwrap_or("https")
            .to_string();
        obj.remove("host");
        obj.remove("basePath");
        obj.remove("schemes");
        if !host.is_empty() {
            let url = format!("{scheme}://{host}{base_path}");
            obj.insert(
                "servers".into(),
                Value::Array(vec![serde_json::json!({ "url": url })]),
            );
        }

        // ---- 2. definitions / parameters / responses → components.* ----
        let mut components = obj
            .remove("components")
            .and_then(|c| {
                if c.is_object() {
                    Some(c)
                } else {
                    None
                }
            })
            .unwrap_or_else(|| Value::Object(Default::default()));
        let comp_obj = components.as_object_mut().expect("components is object");

        if let Some(defs) = obj.remove("definitions") {
            comp_obj.insert("schemas".into(), defs);
        }
        if let Some(params) = obj.remove("parameters") {
            comp_obj.insert("parameters".into(), params);
        }
        if let Some(responses) = obj.remove("responses") {
            comp_obj.insert("responses".into(), responses);
        }
        if let Some(sd) = obj.remove("securityDefinitions") {
            comp_obj.insert("securitySchemes".into(), convert_security_definitions(sd));
        }
        if !comp_obj.is_empty() {
            obj.insert("components".into(), components);
        }
    }

    // ---- 3. rewrite $ref strings everywhere ----
    rewrite_refs(&mut doc);

    // ---- 4. per-operation body / formData → requestBody ----
    convert_operations(&mut doc, &consumes_root);

    doc
}

fn convert_security_definitions(sd: Value) -> Value {
    let Some(obj) = sd.as_object() else {
        return sd;
    };
    let mut out = serde_json::Map::new();
    for (name, scheme) in obj {
        let Some(s) = scheme.as_object() else {
            continue;
        };
        let typ = s.get("type").and_then(Value::as_str).unwrap_or("");
        let converted = match typ {
            // `basic` in 2.0 → `http` with `scheme: basic` in 3.0.
            "basic" => serde_json::json!({
                "type": "http",
                "scheme": "basic",
                "description": s.get("description").cloned().unwrap_or(Value::Null),
            }),
            // apiKey shape is identical (type, name, in).
            "apiKey" => Value::Object(s.clone()),
            // oauth2: the flow shape changed substantially; emit a
            // 3.0-shaped stub so the field at least exists. The
            // OpenAPI walker treats unknown oauth2 details as a
            // best-effort note anyway.
            "oauth2" => {
                let flow = s.get("flow").and_then(Value::as_str).unwrap_or("implicit");
                let flow_key = match flow {
                    "implicit" => "implicit",
                    "password" => "password",
                    "application" => "clientCredentials",
                    "accessCode" => "authorizationCode",
                    _ => "implicit",
                };
                let mut flow_obj = serde_json::Map::new();
                if let Some(url) = s.get("authorizationUrl") {
                    flow_obj.insert("authorizationUrl".into(), url.clone());
                }
                if let Some(url) = s.get("tokenUrl") {
                    flow_obj.insert("tokenUrl".into(), url.clone());
                }
                flow_obj.insert(
                    "scopes".into(),
                    s.get("scopes").cloned().unwrap_or_else(|| serde_json::json!({})),
                );
                serde_json::json!({
                    "type": "oauth2",
                    "flows": { flow_key: Value::Object(flow_obj) },
                })
            }
            _ => Value::Object(s.clone()),
        };
        out.insert(name.clone(), converted);
    }
    Value::Object(out)
}

/// Walk the JSON tree mutating every `$ref` string from a 2.0 path to
/// the matching 3.0 component path.
fn rewrite_refs(node: &mut Value) {
    match node {
        Value::Object(obj) => {
            for (k, v) in obj.iter_mut() {
                if k == "$ref" {
                    if let Value::String(s) = v {
                        if let Some(rest) = s.strip_prefix("#/definitions/") {
                            *s = format!("#/components/schemas/{rest}");
                        } else if let Some(rest) = s.strip_prefix("#/parameters/") {
                            *s = format!("#/components/parameters/{rest}");
                        } else if let Some(rest) = s.strip_prefix("#/responses/") {
                            *s = format!("#/components/responses/{rest}");
                        } else if let Some(rest) = s.strip_prefix("#/securityDefinitions/") {
                            *s = format!("#/components/securitySchemes/{rest}");
                        }
                    }
                } else {
                    rewrite_refs(v);
                }
            }
        }
        Value::Array(arr) => arr.iter_mut().for_each(rewrite_refs),
        _ => {}
    }
}

/// Walk every operation in `paths.*` and rewrite body / formData
/// parameters into a 3.0-shaped `requestBody`.
fn convert_operations(doc: &mut Value, consumes_root: &[String]) {
    let Some(paths) = doc.get_mut("paths").and_then(Value::as_object_mut) else {
        return;
    };
    for (_path, path_item) in paths.iter_mut() {
        let Some(path_obj) = path_item.as_object_mut() else {
            continue;
        };
        for (key, op) in path_obj.iter_mut() {
            // Skip non-operation keys (parameters, x-extensions, etc).
            if !matches!(
                key.as_str(),
                "get" | "post" | "put" | "patch" | "delete" | "head" | "options" | "trace"
            ) {
                continue;
            }
            convert_one_operation(op, consumes_root);
        }
    }
}

fn convert_one_operation(op: &mut Value, consumes_root: &[String]) {
    let Some(op_obj) = op.as_object_mut() else {
        return;
    };
    let Some(params_val) = op_obj.remove("parameters") else {
        return;
    };
    let Some(params) = params_val.as_array() else {
        op_obj.insert("parameters".into(), params_val);
        return;
    };

    let mut kept: Vec<Value> = Vec::new();
    let mut body_param: Option<Value> = None;
    let mut form_params: Vec<Value> = Vec::new();
    for p in params {
        match p.get("in").and_then(Value::as_str) {
            Some("body") => body_param = Some(p.clone()),
            Some("formData") => form_params.push(p.clone()),
            _ => kept.push(p.clone()),
        }
    }
    if !kept.is_empty() {
        op_obj.insert("parameters".into(), Value::Array(kept));
    }

    // The operation's effective consumes list: per-op `consumes`
    // overrides the document-wide one (this is the 2.0 inheritance
    // rule); fall back to a sensible default per body kind.
    let consumes_op: Vec<String> = op_obj
        .get("consumes")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    op_obj.remove("consumes");

    if let Some(body) = body_param {
        let media_type = first_or(&consumes_op, consumes_root, "application/json");
        let schema = body
            .get("schema")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let required = body.get("required").cloned().unwrap_or(Value::Bool(true));
        let description = body.get("description").cloned();
        let mut rb = serde_json::json!({
            "required": required,
            "content": { media_type: { "schema": schema } },
        });
        if let Some(d) = description {
            rb["description"] = d;
        }
        op_obj.insert("requestBody".into(), rb);
    } else if !form_params.is_empty() {
        let media_type = first_or(
            &consumes_op,
            consumes_root,
            "application/x-www-form-urlencoded",
        );
        let mut props = serde_json::Map::new();
        let mut required: Vec<Value> = Vec::new();
        for p in &form_params {
            let Some(name) = p.get("name").and_then(Value::as_str) else {
                continue;
            };
            // Translate the inline 2.0 parameter type / format /
            // enum / items into a small property schema. We don't
            // try to fully model files (`type: file`) — they become
            // `type: string, format: binary` so generators at least
            // produce something usable.
            let mut prop = serde_json::Map::new();
            for k in ["type", "format", "enum", "items", "description", "default"] {
                if let Some(v) = p.get(k) {
                    prop.insert(k.into(), v.clone());
                }
            }
            if prop.get("type").and_then(Value::as_str) == Some("file") {
                prop.insert("type".into(), Value::String("string".into()));
                prop.insert("format".into(), Value::String("binary".into()));
            }
            props.insert(name.into(), Value::Object(prop));
            if p.get("required").and_then(Value::as_bool) == Some(true) {
                required.push(Value::String(name.into()));
            }
        }
        let mut schema = serde_json::json!({
            "type": "object",
            "properties": Value::Object(props),
        });
        if !required.is_empty() {
            schema["required"] = Value::Array(required);
        }
        op_obj.insert(
            "requestBody".into(),
            serde_json::json!({
                "content": { media_type: { "schema": schema } },
            }),
        );
    }
}

fn first_or(primary: &[String], fallback: &[String], default_value: &str) -> String {
    primary
        .first()
        .or_else(|| fallback.first())
        .cloned()
        .unwrap_or_else(|| default_value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn doc(extra: Value) -> String {
        let mut base = json!({
            "openapi": "3.0.3",
            "info": { "title": "Demo", "version": "1.0" },
            "servers": [{ "url": "https://api.example.com" }],
            "paths": {}
        });
        // Merge extra into base at the top level.
        if let Value::Object(extra_map) = extra {
            let base_obj = base.as_object_mut().unwrap();
            for (k, v) in extra_map {
                base_obj.insert(k, v);
            }
        }
        base.to_string()
    }

    #[test]
    fn rejects_non_openapi() {
        // 1.x is too old (we don't try to rewrite it); 4.x is too new
        // (no such spec exists yet, but if it ever does we'd want to
        // bail rather than silently mis-parse).
        let err = from_str(&json!({ "swagger": "1.2", "info": {"title":"x"} }).to_string())
            .unwrap_err();
        assert!(matches!(err, OpenApiImportError::NotOpenApi3(_)));
        let err = from_str(&json!({ "openapi": "4.0.0", "info": {"title":"x"} }).to_string())
            .unwrap_err();
        assert!(matches!(err, OpenApiImportError::NotOpenApi3(_)));
    }

    #[test]
    fn rejects_garbage() {
        let err = from_str(": : not json or yaml :::").unwrap_err();
        assert!(matches!(err, OpenApiImportError::InvalidDocument(_)));
    }

    #[test]
    fn accepts_yaml_input() {
        let yaml = "openapi: 3.0.3\ninfo:\n  title: Y\n  version: '1'\nservers:\n  - url: https://y.test\npaths:\n  /ping:\n    get:\n      operationId: ping\n";
        let c = from_str(yaml).unwrap();
        assert_eq!(c.name, "Y");
        assert_eq!(c.variables[0].0, "baseUrl");
        assert_eq!(c.variables[0].1, "https://y.test");
        let req = first_request(&c);
        assert_eq!(req.url, "{{baseUrl}}/ping");
        assert_eq!(req.method, HttpMethod::Get);
    }

    fn first_request(c: &ImportedCollection) -> RestRequest {
        for it in &c.items {
            match it {
                ImportItem::Request { draft } => {
                    let RequestVariant::Rest(rest) = &draft.variant else { panic!("expected REST variant"); };
                    return rest.clone();
                }
                ImportItem::Folder { items, .. } => {
                    for it in items {
                        if let ImportItem::Request { draft } = it {
                            let RequestVariant::Rest(rest) = &draft.variant else { panic!("expected REST variant"); };
                            return rest.clone();
                        }
                    }
                }
            }
        }
        panic!("no request found");
    }

    #[test]
    fn maps_paths_methods_and_operation_id() {
        let s = doc(json!({
            "paths": {
                "/users": {
                    "get":  { "operationId": "list_users", "tags": ["Users"] },
                    "post": { "operationId": "create_user", "tags": ["Users"] }
                }
            }
        }));
        let c = from_str(&s).unwrap();
        // Both operations grouped under "Users" folder.
        let ImportItem::Folder { name, items, .. } = &c.items[0] else {
            panic!("expected folder");
        };
        assert_eq!(name, "Users");
        assert_eq!(items.len(), 2);
        let names: Vec<_> = items
            .iter()
            .map(|i| match i {
                ImportItem::Request { draft } => draft.name.clone(),
                _ => String::new(),
            })
            .collect();
        assert!(names.contains(&"list_users".to_string()));
        assert!(names.contains(&"create_user".to_string()));
    }

    #[test]
    fn rewrites_path_params_to_argos_template() {
        let s = doc(json!({
            "paths": {
                "/users/{id}/posts/{postId}": {
                    "get": { "operationId": "get_post" }
                }
            }
        }));
        let c = from_str(&s).unwrap();
        let rest = first_request(&c);
        assert_eq!(rest.url, "{{baseUrl}}/users/{{id}}/posts/{{postId}}");
    }

    #[test]
    fn seeds_query_and_header_params_from_examples() {
        let s = doc(json!({
            "paths": {
                "/search": {
                    "get": {
                        "operationId": "search",
                        "parameters": [
                            { "name": "q", "in": "query", "schema": { "type": "string", "example": "widgets" } },
                            { "name": "page", "in": "query", "schema": { "type": "integer", "default": 1 } },
                            { "name": "X-Trace", "in": "header", "example": "abc123" }
                        ]
                    }
                }
            }
        }));
        let c = from_str(&s).unwrap();
        let rest = first_request(&c);
        let q = rest.query.iter().find(|q| q.name == "q").unwrap();
        assert_eq!(q.value, "widgets");
        let page = rest.query.iter().find(|q| q.name == "page").unwrap();
        assert_eq!(page.value, "1");
        let trace = rest
            .headers
            .iter()
            .find(|h| h.name == "X-Trace")
            .unwrap();
        assert_eq!(trace.value, "abc123");
    }

    #[test]
    fn path_level_parameters_are_inherited() {
        let s = doc(json!({
            "paths": {
                "/items/{id}": {
                    "parameters": [
                        { "name": "id", "in": "path", "required": true, "schema": { "type": "integer", "example": 7 } }
                    ],
                    "get": { "operationId": "get_item" }
                }
            }
        }));
        let c = from_str(&s).unwrap();
        let rest = first_request(&c);
        assert_eq!(rest.url, "{{baseUrl}}/items/{{id}}");
    }

    #[test]
    fn body_uses_request_body_example() {
        let s = doc(json!({
            "paths": {
                "/users": {
                    "post": {
                        "operationId": "create",
                        "requestBody": {
                            "content": {
                                "application/json": {
                                    "schema": { "type": "object" },
                                    "example": { "name": "Alice", "n": 3 }
                                }
                            }
                        }
                    }
                }
            }
        }));
        let c = from_str(&s).unwrap();
        let rest = first_request(&c);
        match &rest.body {
            Some(BodyDraft::Json { value }) => {
                assert_eq!(value, &json!({ "name": "Alice", "n": 3 }));
            }
            other => panic!("expected json body, got {other:?}"),
        }
    }

    #[test]
    fn body_stub_from_schema_when_no_example() {
        let s = doc(json!({
            "paths": {
                "/users": {
                    "post": {
                        "requestBody": {
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "name": { "type": "string" },
                                            "age":  { "type": "integer" },
                                            "tags": { "type": "array", "items": { "type": "string" } }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }));
        let c = from_str(&s).unwrap();
        let rest = first_request(&c);
        let Some(BodyDraft::Json { value }) = &rest.body else {
            panic!("expected json body");
        };
        let obj = value.as_object().unwrap();
        assert_eq!(obj.get("name").unwrap(), &Value::String(String::new()));
        assert!(obj.get("age").unwrap().is_number());
        let tags = obj.get("tags").unwrap().as_array().unwrap();
        assert_eq!(tags.len(), 1);
    }

    #[test]
    fn body_from_urlencoded_schema_properties() {
        let s = doc(json!({
            "paths": {
                "/login": {
                    "post": {
                        "requestBody": {
                            "content": {
                                "application/x-www-form-urlencoded": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "user": { "type": "string", "example": "alice" },
                                            "pass": { "type": "string" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }));
        let c = from_str(&s).unwrap();
        let rest = first_request(&c);
        let Some(BodyDraft::FormUrlEncoded { fields }) = &rest.body else {
            panic!("expected form body");
        };
        assert_eq!(fields.len(), 2);
        let user = fields.iter().find(|f| f.name == "user").unwrap();
        assert_eq!(user.value, "alice");
    }

    #[test]
    fn bearer_security_becomes_bearer_auth() {
        let s = doc(json!({
            "components": {
                "securitySchemes": {
                    "bearerAuth": { "type": "http", "scheme": "bearer", "bearerFormat": "JWT" }
                }
            },
            "security": [{ "bearerAuth": [] }],
            "paths": {
                "/me": { "get": { "operationId": "me" } }
            }
        }));
        let c = from_str(&s).unwrap();
        let rest = first_request(&c);
        match &rest.auth {
            Some(AuthConfig::Bearer { token }) => assert_eq!(token, "{{bearerAuth}}"),
            other => panic!("expected bearer, got {other:?}"),
        }
    }

    #[test]
    fn apikey_security_with_header_location() {
        let s = doc(json!({
            "components": {
                "securitySchemes": {
                    "ApiKey": { "type": "apiKey", "in": "header", "name": "X-Api-Key" }
                }
            },
            "security": [{ "ApiKey": [] }],
            "paths": {
                "/protected": { "get": { "operationId": "p" } }
            }
        }));
        let c = from_str(&s).unwrap();
        let rest = first_request(&c);
        match &rest.auth {
            Some(AuthConfig::ApiKey { name, value, location }) => {
                assert_eq!(name, "X-Api-Key");
                assert_eq!(value, "{{ApiKey}}");
                assert!(matches!(location, ApiKeyLocation::Header));
            }
            other => panic!("expected apikey, got {other:?}"),
        }
    }

    #[test]
    fn operation_security_overrides_global() {
        let s = doc(json!({
            "components": {
                "securitySchemes": {
                    "globalBearer": { "type": "http", "scheme": "bearer" },
                    "opKey": { "type": "apiKey", "in": "query", "name": "key" }
                }
            },
            "security": [{ "globalBearer": [] }],
            "paths": {
                "/x": { "get": { "operationId": "x", "security": [{ "opKey": [] }] } }
            }
        }));
        let c = from_str(&s).unwrap();
        let rest = first_request(&c);
        assert!(matches!(rest.auth, Some(AuthConfig::ApiKey { .. })));
    }

    #[test]
    fn empty_security_array_disables_auth() {
        let s = doc(json!({
            "components": {
                "securitySchemes": {
                    "b": { "type": "http", "scheme": "bearer" }
                }
            },
            "security": [{ "b": [] }],
            "paths": {
                "/public": { "get": { "operationId": "pub", "security": [] } }
            }
        }));
        let c = from_str(&s).unwrap();
        let rest = first_request(&c);
        assert!(rest.auth.is_none());
    }

    #[test]
    fn ref_in_parameters_is_resolved() {
        let s = doc(json!({
            "components": {
                "parameters": {
                    "QParam": {
                        "name": "q", "in": "query",
                        "schema": { "type": "string", "example": "ref!" }
                    }
                }
            },
            "paths": {
                "/r": {
                    "get": {
                        "operationId": "r",
                        "parameters": [{ "$ref": "#/components/parameters/QParam" }]
                    }
                }
            }
        }));
        let c = from_str(&s).unwrap();
        let rest = first_request(&c);
        let q = rest.query.iter().find(|q| q.name == "q").unwrap();
        assert_eq!(q.value, "ref!");
    }

    #[test]
    fn ref_in_request_body_schema_is_resolved() {
        let s = doc(json!({
            "components": {
                "schemas": {
                    "User": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string", "example": "Alice" }
                        }
                    }
                }
            },
            "paths": {
                "/u": {
                    "post": {
                        "operationId": "u",
                        "requestBody": {
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/User" }
                                }
                            }
                        }
                    }
                }
            }
        }));
        let c = from_str(&s).unwrap();
        let rest = first_request(&c);
        let Some(BodyDraft::Json { value }) = &rest.body else {
            panic!("expected json");
        };
        assert_eq!(value.get("name").unwrap(), &json!("Alice"));
    }

    #[test]
    fn absolute_path_url_skips_base() {
        let s = doc(json!({
            "paths": {
                "https://other.test/abs": { "get": { "operationId": "abs" } }
            }
        }));
        let c = from_str(&s).unwrap();
        let rest = first_request(&c);
        assert_eq!(rest.url, "https://other.test/abs");
    }

    #[test]
    fn untagged_operations_go_to_root() {
        let s = doc(json!({
            "paths": {
                "/a": { "get": { "operationId": "a" } },
                "/b": { "get": { "operationId": "b", "tags": ["G"] } }
            }
        }));
        let c = from_str(&s).unwrap();
        // First item is the tag bucket; the untagged request follows.
        let kinds: Vec<&'static str> = c
            .items
            .iter()
            .map(|i| match i {
                ImportItem::Folder { .. } => "folder",
                ImportItem::Request { .. } => "request",
            })
            .collect();
        assert!(kinds.contains(&"folder"));
        assert!(kinds.contains(&"request"));
    }

    #[test]
    fn falls_back_to_method_path_for_name_when_no_id() {
        let s = doc(json!({
            "paths": {
                "/heartbeat": { "get": { } }
            }
        }));
        let c = from_str(&s).unwrap();
        let req = match &c.items[0] {
            ImportItem::Request { draft } => draft.clone(),
            _ => panic!(),
        };
        assert_eq!(req.name, "GET /heartbeat");
    }

    #[test]
    fn handles_recursive_schemas_without_stack_overflow() {
        let s = doc(json!({
            "components": {
                "schemas": {
                    "Node": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" },
                            "child": { "$ref": "#/components/schemas/Node" }
                        }
                    }
                }
            },
            "paths": {
                "/n": {
                    "post": {
                        "requestBody": {
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/Node" }
                                }
                            }
                        }
                    }
                }
            }
        }));
        let c = from_str(&s).unwrap();
        let rest = first_request(&c);
        // We mostly care that the stub generated *without crashing*.
        assert!(matches!(rest.body, Some(BodyDraft::Json { .. })));
    }

    // ---- Swagger 2.0 tests --------------------------------------------------

    #[test]
    fn swagger_2_minimal_round_trips_to_oas3() {
        // host + basePath + schemes → servers[0].url; a single GET
        // with one query parameter; tag → folder; baseUrl variable.
        let s = json!({
            "swagger": "2.0",
            "info": { "title": "Petstore", "version": "1.0" },
            "host": "petstore.swagger.io",
            "basePath": "/v2",
            "schemes": ["https"],
            "paths": {
                "/pet/findByStatus": {
                    "get": {
                        "tags": ["pet"],
                        "operationId": "findByStatus",
                        "parameters": [
                            { "name": "status", "in": "query", "type": "string", "required": true }
                        ]
                    }
                }
            }
        })
        .to_string();
        let c = from_str(&s).unwrap();
        assert_eq!(c.name, "Petstore");
        assert_eq!(c.variables, vec![("baseUrl".into(), "https://petstore.swagger.io/v2".into())]);
        let folder = match &c.items[0] {
            ImportItem::Folder { name, items, .. } => {
                assert_eq!(name, "pet");
                items
            }
            _ => panic!("expected folder bucket"),
        };
        let rest = match &folder[0] {
            ImportItem::Request { draft } => match &draft.variant {
                RequestVariant::Rest(r) => r,
                _ => panic!("expected REST variant"),
            },
            _ => panic!("expected request"),
        };
        assert_eq!(rest.method, HttpMethod::Get);
        assert!(rest.url.ends_with("/pet/findByStatus"));
        assert!(rest.query.iter().any(|q| q.name == "status"));
    }

    #[test]
    fn swagger_2_body_param_becomes_request_body() {
        // POST with `parameters: [{in: body, schema: {$ref: '#/definitions/Pet'}}]`
        // → `requestBody.content["application/json"].schema` with ref
        // rewritten to `#/components/schemas/Pet`.
        let s = json!({
            "swagger": "2.0",
            "info": { "title": "Petstore", "version": "1.0" },
            "host": "petstore.swagger.io",
            "basePath": "/v2",
            "consumes": ["application/json"],
            "paths": {
                "/pet": {
                    "post": {
                        "operationId": "addPet",
                        "parameters": [
                            { "name": "body", "in": "body", "required": true,
                              "schema": { "$ref": "#/definitions/Pet" } }
                        ]
                    }
                }
            },
            "definitions": {
                "Pet": {
                    "type": "object",
                    "required": ["name"],
                    "properties": {
                        "name": { "type": "string", "example": "doggie" },
                        "status": { "type": "string", "example": "available" }
                    }
                }
            }
        })
        .to_string();
        let c = from_str(&s).unwrap();
        let rest = first_request(&c);
        assert_eq!(rest.method, HttpMethod::Post);
        match &rest.body {
            Some(BodyDraft::Json { value }) => {
                // Schema example stub picked up `name` + `status`.
                assert_eq!(value["name"], "doggie");
                assert_eq!(value["status"], "available");
            }
            other => panic!("expected JSON body, got {other:?}"),
        }
    }

    #[test]
    fn swagger_2_form_data_becomes_urlencoded_body() {
        let s = json!({
            "swagger": "2.0",
            "info": { "title": "X", "version": "1.0" },
            "host": "api.example.com",
            "paths": {
                "/login": {
                    "post": {
                        "operationId": "login",
                        "consumes": ["application/x-www-form-urlencoded"],
                        "parameters": [
                            { "name": "username", "in": "formData", "type": "string", "required": true },
                            { "name": "password", "in": "formData", "type": "string", "required": true }
                        ]
                    }
                }
            }
        })
        .to_string();
        let c = from_str(&s).unwrap();
        let rest = first_request(&c);
        match &rest.body {
            Some(BodyDraft::FormUrlEncoded { fields }) => {
                let names: Vec<_> = fields.iter().map(|e| e.name.as_str()).collect();
                assert!(names.contains(&"username"));
                assert!(names.contains(&"password"));
            }
            other => panic!("expected form-urlencoded body, got {other:?}"),
        }
    }

    #[test]
    fn swagger_2_basic_auth_security_is_migrated() {
        // `securityDefinitions.basicAuth: { type: basic }` → 3.0
        // `components.securitySchemes.basicAuth: { type: http, scheme: basic }`,
        // and an operation referencing it should resolve to
        // `AuthConfig::Basic`.
        let s = json!({
            "swagger": "2.0",
            "info": { "title": "X", "version": "1.0" },
            "host": "api.example.com",
            "securityDefinitions": {
                "basicAuth": { "type": "basic" }
            },
            "security": [{ "basicAuth": [] }],
            "paths": {
                "/me": {
                    "get": { "operationId": "me" }
                }
            }
        })
        .to_string();
        let c = from_str(&s).unwrap();
        let rest = first_request(&c);
        assert!(matches!(rest.auth, Some(AuthConfig::Basic { .. })));
    }

    #[test]
    fn swagger_2_api_key_in_header_survives_migration() {
        let s = json!({
            "swagger": "2.0",
            "info": { "title": "X", "version": "1.0" },
            "host": "api.example.com",
            "securityDefinitions": {
                "apiKeyHeader": { "type": "apiKey", "in": "header", "name": "X-API-Key" }
            },
            "security": [{ "apiKeyHeader": [] }],
            "paths": {
                "/me": { "get": { "operationId": "me" } }
            }
        })
        .to_string();
        let c = from_str(&s).unwrap();
        let rest = first_request(&c);
        match &rest.auth {
            Some(AuthConfig::ApiKey { name, location, .. }) => {
                assert_eq!(name, "X-API-Key");
                assert!(matches!(location, ApiKeyLocation::Header));
            }
            other => panic!("expected ApiKey auth, got {other:?}"),
        }
    }
}
