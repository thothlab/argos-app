//! Import a Bruno collection (`.bru` text DSL).
//!
//! Bruno collections are filesystem directories: one `.bru` per
//! request, optional `folder.bru` per nested folder, and a top-level
//! `bruno.json` describing the collection. The custom DSL is
//! line-oriented blocks:
//!
//! ```text
//! meta { name: List users }
//! get { url: {{baseUrl}}/users  body: none  auth: none }
//! headers { Accept: application/json }
//! body:json { {"name":"Alice"} }
//! auth:bearer { token: {{token}} }
//! script:pre-request { bru.env.set('x', 1); }
//! tests { /* free JS */ }
//! ```
//!
//! Lines prefixed with `~` are disabled entries (matches Postman /
//! Insomnia's `disabled` flag). Block contents that are user-typed
//! code (`body:*`, `script:*`, `tests`) are captured verbatim so the
//! original formatting survives the round-trip.

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

use crate::format::request::{
    ApiKeyLocation, AuthConfig, BodyDraft, FormField, KeyValue, RequestDraft, RequestVariant,
    RestRequest, ScriptHooks,
};
use crate::http::HttpMethod;

use super::{ImportItem, ImportedCollection};

/// Errors produced by [`from_str`] / [`from_dir`].
#[derive(Debug, thiserror::Error)]
pub enum BrunoImportError {
    /// A `.bru` file lacks a recognisable HTTP verb block (`get {…}`,
    /// `post {…}`, …) — without it we can't infer a method.
    #[error("`.bru` file is missing an HTTP verb block ({0}.bru)")]
    NoMethodBlock(String),
    /// A block opened with `{` was never closed.
    #[error("unclosed block `{0}` (missing `}}`)")]
    UnclosedBlock(String),
    /// I/O failure while walking a Bruno collection directory.
    #[error("filesystem error: {0}")]
    Io(String),
}

/// Parse a single `.bru` file into a [`RequestDraft`].
///
/// `name_hint` is the file's stem used as the request name when the
/// `meta { name: ... }` block doesn't supply one.
///
/// # Errors
///
/// See [`BrunoImportError`].
pub fn from_str(content: &str, name_hint: &str) -> Result<RequestDraft, BrunoImportError> {
    let blocks = parse_blocks(content, name_hint)?;
    let (method, base_block) = pick_method_block(&blocks, name_hint)?;

    let url = take_kv(base_block, "url").unwrap_or_default();
    let _body_marker = take_kv(base_block, "body"); // informational
    let _auth_marker = take_kv(base_block, "auth"); // informational

    let name = blocks
        .iter()
        .find(|b| b.kind == "meta" && b.sub.is_none())
        .and_then(|b| take_kv(b, "name"))
        .unwrap_or_else(|| name_hint.to_string());

    let description = blocks
        .iter()
        .find(|b| b.kind == "docs" && b.sub.is_none())
        .map(|b| b.body.trim().to_string())
        .filter(|s| !s.is_empty());

    let query = collect_kv_block(&blocks, "query");
    let headers = collect_kv_block(&blocks, "headers");

    let body = pick_body(&blocks);
    let auth = pick_auth(&blocks);

    let scripts = ScriptHooks {
        pre_request: blocks
            .iter()
            .find(|b| b.kind == "script" && b.sub.as_deref() == Some("pre-request"))
            .map(|b| b.body.trim().to_string())
            .filter(|s| !s.is_empty()),
        tests: blocks
            .iter()
            .find(|b| b.kind == "tests" && b.sub.is_none())
            .or_else(|| {
                blocks
                    .iter()
                    .find(|b| b.kind == "script" && b.sub.as_deref() == Some("post-response"))
            })
            .map(|b| b.body.trim().to_string())
            .filter(|s| !s.is_empty()),
    };

    Ok(RequestDraft {
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
        scripts,
        schema_ref: None,
    })
}

/// Walk a Bruno collection directory and build an
/// [`ImportedCollection`]. Folders are mirrored 1:1; `.bru` files at
/// each level become requests. `bruno.json` and `environments/` at the
/// root contribute the collection name and variables.
///
/// # Errors
///
/// I/O failures bubble up as [`BrunoImportError::Io`]; parse errors
/// from individual `.bru` files surface verbatim — one bad file
/// aborts the whole import (we'd rather the user see it than have
/// requests silently dropped).
pub fn from_dir(root: &Path) -> Result<ImportedCollection, BrunoImportError> {
    if !root.is_dir() {
        return Err(BrunoImportError::Io(format!(
            "{} is not a directory",
            root.display()
        )));
    }

    // Collection meta: prefer bruno.json's `name` if present; fall
    // back to the directory name.
    let (name, description) = collection_meta(root);
    let variables = collect_environment_vars(root);
    let items = walk_dir(root, true)?;

    Ok(ImportedCollection {
        name,
        description,
        items,
        variables,
    })
}

fn collection_meta(root: &Path) -> (String, Option<String>) {
    let manifest_path = root.join("bruno.json");
    if let Ok(text) = std::fs::read_to_string(&manifest_path) {
        if let Ok(v) = serde_json::from_str::<Value>(&text) {
            let name = v
                .get("name")
                .and_then(Value::as_str)
                .map_or_else(|| dir_name(root), str::to_string);
            let desc = v
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string)
                .filter(|s| !s.is_empty());
            return (name, desc);
        }
    }
    (dir_name(root), None)
}

fn dir_name(p: &Path) -> String {
    p.file_name().map_or_else(
        || "Imported collection".to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

fn collect_environment_vars(root: &Path) -> Vec<(String, String)> {
    let env_dir = root.join("environments");
    if !env_dir.is_dir() {
        return Vec::new();
    }
    let mut out: Vec<(String, String)> = Vec::new();
    let Ok(entries) = std::fs::read_dir(&env_dir) else {
        return out;
    };
    // Pick the first env file — collections often have one default.
    // Users get richer env handling once we add env-file import.
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("bru") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Ok(blocks) = parse_blocks(&content, "env") {
            for b in &blocks {
                if b.kind == "vars" && b.sub.is_none() {
                    for line in kv_lines(&b.body) {
                        if line.enabled {
                            out.push((line.name, line.value));
                        }
                    }
                }
            }
        }
        if !out.is_empty() {
            break;
        }
    }
    out
}

fn walk_dir(dir: &Path, is_root: bool) -> Result<Vec<ImportItem>, BrunoImportError> {
    let mut items: Vec<(SortKey, ImportItem)> = Vec::new();
    let entries = std::fs::read_dir(dir)
        .map_err(|e| BrunoImportError::Io(format!("{}: {e}", dir.display())))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name == "bruno.json" || name == "folder.bru" {
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("bru") {
                continue;
            }
            let content = std::fs::read_to_string(&path)
                .map_err(|e| BrunoImportError::Io(format!("{}: {e}", path.display())))?;
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("request");
            let draft = from_str(&content, stem)?;
            let seq = blocks_seq(&content);
            items.push((
                SortKey(seq, draft.name.clone()),
                ImportItem::Request { draft },
            ));
        } else if path.is_dir() {
            let folder_name_str = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            // `environments/` at the root is handled separately by
            // `collect_environment_vars`. Hidden + Bruno metadata
            // directories never become folders in the tree.
            if is_root && folder_name_str == "environments" {
                continue;
            }
            if folder_name_str.starts_with('.') {
                continue;
            }
            let folder_name = folder_display_name(&path);
            let folder_seq = folder_meta_seq(&path);
            let children = walk_dir(&path, false)?;
            items.push((
                SortKey(folder_seq, folder_name.clone()),
                ImportItem::Folder {
                    name: folder_name,
                    description: None,
                    items: children,
                },
            ));
        }
    }

    items.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(items.into_iter().map(|(_, item)| item).collect())
}

fn folder_display_name(dir: &Path) -> String {
    let meta_path = dir.join("folder.bru");
    if let Ok(text) = std::fs::read_to_string(&meta_path) {
        if let Ok(blocks) = parse_blocks(&text, "folder") {
            if let Some(name) = blocks
                .iter()
                .find(|b| b.kind == "meta")
                .and_then(|b| take_kv(b, "name"))
            {
                return name;
            }
        }
    }
    dir_name(dir)
}

fn folder_meta_seq(dir: &Path) -> i64 {
    let meta_path = dir.join("folder.bru");
    let Ok(text) = std::fs::read_to_string(&meta_path) else {
        return i64::MAX;
    };
    let Ok(blocks) = parse_blocks(&text, "folder") else {
        return i64::MAX;
    };
    blocks
        .iter()
        .find(|b| b.kind == "meta")
        .and_then(|b| take_kv(b, "seq"))
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(i64::MAX)
}

fn blocks_seq(content: &str) -> i64 {
    let Ok(blocks) = parse_blocks(content, "x") else {
        return i64::MAX;
    };
    blocks
        .iter()
        .find(|b| b.kind == "meta")
        .and_then(|b| take_kv(b, "seq"))
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(i64::MAX)
}

/// (seq, name) sort key — Bruno orders siblings by `meta.seq`, with
/// alphabetical name as the tiebreaker.
#[derive(PartialEq, Eq)]
struct SortKey(i64, String);

impl Ord for SortKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0).then_with(|| self.1.cmp(&other.1))
    }
}
impl PartialOrd for SortKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

// ---- low-level block parser ---------------------------------------------

#[derive(Debug, Clone)]
struct Block {
    kind: String,
    sub: Option<String>,
    body: String,
}

fn parse_blocks(input: &str, where_label: &str) -> Result<Vec<Block>, BrunoImportError> {
    let bytes = input.as_bytes();
    let mut i = 0usize;
    let mut out = Vec::new();

    while i < bytes.len() {
        // Skip leading whitespace + line comments (`//` to EOL).
        let (next, _) = skip_ws_and_comments(bytes, i);
        i = next;
        if i >= bytes.len() {
            break;
        }

        // Block header: identifier (with optional `:sub`) then `{`.
        let header_start = i;
        while i < bytes.len()
            && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'_' | b'-' | b':'))
        {
            i += 1;
        }
        let header = std::str::from_utf8(&bytes[header_start..i])
            .map(str::to_string)
            .unwrap_or_default();
        if header.is_empty() {
            // Skip unknown chars rather than fail — Bruno tolerates
            // arbitrary text between blocks (rare but possible).
            i += 1;
            continue;
        }

        let (kind, sub) = match header.split_once(':') {
            Some((k, s)) => (k.to_string(), Some(s.to_string())),
            None => (header.clone(), None),
        };

        // Whitespace then `{`.
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'{' {
            // Not a block — skip token.
            continue;
        }
        i += 1; // consume '{'

        // Capture the body, counting nested braces so embedded JSON /
        // JS doesn't trip us up.
        let body_start = i;
        let mut depth = 1;
        while i < bytes.len() && depth > 0 {
            match bytes[i] {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                b'"' => {
                    i = skip_string(bytes, i + 1, b'"');
                    continue;
                }
                b'\'' => {
                    i = skip_string(bytes, i + 1, b'\'');
                    continue;
                }
                // Skip line comments only when they sit at the start
                // of a line — bare `//` inside the body (e.g.
                // `https://...`) isn't a comment.
                b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' && is_line_start(bytes, i) => {
                    while i < bytes.len() && bytes[i] != b'\n' {
                        i += 1;
                    }
                    continue;
                }
                _ => {}
            }
            i += 1;
        }
        if depth != 0 {
            return Err(BrunoImportError::UnclosedBlock(format!(
                "{where_label}: {header}"
            )));
        }
        let body_end = i - 1; // exclude the closing '}'
        let body = std::str::from_utf8(&bytes[body_start..body_end])
            .unwrap_or("")
            .to_string();
        out.push(Block { kind, sub, body });
    }

    Ok(out)
}

fn skip_ws_and_comments(bytes: &[u8], mut i: usize) -> (usize, bool) {
    loop {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        return (i, true);
    }
}

/// `true` if the position is at the start of a logical line (only
/// whitespace between `i` and the previous `\n` or buffer start).
fn is_line_start(bytes: &[u8], at: usize) -> bool {
    let mut j = at;
    while j > 0 {
        j -= 1;
        match bytes[j] {
            b'\n' => return true,
            b' ' | b'\t' => {}
            _ => return false,
        }
    }
    true
}

fn skip_string(bytes: &[u8], mut i: usize, quote: u8) -> usize {
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            i += 2;
            continue;
        }
        if bytes[i] == quote {
            return i + 1;
        }
        i += 1;
    }
    i
}

// ---- block content helpers ----------------------------------------------

fn pick_method_block<'a>(
    blocks: &'a [Block],
    name_hint: &str,
) -> Result<(HttpMethod, &'a Block), BrunoImportError> {
    for b in blocks {
        if let Some(m) = verb_to_method(&b.kind) {
            if b.sub.is_none() {
                return Ok((m, b));
            }
        }
    }
    Err(BrunoImportError::NoMethodBlock(name_hint.to_string()))
}

fn verb_to_method(s: &str) -> Option<HttpMethod> {
    Some(match s {
        "get" => HttpMethod::Get,
        "post" => HttpMethod::Post,
        "put" => HttpMethod::Put,
        "patch" => HttpMethod::Patch,
        "delete" => HttpMethod::Delete,
        "head" => HttpMethod::Head,
        "options" => HttpMethod::Options,
        _ => return None,
    })
}

fn take_kv(block: &Block, key: &str) -> Option<String> {
    for line in kv_lines(&block.body) {
        if line.enabled && line.name == key {
            return Some(line.value);
        }
    }
    None
}

fn collect_kv_block(blocks: &[Block], kind: &str) -> Vec<KeyValue> {
    blocks
        .iter()
        .filter(|b| b.kind == kind && b.sub.is_none())
        .flat_map(|b| {
            kv_lines(&b.body)
                .into_iter()
                .map(|l| KeyValue {
                    name: l.name,
                    value: l.value,
                    enabled: l.enabled,
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

struct KvLine {
    name: String,
    value: String,
    enabled: bool,
}

fn kv_lines(body: &str) -> Vec<KvLine> {
    let mut out = Vec::new();
    for raw in body.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        let (enabled, work) = if let Some(rest) = trimmed.strip_prefix('~') {
            (false, rest.trim())
        } else {
            (true, trimmed)
        };
        let Some((k, v)) = work.split_once(':') else {
            continue;
        };
        out.push(KvLine {
            name: k.trim().to_string(),
            value: v.trim().to_string(),
            enabled,
        });
    }
    out
}

fn pick_body(blocks: &[Block]) -> Option<BodyDraft> {
    let body_block = blocks
        .iter()
        .find(|b| b.kind == "body" && b.sub.is_some())?;
    let sub = body_block.sub.as_deref().unwrap_or("");
    let raw = body_block.body.trim();
    match sub {
        "json" => {
            // Bruno wraps JSON bodies in an extra `{ ... }` per the
            // DSL; our brace counter strips that. The remaining text
            // is the JSON document itself.
            if let Ok(v) = serde_json::from_str::<Value>(raw) {
                return Some(BodyDraft::Json { value: v });
            }
            Some(BodyDraft::Text {
                content: raw.to_string(),
                content_type: "application/json".into(),
            })
        }
        "xml" => Some(BodyDraft::Text {
            content: raw.to_string(),
            content_type: "application/xml".into(),
        }),
        "form-urlencoded" => {
            let fields = kv_lines(raw)
                .into_iter()
                .map(|l| FormField {
                    name: l.name,
                    value: l.value,
                    enabled: l.enabled,
                })
                .collect();
            Some(BodyDraft::FormUrlEncoded { fields })
        }
        "multipart-form" => {
            // Same downgrade strategy as the Insomnia importer:
            // multipart isn't first-class yet, so represent it as
            // form-urlencoded with file values flagged inline.
            let fields = kv_lines(raw)
                .into_iter()
                .map(|l| FormField {
                    name: l.name,
                    value: if l.value.starts_with('@') {
                        format!("<file upload: {}>", &l.value[1..])
                    } else {
                        l.value
                    },
                    enabled: l.enabled,
                })
                .collect();
            Some(BodyDraft::FormUrlEncoded { fields })
        }
        "graphql" => Some(BodyDraft::Text {
            content: raw.to_string(),
            content_type: "application/graphql".into(),
        }),
        _ => Some(BodyDraft::Text {
            content: raw.to_string(),
            content_type: "text/plain".into(),
        }),
    }
}

fn pick_auth(blocks: &[Block]) -> Option<AuthConfig> {
    let auth_block = blocks
        .iter()
        .find(|b| b.kind == "auth" && b.sub.is_some())?;
    let sub = auth_block.sub.as_deref().unwrap_or("");
    let kvs: HashMap<String, String> = kv_lines(&auth_block.body)
        .into_iter()
        .filter(|l| l.enabled)
        .map(|l| (l.name, l.value))
        .collect();
    match sub {
        "bearer" => Some(AuthConfig::Bearer {
            token: kvs.get("token").cloned().unwrap_or_default(),
        }),
        "basic" => Some(AuthConfig::Basic {
            username: kvs.get("username").cloned().unwrap_or_default(),
            password: kvs.get("password").cloned().unwrap_or_default(),
        }),
        "apikey" => {
            let location = match kvs.get("placement").map(String::as_str) {
                Some("query") => ApiKeyLocation::Query,
                Some("cookie") => ApiKeyLocation::Cookie,
                _ => ApiKeyLocation::Header,
            };
            Some(AuthConfig::ApiKey {
                name: kvs.get("key").cloned().unwrap_or_default(),
                value: kvs.get("value").cloned().unwrap_or_default(),
                location,
            })
        }
        "inherit" => Some(AuthConfig::Inherit),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write(dir: &Path, rel: &str, content: &str) {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&p, content).unwrap();
    }

    #[test]
    fn parse_minimal_get_request() {
        let src = "
meta {
  name: List users
  seq: 1
}

get {
  url: https://api.example.com/users
}
";
        let draft = from_str(src, "fallback").unwrap();
        assert_eq!(draft.name, "List users");
        let RequestVariant::Rest(rest) = &draft.variant else { panic!("expected REST variant"); };
        assert_eq!(rest.method, HttpMethod::Get);
        assert_eq!(rest.url, "https://api.example.com/users");
    }

    #[test]
    fn falls_back_to_name_hint_when_meta_missing_name() {
        let src = "get { url: https://x }";
        let draft = from_str(src, "fallback").unwrap();
        assert_eq!(draft.name, "fallback");
    }

    #[test]
    fn errors_when_no_verb_block_present() {
        let err = from_str("meta { name: bad }", "fallback").unwrap_err();
        assert!(matches!(err, BrunoImportError::NoMethodBlock(_)));
    }

    #[test]
    fn parses_query_and_headers_blocks_with_disabled_entries() {
        let src = "
post { url: https://x/x }
query {
  q: widgets
  ~stale: 1
}
headers {
  Accept: application/json
  ~X-Trace: off
}
";
        let draft = from_str(src, "x").unwrap();
        let RequestVariant::Rest(rest) = &draft.variant else { panic!("expected REST variant"); };
        assert_eq!(rest.method, HttpMethod::Post);
        assert_eq!(rest.query.len(), 2);
        assert!(rest.query.iter().any(|q| q.name == "q" && q.enabled));
        assert!(rest.query.iter().any(|q| q.name == "stale" && !q.enabled));
        assert!(rest.headers.iter().any(|h| h.name == "Accept" && h.enabled));
        assert!(rest
            .headers
            .iter()
            .any(|h| h.name == "X-Trace" && !h.enabled));
    }

    #[test]
    fn parses_json_body() {
        let src = r#"
post { url: https://x/x }
body:json {
  {
    "name": "Alice",
    "n": 3
  }
}
"#;
        let draft = from_str(src, "x").unwrap();
        let RequestVariant::Rest(rest) = &draft.variant else { panic!("expected REST variant"); };
        match &rest.body {
            Some(BodyDraft::Json { value }) => {
                assert_eq!(value, &serde_json::json!({"name":"Alice","n":3}));
            }
            other => panic!("expected json body, got {other:?}"),
        }
    }

    #[test]
    fn parses_form_urlencoded_body() {
        let src = "
post { url: https://x/x }
body:form-urlencoded {
  user: alice
  ~remember: 1
}
";
        let draft = from_str(src, "x").unwrap();
        let RequestVariant::Rest(rest) = &draft.variant else { panic!("expected REST variant"); };
        match &rest.body {
            Some(BodyDraft::FormUrlEncoded { fields }) => {
                assert_eq!(fields.len(), 2);
                assert!(fields.iter().any(|f| f.name == "user" && f.enabled));
                assert!(fields.iter().any(|f| f.name == "remember" && !f.enabled));
            }
            other => panic!("expected form, got {other:?}"),
        }
    }

    #[test]
    fn parses_bearer_basic_apikey_auth() {
        let cases = [
            (
                r"
get { url: https://x }
auth:bearer { token: {{token}} }
",
                "bearer",
            ),
            (
                r"
get { url: https://x }
auth:basic {
  username: u
  password: p
}
",
                "basic",
            ),
            (
                r"
get { url: https://x }
auth:apikey {
  key: X-Key
  value: abc
  placement: query
}
",
                "apikey",
            ),
        ];
        for (src, label) in cases {
            let draft = from_str(src, "x").unwrap();
            let RequestVariant::Rest(rest) = &draft.variant else { panic!("expected REST variant"); };
            match (label, &rest.auth) {
                ("bearer", Some(AuthConfig::Bearer { token })) => {
                    assert_eq!(token, "{{token}}");
                }
                ("basic", Some(AuthConfig::Basic { username, password })) => {
                    assert_eq!(username, "u");
                    assert_eq!(password, "p");
                }
                (
                    "apikey",
                    Some(AuthConfig::ApiKey {
                        name,
                        value,
                        location: ApiKeyLocation::Query,
                    }),
                ) => {
                    assert_eq!(name, "X-Key");
                    assert_eq!(value, "abc");
                }
                _ => panic!("unexpected auth for {label}: {:?}", rest.auth),
            }
        }
    }

    #[test]
    fn parses_pre_request_and_tests_scripts() {
        let src = r"
get { url: https://x }
script:pre-request {
  bru.env.set('ts', Date.now());
}
tests {
  pm.test('ok', function () { pm.expect(pm.response.code).to.equal(200); });
}
";
        let draft = from_str(src, "x").unwrap();
        assert!(draft
            .scripts
            .pre_request
            .as_deref()
            .unwrap()
            .contains("bru.env.set"));
        assert!(draft.scripts.tests.as_deref().unwrap().contains("pm.test"));
    }

    #[test]
    fn nested_braces_inside_script_body_do_not_break_parsing() {
        let src = "
get { url: https://x }
tests {
  if (true) { console.log('ok'); }
}
headers { Accept: application/json }
";
        let draft = from_str(src, "x").unwrap();
        let RequestVariant::Rest(rest) = &draft.variant else { panic!("expected REST variant"); };
        // The headers block must still be picked up after the
        // script block with nested braces.
        assert!(rest.headers.iter().any(|h| h.name == "Accept"));
        assert!(draft
            .scripts
            .tests
            .as_deref()
            .unwrap()
            .contains("if (true)"));
    }

    #[test]
    fn line_comments_are_ignored() {
        let src = "
// outer comment
meta {
  // inner comment
  name: Foo
}
get {
  url: https://x
  // method-block comment
}
";
        let draft = from_str(src, "x").unwrap();
        assert_eq!(draft.name, "Foo");
    }

    #[test]
    fn unclosed_block_returns_error() {
        let err = from_str("get { url: https://x", "x").unwrap_err();
        assert!(matches!(err, BrunoImportError::UnclosedBlock(_)));
    }

    #[test]
    fn from_dir_walks_a_bruno_collection() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        write(
            root,
            "bruno.json",
            r#"{ "version": "1", "name": "Demo", "type": "collection" }"#,
        );
        write(
            root,
            "list-users.bru",
            "meta {\n  name: List users\n  seq: 1\n}\nget { url: https://x/users }\n",
        );
        write(
            root,
            "Users/folder.bru",
            "meta {\n  name: Users\n  seq: 2\n}\n",
        );
        write(
            root,
            "Users/get-one.bru",
            "meta {\n  name: Get one\n  seq: 1\n}\nget { url: https://x/users/1 }\n",
        );
        write(
            root,
            "environments/local.bru",
            "vars {\n  baseUrl: https://api.example.com\n  ~stale: 1\n}\n",
        );

        let c = from_dir(root).unwrap();
        assert_eq!(c.name, "Demo");
        // Two top-level items: the file `list-users.bru` and the
        // `Users/` folder.
        assert_eq!(c.items.len(), 2);
        assert!(c
            .items
            .iter()
            .any(|i| matches!(i, ImportItem::Folder { name, .. } if name == "Users")));
        assert!(c
            .items
            .iter()
            .any(|i| matches!(i, ImportItem::Request { draft } if draft.name == "List users")));

        // Env: only enabled var flows in.
        assert_eq!(c.variables.len(), 1);
        assert_eq!(c.variables[0].0, "baseUrl");
    }

    #[test]
    fn from_dir_orders_siblings_by_meta_seq() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        write(
            root,
            "bruno.json",
            r#"{ "version": "1", "name": "Ordered" }"#,
        );
        write(
            root,
            "a.bru",
            "meta {\n  name: A\n  seq: 3\n}\nget { url: https://x }\n",
        );
        write(
            root,
            "b.bru",
            "meta {\n  name: B\n  seq: 1\n}\nget { url: https://x }\n",
        );
        write(
            root,
            "c.bru",
            "meta {\n  name: C\n  seq: 2\n}\nget { url: https://x }\n",
        );
        let c = from_dir(root).unwrap();
        let names: Vec<_> = c
            .items
            .iter()
            .filter_map(|i| match i {
                ImportItem::Request { draft } => Some(draft.name.clone()),
                ImportItem::Folder { .. } => None,
            })
            .collect();
        assert_eq!(names, vec!["B", "C", "A"]);
    }
}
