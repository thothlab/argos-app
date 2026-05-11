// Prevents additional console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod watcher;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use std::collections::HashMap;

use argos_core::codegen::curl;
use argos_core::codegen::curl::from_curl;
use argos_core::exports::{har, postman as postman_export};
use argos_core::format::{slugify, EnvVar, Environment, Folder, RequestDraft};
use argos_core::imports::bruno;
use argos_core::imports::insomnia;
use argos_core::imports::openapi;
use argos_core::imports::postman;
use argos_core::imports::ImportItem;
use argos_core::ws::{self as ws_core, WsConnectOptions, WsDirection, WsEvent, WsHandle};
use argos_core::{HttpClient, HttpMethod, HttpRequest, HttpResponse, Resolver, Workspace};
use argos_scripting::{
    run_pre_request, run_tests, ScriptBody, ScriptFormField, ScriptHeader, ScriptRequest,
    ScriptResponse, TestResult,
};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, State};
use tokio::sync::{Mutex, OnceCell};

use watcher::ActiveWatcher;

// ---- shared state --------------------------------------------------------

/// HTTP client built lazily on first use and reused for the lifetime of the app.
type AppState = Arc<OnceCell<HttpClient>>;

async fn http_client(state: &AppState) -> Result<&HttpClient, String> {
    state
        .get_or_try_init(|| async { HttpClient::new().map_err(|e| e.to_string()) })
        .await
}

/// Live WebSocket connections, keyed by the client-supplied connection
/// id. The handle owns the spawned task; dropping it terminates the
/// connection (see `argos_core::ws::WsHandle::Drop`).
type WsRegistry = Arc<Mutex<HashMap<String, WsHandle>>>;

// ---- core / health -------------------------------------------------------

/// Returns the embedded `argos-core` version string.
#[tauri::command]
fn core_version() -> String {
    argos_core::version().to_string()
}

/// Health-check ping. Used by the UI on startup to verify the IPC bridge.
#[tauri::command]
fn ping() -> &'static str {
    "pong"
}

// ---- HTTP ----------------------------------------------------------------

/// Outcome of a `send_request` call. The `pre_request_logs` field is
/// always present (empty array if no script ran), so the UI can render
/// the script console without branching on whether a script was attached.
#[derive(Debug, Serialize, Deserialize)]
pub struct SendOutcome {
    pub response: HttpResponse,
    pub pre_request_logs: Vec<String>,
    pub tests_logs: Vec<String>,
    pub tests: Vec<TestResult>,
    pub env_updates: HashMap<String, String>,
    /// Names the script(s) cleared via `bru.env.unset` /
    /// `pm.environment.unset`. Empty array if none.
    #[serde(default)]
    pub env_unsets: Vec<String>,
}

/// Execute one HTTP request via `argos-core` and return the buffered response.
///
/// `env` carries the active environment's variables (plus secrets) so the
/// backend can resolve `{{name}}` placeholders before sending. Pass an empty
/// map for a "no env active" send — placeholders will then be left verbatim.
///
/// `pre_request_script` is an optional JS snippet evaluated through the
/// `argos-scripting` sandbox before the request leaves Argos. It can mutate
/// `bru.req` and `bru.env`; we apply those mutations here.
#[tauri::command]
async fn send_request(
    state: State<'_, AppState>,
    req: HttpRequest,
    env: Option<HashMap<String, String>>,
    pre_request_script: Option<String>,
    tests_script: Option<String>,
) -> Result<SendOutcome, String> {
    let mut env = env.unwrap_or_default();
    let mut pre_request_logs = Vec::new();
    let mut env_updates = HashMap::new();
    let mut env_unsets: Vec<String> = Vec::new();

    let mut req = req;
    if let Some(script) = pre_request_script.as_ref().filter(|s| !s.trim().is_empty()) {
        let script_req = ScriptRequest {
            method: req.method.as_str().to_string(),
            url: req.url.clone(),
            headers: req
                .headers
                .iter()
                .map(|h| ScriptHeader {
                    name: h.name.clone(),
                    value: h.value.clone(),
                })
                .collect(),
            body: http_body_to_script(req.body.as_ref()),
        };
        let outcome =
            run_pre_request(script, script_req, env.clone()).map_err(|e| e.to_string())?;
        pre_request_logs = outcome.logs;
        env_updates = outcome.env_updates;
        env_unsets = outcome.env_unsets;

        // Apply mutations.
        if let Some(method) = parse_http_method(&outcome.request.method) {
            req.method = method;
        }
        req.url = outcome.request.url;
        req.headers = outcome
            .request
            .headers
            .into_iter()
            .map(|h| argos_core::HttpHeader::new(h.name, h.value))
            .collect();

        // Body: only overwrite if the script actually touched it. Raw
        // (binary) bodies the script can't represent stay intact this way.
        if outcome.body_modified {
            req.body = outcome.request.body.map(script_body_to_http);
        }

        // Env updates / unsets feed into the resolver for *this* send.
        // We don't persist them; downstream sends in the same session
        // start from the disk-backed env again.
        for (k, v) in &env_updates {
            env.insert(k.clone(), v.clone());
        }
        for k in &env_unsets {
            env.remove(k);
        }
    }

    let resolved = resolve_request(req, env.clone());
    let client = http_client(&state).await?;
    let response = client.execute(&resolved).await.map_err(|e| e.to_string())?;

    let mut tests_logs = Vec::new();
    let mut tests = Vec::new();
    if let Some(script) = tests_script.as_ref().filter(|s| !s.trim().is_empty()) {
        let script_res = ScriptResponse {
            status: response.status,
            body: String::from_utf8_lossy(&response.body.bytes).to_string(),
            headers: response
                .headers
                .iter()
                .map(|h| ScriptHeader {
                    name: h.name.clone(),
                    value: h.value.clone(),
                })
                .collect(),
        };
        let outcome = run_tests(script, script_res, env).map_err(|e| e.to_string())?;
        tests_logs = outcome.logs;
        tests = outcome.tests;
        for (k, v) in outcome.env_updates {
            // A set in tests overrides a tombstone from pre-request.
            env_unsets.retain(|name| name != &k);
            env_updates.insert(k, v);
        }
        for k in outcome.env_unsets {
            env_updates.remove(&k);
            if !env_unsets.contains(&k) {
                env_unsets.push(k);
            }
        }
    }

    Ok(SendOutcome {
        response,
        pre_request_logs,
        tests_logs,
        tests,
        env_updates,
        env_unsets,
    })
}

// ---- WebSocket ----------------------------------------------------------

/// Open a WebSocket connection identified by `connection_id`. The
/// frontend chooses the id (typically a per-tab `nanoid`) so events
/// can be routed back to the right view. Calling `ws_connect` twice
/// with the same id replaces the previous connection.
///
/// Events are emitted on `ws://event` with payload
/// `{ connection_id, kind, ...fields }`:
///   - `kind: "connected"`
///   - `kind: "message"`, `direction`, `body`, `timestamp_ms`
///   - `kind: "binary"`,  `direction`, `bytes`, `timestamp_ms`
///   - `kind: "closed"`,  `code`, `reason`
///   - `kind: "error"`,   `message`
#[tauri::command]
async fn ws_connect(
    app: tauri::AppHandle,
    registry: State<'_, WsRegistry>,
    connection_id: String,
    url: String,
    subprotocols: Option<Vec<String>>,
    headers: Option<Vec<(String, String)>>,
    env: Option<HashMap<String, String>>,
) -> Result<(), String> {
    // Resolve `{{var}}` in url + headers so the WS picks up env
    // variables the same way the HTTP engine does.
    let mut resolver = Resolver::new(env.unwrap_or_default());
    let resolved_url = resolver.resolve(&url);
    let resolved_headers = headers
        .unwrap_or_default()
        .into_iter()
        .map(|(k, v)| (resolver.resolve(&k), resolver.resolve(&v)))
        .collect();
    let handle = ws_core::connect(WsConnectOptions {
        url: resolved_url,
        subprotocols: subprotocols.unwrap_or_default(),
        headers: resolved_headers,
    })?;

    // Spawn an event pump. The pump owns a clone of the receiver via
    // mem::take so we can keep the WsHandle in the registry with a
    // sender channel and a JoinHandle.
    let app_clone = app.clone();
    let id_for_pump = connection_id.clone();
    let registry_for_drop = registry.inner().clone();
    let mut handle = handle;
    let mut events = std::mem::replace(&mut handle.events, tokio::sync::mpsc::channel(1).1);
    tokio::spawn(async move {
        while let Some(ev) = events.recv().await {
            let payload = ws_event_to_payload(&id_for_pump, &ev);
            let _ = app_clone.emit("ws://event", payload);
            if matches!(ev, WsEvent::Closed { .. } | WsEvent::Error(_)) {
                registry_for_drop.lock().await.remove(&id_for_pump);
                break;
            }
        }
    });

    registry
        .inner()
        .lock()
        .await
        .insert(connection_id, handle);
    Ok(())
}

#[tauri::command]
async fn ws_send(
    registry: State<'_, WsRegistry>,
    connection_id: String,
    text: String,
) -> Result<(), String> {
    let guard = registry.inner().lock().await;
    let handle = guard
        .get(&connection_id)
        .ok_or_else(|| format!("no live connection: {connection_id}"))?;
    handle.send_text(text).map_err(str::to_string)
}

#[tauri::command]
async fn ws_close(
    registry: State<'_, WsRegistry>,
    connection_id: String,
) -> Result<(), String> {
    let mut guard = registry.inner().lock().await;
    if let Some(handle) = guard.remove(&connection_id) {
        handle.close();
    }
    Ok(())
}

fn ws_event_to_payload(connection_id: &str, ev: &WsEvent) -> serde_json::Value {
    use serde_json::json;
    match ev {
        WsEvent::Connected => json!({ "connection_id": connection_id, "kind": "connected" }),
        WsEvent::Message {
            direction,
            body,
            timestamp_ms,
        } => json!({
            "connection_id": connection_id,
            "kind": "message",
            "direction": direction.as_str(),
            "body": body,
            "timestamp_ms": timestamp_ms.to_string(),
        }),
        WsEvent::Binary {
            direction,
            bytes,
            timestamp_ms,
        } => json!({
            "connection_id": connection_id,
            "kind": "binary",
            "direction": direction.as_str(),
            "bytes": bytes,
            "timestamp_ms": timestamp_ms.to_string(),
        }),
        WsEvent::Closed { code, reason } => json!({
            "connection_id": connection_id,
            "kind": "closed",
            "code": code,
            "reason": reason,
        }),
        WsEvent::Error(msg) => json!({
            "connection_id": connection_id,
            "kind": "error",
            "message": msg,
        }),
    }
}

#[allow(dead_code)]
fn _ws_direction_typecheck() -> WsDirection {
    WsDirection::Incoming
}

/// Render the request as a `curl` invocation. Resolves `{{var}}` first using
/// the supplied environment so the curl preview matches what would be sent.
#[tauri::command]
fn request_to_curl(req: HttpRequest, env: Option<HashMap<String, String>>) -> String {
    let resolved = resolve_request(req, env.unwrap_or_default());
    curl::to_curl(&resolved)
}

/// Parse a pasted `curl` command into a wire request. Multi-line
/// commands with backslash continuations are accepted.
#[tauri::command]
fn curl_to_request(input: String) -> Result<HttpRequest, String> {
    from_curl(&input).map_err(|e| e.to_string())
}

/// Export the open workspace as a Postman v2.1 collection JSON.
///
/// Writes a pretty-printed `.postman_collection.json` next to the
/// workspace root by default; the caller can override `target_path`
/// to write anywhere else. Returns the resolved output path.
#[tauri::command]
fn postman_export(workspace_root: String, target_path: Option<String>) -> Result<String, String> {
    let ws = Workspace::open(&workspace_root).map_err(|e| e.to_string())?;
    let json = postman_export::to_postman_v21_string(&ws.manifest.name, &ws.tree)
        .map_err(|e| e.to_string())?;

    let path = match target_path {
        Some(p) => PathBuf::from(p),
        None => {
            let slug = slugify(&ws.manifest.name);
            Path::new(&workspace_root).join(format!("{slug}.postman_collection.json"))
        }
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

/// Export a single run as a HAR 1.2 archive.
///
/// `request` and `response` are JSON values matching `HttpRequest` /
/// `HttpResponse` (the UI already has them as part of its run history,
/// so we don't make it round-trip through the loader). Returns the
/// path of the freshly written `.har` file.
#[tauri::command]
fn run_export_har(
    request: serde_json::Value,
    response: serde_json::Value,
    started_at_iso8601: String,
    target_path: String,
) -> Result<String, String> {
    let req: HttpRequest = serde_json::from_value(request).map_err(|e| e.to_string())?;
    let res: HttpResponse = serde_json::from_value(response).map_err(|e| e.to_string())?;
    let json = har::to_har_string(&req, &res, &started_at_iso8601).map_err(|e| e.to_string())?;
    let path = PathBuf::from(target_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

/// Outcome of a Postman v2.1 import. The host UI uses these counts
/// to render a brief toast-style summary ("Imported 23 requests in 4
/// folders") and the absolute path of the freshly created folder so
/// the workspace tree can scroll to it.
#[derive(Debug, Serialize, Deserialize)]
pub struct PostmanImportReport {
    pub folder_path: String,
    pub folders_created: usize,
    pub requests_created: usize,
    pub variables_count: usize,
    pub env_path: Option<String>,
}

/// Import a Postman v2.1 collection JSON into the active workspace.
///
/// Creates a new top-level folder under `<workspace>/collections/` (or
/// directly at the workspace root if there is no collections dir),
/// named after the collection. Folders and requests are mirrored
/// 1:1 from the Postman tree, with names slugified for filesystem
/// safety. Collection variables become a fresh environment file
/// `environments/<slug>.env.argos.yaml`; if one exists already we
/// suffix with a counter to avoid clobbering.
///
/// `source` is either inline JSON (when `inline = true`) or a path to
/// a JSON file on disk. Reading on the Rust side avoids needing the
/// `@tauri-apps/plugin-fs` plugin in the UI.
#[tauri::command]
fn postman_import(
    workspace_root: String,
    source: String,
    inline: Option<bool>,
) -> Result<PostmanImportReport, String> {
    let json = read_inline_or_path(&source, inline.unwrap_or(false))?;
    let collection = postman::from_json(&json).map_err(|e| e.to_string())?;
    materialise_import(&workspace_root, collection)
}

/// Import a Bruno collection directory.
///
/// `source` is the absolute path of the Bruno collection root (the
/// folder containing `bruno.json`). The walker reads each `.bru` file,
/// rebuilds the folder tree, and materialises the result through the
/// same path Postman / Insomnia imports use.
#[tauri::command]
fn bruno_import(workspace_root: String, source: String) -> Result<PostmanImportReport, String> {
    let collection = bruno::from_dir(Path::new(&source)).map_err(|e| e.to_string())?;
    materialise_import(&workspace_root, collection)
}

/// Import an Insomnia v4 export. Mirrors `postman_import` —
/// re-uses `materialise_import` for the on-disk layout.
#[tauri::command]
fn insomnia_import(
    workspace_root: String,
    source: String,
    inline: Option<bool>,
) -> Result<PostmanImportReport, String> {
    let json = read_inline_or_path(&source, inline.unwrap_or(false))?;
    let collection = insomnia::from_json(&json).map_err(|e| e.to_string())?;
    materialise_import(&workspace_root, collection)
}

/// Import an OpenAPI 3.x document (JSON or YAML). Same materialisation
/// path as the other importers; the parser accepts either format and
/// silently picks the right one based on what serde manages to decode.
#[tauri::command]
fn openapi_import(
    workspace_root: String,
    source: String,
    inline: Option<bool>,
) -> Result<PostmanImportReport, String> {
    let text = read_inline_or_path(&source, inline.unwrap_or(false))?;
    let collection = openapi::from_str(&text).map_err(|e| e.to_string())?;
    materialise_import(&workspace_root, collection)
}

/// Sniff `path` and decide which importer to use. Drives the drag-drop
/// wizard: the UI calls this once per dropped path, then dispatches to
/// the matching `*_import` command.
///
/// Detection is intentionally conservative — we only return a concrete
/// format when the structural fingerprint is unambiguous, so the user
/// doesn't get silently funneled into the wrong importer.
#[tauri::command]
fn import_detect(path: String) -> Result<ImportDetectResult, String> {
    let p = Path::new(&path);
    if !p.exists() {
        return Err(format!("not found: {path}"));
    }
    if p.is_dir() {
        if p.join("bruno.json").is_file() {
            return Ok(ImportDetectResult {
                format: "bruno".into(),
                name: dir_name(p),
            });
        }
        return Ok(ImportDetectResult {
            format: "unknown".into(),
            name: dir_name(p),
        });
    }

    // Read up to ~64KB — enough for the headers of any reasonable spec
    // without slurping huge OpenAPI files that ship inline examples.
    use std::io::Read;
    let mut head = String::new();
    std::fs::File::open(p)
        .and_then(|mut f| {
            let mut buf = vec![0_u8; 64 * 1024];
            let n = f.read(&mut buf)?;
            head = String::from_utf8_lossy(&buf[..n]).into_owned();
            Ok(())
        })
        .map_err(|e| format!("read {path}: {e}"))?;

    let format = sniff_format(&head, p);
    Ok(ImportDetectResult {
        format: format.into(),
        name: p
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
    })
}

/// Result of [`import_detect`]. `format` is one of:
/// `"postman" | "insomnia" | "openapi" | "bruno" | "unknown"`.
#[derive(Debug, Serialize, Deserialize)]
pub struct ImportDetectResult {
    pub format: String,
    pub name: String,
}

fn dir_name(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Inspect the first few KB of a file plus its extension to classify
/// the importer. The fingerprint set:
///   - Postman: `info.schema` contains `"v2.1"`.
///   - Insomnia: top-level `"_type":"export"` OR `"__export_format"`.
///   - OpenAPI: `"openapi":"3.x"` (JSON) or `openapi: 3.x` (YAML).
fn sniff_format(head: &str, path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(str::to_ascii_lowercase);

    let trimmed = head.trim_start();
    let looks_json = trimmed.starts_with('{') || trimmed.starts_with('[');

    if looks_json || matches!(ext.as_deref(), Some("json")) {
        // Cheap structural matches first — avoid parsing the full JSON
        // when a substring search is unambiguous.
        if head.contains("schema.getpostman.com") && head.contains("v2.1") {
            return "postman";
        }
        if head.contains("\"_type\"") && head.contains("\"export\"") {
            return "insomnia";
        }
        if head.contains("\"__export_format\"") {
            return "insomnia";
        }
        if head.contains("\"openapi\"") && head.contains("\"3.") {
            return "openapi";
        }
        // Last-ditch: a full parse if the prefix landed; tolerate
        // partial reads by parsing the head into Value::Null on
        // failure.
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(head) {
            if v.get("info")
                .and_then(|i| i.get("schema"))
                .and_then(|s| s.as_str())
                .is_some_and(|s| s.contains("v2.1"))
            {
                return "postman";
            }
            if v.get("_type").and_then(|t| t.as_str()) == Some("export") {
                return "insomnia";
            }
            if v.get("openapi")
                .and_then(|o| o.as_str())
                .is_some_and(|s| s.starts_with("3."))
            {
                return "openapi";
            }
        }
    }

    if matches!(ext.as_deref(), Some("yaml") | Some("yml")) {
        // Trivial YAML sniff — first non-blank, non-comment line that
        // starts with `openapi:` followed by `3.`.
        for line in head.lines() {
            let l = line.trim();
            if l.is_empty() || l.starts_with('#') {
                continue;
            }
            if let Some(rest) = l.strip_prefix("openapi:") {
                let v = rest.trim().trim_matches(|c: char| c == '"' || c == '\'');
                if v.starts_with("3.") {
                    return "openapi";
                }
            }
            // Don't keep scanning the whole head — OpenAPI puts the
            // version at the top.
            break;
        }
        // Some specs lead with `info:` or comments before `openapi:`;
        // fall back to a substring sniff before giving up.
        if head.contains("openapi: 3.") || head.contains("openapi: \"3.") {
            return "openapi";
        }
    }

    "unknown"
}

fn read_inline_or_path(source: &str, inline: bool) -> Result<String, String> {
    if inline {
        Ok(source.to_string())
    } else {
        std::fs::read_to_string(source).map_err(|e| format!("read {source}: {e}"))
    }
}

fn materialise_import(
    workspace_root: &str,
    collection: argos_core::imports::ImportedCollection,
) -> Result<PostmanImportReport, String> {
    let ws_root = Path::new(&workspace_root);
    if !ws_root.is_dir() {
        return Err(format!(
            "workspace root is not a directory: {workspace_root}"
        ));
    }

    let collections_root = if ws_root.join("collections").is_dir() {
        ws_root.join("collections")
    } else {
        ws_root.to_path_buf()
    };

    let base_slug = slugify(&collection.name);
    let mut target = collections_root.join(&base_slug);
    let mut counter = 1;
    while target.exists() {
        counter += 1;
        target = collections_root.join(format!("{base_slug}-{counter}"));
    }
    std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
    let mut folder_meta = Folder::new(&collection.name);
    folder_meta.description = collection.description.clone();
    folder_meta.save(&target).map_err(|e| e.to_string())?;

    let mut counts = (0_usize, 0_usize); // (folders, requests)
    write_import_items(&target, &collection.items, &mut counts)?;

    // Variables → fresh environment file. We don't stomp on an
    // existing one — pick a unique slug.
    let env_path = if collection.variables.is_empty() {
        None
    } else {
        let env_dir = ws_root.join("environments");
        std::fs::create_dir_all(&env_dir).map_err(|e| e.to_string())?;
        let mut env_slug = base_slug.clone();
        let mut env_path = env_dir.join(format!("{env_slug}.env.argos.yaml"));
        let mut env_counter = 1;
        while env_path.exists() {
            env_counter += 1;
            env_slug = format!("{base_slug}-{env_counter}");
            env_path = env_dir.join(format!("{env_slug}.env.argos.yaml"));
        }
        let mut env = Environment::new(&collection.name);
        env.variables = collection
            .variables
            .iter()
            .map(|(k, v)| EnvVar {
                name: k.clone(),
                value: v.clone(),
                enabled: true,
            })
            .collect();
        env.save(&env_path).map_err(|e| e.to_string())?;
        Some(env_path.to_string_lossy().into_owned())
    };

    Ok(PostmanImportReport {
        folder_path: target.to_string_lossy().into_owned(),
        folders_created: counts.0,
        requests_created: counts.1,
        variables_count: collection.variables.len(),
        env_path,
    })
}

fn write_import_items(
    parent: &Path,
    items: &[ImportItem],
    counts: &mut (usize, usize),
) -> Result<(), String> {
    let mut used_names: HashMap<String, u32> = HashMap::new();
    for item in items {
        match item {
            ImportItem::Folder {
                name,
                description,
                items,
            } => {
                let slug = unique_child_slug(parent, name, &mut used_names);
                let dir = parent.join(&slug);
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let mut meta = Folder::new(name);
                meta.description = description.clone();
                meta.save(&dir).map_err(|e| e.to_string())?;
                counts.0 += 1;
                write_import_items(&dir, items, counts)?;
            }
            ImportItem::Request { draft } => {
                let slug = unique_child_slug(parent, &draft.name, &mut used_names);
                let path = parent.join(format!("{slug}.argos.yaml"));
                draft.save(&path).map_err(|e| e.to_string())?;
                counts.1 += 1;
            }
        }
    }
    Ok(())
}

/// Compute a filesystem-safe child name unique within `parent`. We
/// keep a per-call counter so siblings with identical sluggified
/// names ("Get user", "GET-USER") don't clash.
fn unique_child_slug(parent: &Path, name: &str, used: &mut HashMap<String, u32>) -> String {
    let base = slugify(name);
    let key = base.clone();
    let n = used.entry(key).and_modify(|c| *c += 1).or_insert(0);
    let candidate = if *n == 0 {
        base.clone()
    } else {
        format!("{base}-{n}")
    };
    if !parent.join(&candidate).exists() && !parent.join(format!("{candidate}.argos.yaml")).exists()
    {
        return candidate;
    }
    // Fall back to numeric suffixes until we find a free slot.
    let mut i = used[&base];
    loop {
        i += 1;
        let try_slug = format!("{base}-{i}");
        if !parent.join(&try_slug).exists()
            && !parent.join(format!("{try_slug}.argos.yaml")).exists()
        {
            *used.get_mut(&base).unwrap() = i;
            return try_slug;
        }
    }
}

/// Translate an `argos_core::HttpBody` into a `ScriptBody` for the
/// scripting sandbox. `Raw` (binary) bodies don't have a JS-friendly
/// representation, so we hide them by returning `None` — the caller
/// keeps the original `Raw` body intact unless the script writes a
/// new one.
fn http_body_to_script(body: Option<&argos_core::HttpBody>) -> Option<ScriptBody> {
    match body? {
        argos_core::HttpBody::Text {
            content,
            content_type,
        } => Some(ScriptBody::Text {
            content: content.clone(),
            content_type: content_type.clone(),
        }),
        argos_core::HttpBody::Json { value } => Some(ScriptBody::Json {
            value: value.clone(),
        }),
        argos_core::HttpBody::FormUrlEncoded { fields } => Some(ScriptBody::FormUrlEncoded {
            fields: fields
                .iter()
                .map(|(name, value)| ScriptFormField {
                    name: name.clone(),
                    value: value.clone(),
                })
                .collect(),
        }),
        argos_core::HttpBody::Raw { .. } => None,
    }
}

fn script_body_to_http(body: ScriptBody) -> argos_core::HttpBody {
    match body {
        ScriptBody::Text {
            content,
            content_type,
        } => argos_core::HttpBody::Text {
            content,
            content_type,
        },
        ScriptBody::Json { value } => argos_core::HttpBody::Json { value },
        ScriptBody::FormUrlEncoded { fields } => argos_core::HttpBody::FormUrlEncoded {
            fields: fields.into_iter().map(|f| (f.name, f.value)).collect(),
        },
    }
}

fn parse_http_method(s: &str) -> Option<HttpMethod> {
    match s.to_ascii_uppercase().as_str() {
        "GET" => Some(HttpMethod::Get),
        "POST" => Some(HttpMethod::Post),
        "PUT" => Some(HttpMethod::Put),
        "PATCH" => Some(HttpMethod::Patch),
        "DELETE" => Some(HttpMethod::Delete),
        "HEAD" => Some(HttpMethod::Head),
        "OPTIONS" => Some(HttpMethod::Options),
        _ => None,
    }
}

/// Apply variable substitution to a request's URL, headers, query and body.
fn resolve_request(mut req: HttpRequest, env: HashMap<String, String>) -> HttpRequest {
    let mut r = Resolver::new(env);
    req.url = r.resolve(&req.url);
    for h in &mut req.headers {
        h.name = r.resolve(&h.name);
        h.value = r.resolve(&h.value);
    }
    for (k, v) in &mut req.query {
        *k = r.resolve(k);
        *v = r.resolve(v);
    }
    if let Some(body) = req.body.as_mut() {
        match body {
            argos_core::HttpBody::Text {
                content,
                content_type,
            } => {
                *content = r.resolve(content);
                *content_type = r.resolve(content_type);
            }
            argos_core::HttpBody::Json { value } => {
                resolve_json(value, &mut r);
            }
            argos_core::HttpBody::FormUrlEncoded { fields } => {
                for (k, v) in fields {
                    *k = r.resolve(k);
                    *v = r.resolve(v);
                }
            }
            argos_core::HttpBody::Raw { content_type, .. } => {
                *content_type = r.resolve(content_type);
            }
        }
    }
    req
}

fn resolve_json(value: &mut serde_json::Value, r: &mut Resolver) {
    match value {
        serde_json::Value::String(s) => *s = r.resolve(s),
        serde_json::Value::Array(arr) => {
            for v in arr {
                resolve_json(v, r);
            }
        }
        serde_json::Value::Object(map) => {
            for v in map.values_mut() {
                resolve_json(v, r);
            }
        }
        _ => {}
    }
}

// ---- workspace -----------------------------------------------------------

/// Open an existing workspace at `path`. Adds the path to recents on success
/// and starts the file watcher.
#[tauri::command]
fn workspace_open(app: tauri::AppHandle, path: String) -> Result<Workspace, String> {
    let ws = Workspace::open(&path).map_err(|e| e.to_string())?;
    let _ = recents_add(&app, &ws.root);
    let _ = watcher::start(&app, &ws.root);
    Ok(ws)
}

/// Create a new workspace at `path` with the given display name. Also starts
/// the file watcher.
#[tauri::command]
fn workspace_create(
    app: tauri::AppHandle,
    path: String,
    name: String,
) -> Result<Workspace, String> {
    let ws = Workspace::create(&path, &name).map_err(|e| e.to_string())?;
    let _ = recents_add(&app, &ws.root);
    let _ = watcher::start(&app, &ws.root);
    Ok(ws)
}

/// Stop the currently-running file watcher. Frontend calls this when the
/// user closes the workspace (returns to the welcome screen).
#[tauri::command]
fn workspace_close(app: tauri::AppHandle) {
    watcher::stop(&app);
}

/// Re-scan the workspace at `path` (called after external file changes).
#[tauri::command]
fn workspace_reload(path: String) -> Result<Workspace, String> {
    Workspace::open(&path).map_err(|e| e.to_string())
}

/// Persist a request draft to disk.
///
/// `path` is the absolute path to the YAML file the draft should live in. If
/// the file doesn't exist yet (new request), the parent dir is created
/// implicitly by the atomic-write helper.
#[tauri::command]
fn request_save(path: String, draft: RequestDraft) -> Result<(), String> {
    draft.save(Path::new(&path)).map_err(|e| e.to_string())
}

/// Persist an environment file at `path`.
#[tauri::command]
fn environment_save(path: String, env: Environment) -> Result<(), String> {
    env.save(Path::new(&path)).map_err(|e| e.to_string())
}

/// Create a new environment file under the workspace's environments dir.
/// Returns the absolute path of the new file.
#[tauri::command]
fn environment_create(env_dir: String, name: String) -> Result<String, String> {
    let dir = Path::new(&env_dir);
    if !dir.is_dir() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let slug = slugify(&name);
    let path = dir.join(format!("{slug}.env.argos.yaml"));
    if path.exists() {
        return Err(format!("already exists: {}", path.display()));
    }
    let env = Environment::new(&name);
    env.save(&path).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

/// Delete an environment file at `path`.
#[tauri::command]
fn environment_delete(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    if !p.exists() {
        return Ok(());
    }
    std::fs::remove_file(p).map_err(|e| e.to_string())
}

// ---- run history ---------------------------------------------------------

/// Persisted run record. Uses opaque JSON values for `request` / `response`
/// so we don't have to evolve the format here every time those wire types
/// change. The cap on disk matches `MAX_RUNS_PER_TAB` in the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedRun {
    id: String,
    started_at_ms: u64,
    request: serde_json::Value,
    response: serde_json::Value,
}

const RUNS_PER_REQUEST_CAP: usize = 100;

/// Compute the on-disk JSON path for a request's run history.
///
/// `request_path` is the absolute path of the YAML file under the
/// workspace root. We slugify it (replace `/` with `__`, strip the
/// `.argos.yaml` suffix) so the runs dir stays flat and predictable.
fn runs_file_for(workspace_root: &Path, request_path: &Path) -> Option<PathBuf> {
    let rel = request_path.strip_prefix(workspace_root).ok()?;
    let mut key = rel.to_string_lossy().replace(['/', '\\'], "__");
    if let Some(stripped) = key.strip_suffix(".argos.yaml") {
        key = stripped.to_string();
    }
    Some(workspace_root.join("runs").join(format!("{key}.json")))
}

/// Append a run to disk, keeping at most `RUNS_PER_REQUEST_CAP`.
#[tauri::command]
fn run_record(
    workspace_root: String,
    request_path: String,
    run: serde_json::Value,
) -> Result<(), String> {
    let ws_root = Path::new(&workspace_root);
    let req_path = Path::new(&request_path);
    let Some(file) = runs_file_for(ws_root, req_path) else {
        return Err(format!(
            "request path is outside workspace: {}",
            req_path.display()
        ));
    };

    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let mut runs: Vec<serde_json::Value> = if file.exists() {
        let data = std::fs::read_to_string(&file).map_err(|e| e.to_string())?;
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        Vec::new()
    };

    runs.insert(0, run);
    if runs.len() > RUNS_PER_REQUEST_CAP {
        runs.truncate(RUNS_PER_REQUEST_CAP);
    }

    let tmp = file.with_extension("json.tmp");
    let body = serde_json::to_vec_pretty(&runs).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, body).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &file).map_err(|e| e.to_string())?;
    Ok(())
}

/// Load up to `RUNS_PER_REQUEST_CAP` persisted runs for a request, newest
/// first. Missing file → empty array.
#[tauri::command]
fn run_load(workspace_root: String, request_path: String) -> Result<Vec<PersistedRun>, String> {
    let ws_root = Path::new(&workspace_root);
    let req_path = Path::new(&request_path);
    let Some(file) = runs_file_for(ws_root, req_path) else {
        return Ok(Vec::new());
    };
    if !file.exists() {
        return Ok(Vec::new());
    }
    let data = std::fs::read_to_string(&file).map_err(|e| e.to_string())?;
    let runs: Vec<PersistedRun> = serde_json::from_str(&data).map_err(|e| e.to_string())?;
    Ok(runs)
}

/// Drop all persisted runs for a request.
#[tauri::command]
fn run_clear(workspace_root: String, request_path: String) -> Result<(), String> {
    let ws_root = Path::new(&workspace_root);
    let req_path = Path::new(&request_path);
    let Some(file) = runs_file_for(ws_root, req_path) else {
        return Ok(());
    };
    if file.exists() {
        std::fs::remove_file(&file).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Compute a filesystem-friendly slug from a human request name.
#[tauri::command]
fn slug(name: String) -> String {
    slugify(&name)
}

// ---- tree CRUD -----------------------------------------------------------

/// Create a sub-folder under `parent_dir` with display name `name`.
/// Writes a `_folder.argos.yaml` so the loader picks up the human name.
/// Returns the new folder's absolute path.
#[tauri::command]
fn tree_create_folder(parent_dir: String, name: String) -> Result<String, String> {
    let parent = Path::new(&parent_dir);
    if !parent.is_dir() {
        return Err(format!("not a directory: {}", parent.display()));
    }
    let slug = slugify(&name);
    let new_dir = parent.join(&slug);
    if new_dir.exists() {
        return Err(format!("already exists: {}", new_dir.display()));
    }
    std::fs::create_dir_all(&new_dir).map_err(|e| e.to_string())?;
    let folder = Folder::new(&name);
    folder.save(&new_dir).map_err(|e| e.to_string())?;
    Ok(new_dir.to_string_lossy().into_owned())
}

/// Create a new request file under `parent_dir`. Returns the new file path.
#[tauri::command]
fn tree_create_request(
    parent_dir: String,
    name: String,
    method: Option<HttpMethod>,
    protocol: Option<String>,
) -> Result<String, String> {
    let parent = Path::new(&parent_dir);
    if !parent.is_dir() {
        return Err(format!("not a directory: {}", parent.display()));
    }
    let slug = slugify(&name);
    let mut file_path = parent.join(format!("{slug}.argos.yaml"));
    let mut counter = 1;
    while file_path.exists() {
        counter += 1;
        file_path = parent.join(format!("{slug}-{counter}.argos.yaml"));
    }
    let req = match protocol.as_deref().unwrap_or("rest") {
        "graphql" => RequestDraft::new_graphql(&name, ""),
        "websocket" => RequestDraft::new_websocket(&name, ""),
        _ => RequestDraft::new_rest(&name, method.unwrap_or(HttpMethod::Get), ""),
    };
    req.save(&file_path).map_err(|e| e.to_string())?;
    Ok(file_path.to_string_lossy().into_owned())
}

/// Rename the file or folder at `path` to `new_name`. Folders' display
/// name in the YAML is updated separately by the editor; this command
/// only handles the filesystem rename.
#[tauri::command]
fn tree_rename(path: String, new_name: String) -> Result<String, String> {
    let p = Path::new(&path);
    if !p.exists() {
        return Err(format!("not found: {}", p.display()));
    }
    let parent = p.parent().ok_or_else(|| "no parent dir".to_string())?;
    let slug = slugify(&new_name);

    let new_path = if p.is_dir() {
        parent.join(slug)
    } else {
        // File rename — preserve `.argos.yaml` suffix.
        parent.join(format!("{slug}.argos.yaml"))
    };

    if new_path.exists() {
        return Err(format!("already exists: {}", new_path.display()));
    }
    std::fs::rename(p, &new_path).map_err(|e| e.to_string())?;

    // For folders, also update the inner _folder.argos.yaml display name.
    if new_path.is_dir() {
        if let Ok(mut f) = Folder::load(&new_path) {
            f.name = new_name;
            f.save(&new_path).ok();
        }
    }

    Ok(new_path.to_string_lossy().into_owned())
}

/// Delete the file or folder at `path`. Folders are removed recursively.
#[tauri::command]
fn tree_delete(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    if !p.exists() {
        return Ok(());
    }
    if p.is_dir() {
        std::fs::remove_dir_all(p).map_err(|e| e.to_string())
    } else {
        std::fs::remove_file(p).map_err(|e| e.to_string())
    }
}

/// Move the file or folder at `src` into the folder `dest_dir`.
/// Used by drag-n-drop reorder. Conflict on filename suffixes a counter.
#[tauri::command]
fn tree_move(src: String, dest_dir: String) -> Result<String, String> {
    let s = Path::new(&src);
    let d = Path::new(&dest_dir);
    if !s.exists() {
        return Err(format!("not found: {}", s.display()));
    }
    if !d.is_dir() {
        return Err(format!("not a directory: {}", d.display()));
    }
    // No-op if already inside dest.
    if let Some(parent) = s.parent() {
        if parent == d {
            return Ok(src);
        }
    }
    let file_name = s
        .file_name()
        .ok_or_else(|| "missing file name".to_string())?;

    let mut target = d.join(file_name);
    let mut counter = 1;
    while target.exists() {
        counter += 1;
        let stem = file_name.to_string_lossy();
        target = d.join(format!("{stem}-{counter}"));
    }
    std::fs::rename(s, &target).map_err(|e| e.to_string())?;
    Ok(target.to_string_lossy().into_owned())
}

// ---- recents -------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RecentEntry {
    path: PathBuf,
    /// Last-opened timestamp (millis since epoch). UI sorts by this.
    last_opened_ms: u128,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Recents {
    entries: Vec<RecentEntry>,
}

const MAX_RECENTS: usize = 12;

fn recents_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("recents.json"))
}

fn read_recents(app: &tauri::AppHandle) -> Recents {
    let Ok(path) = recents_path(app) else {
        return Recents::default();
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return Recents::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn write_recents(app: &tauri::AppHandle, r: &Recents) -> Result<(), String> {
    let path = recents_path(app)?;
    let body = serde_json::to_vec_pretty(r).map_err(|e| e.to_string())?;
    std::fs::write(&path, body).map_err(|e| e.to_string())
}

fn recents_add(app: &tauri::AppHandle, ws_path: &Path) -> Result<(), String> {
    let abs = ws_path
        .canonicalize()
        .unwrap_or_else(|_| ws_path.to_path_buf());
    let mut r = read_recents(app);
    r.entries.retain(|e| e.path != abs);
    r.entries.insert(
        0,
        RecentEntry {
            path: abs,
            last_opened_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0),
        },
    );
    if r.entries.len() > MAX_RECENTS {
        r.entries.truncate(MAX_RECENTS);
    }
    write_recents(app, &r)
}

#[tauri::command]
fn workspace_list_recent(app: tauri::AppHandle) -> Vec<RecentEntry> {
    // Filter out paths that no longer exist so the welcome screen stays clean.
    read_recents(&app)
        .entries
        .into_iter()
        .filter(|e| e.path.is_dir())
        .collect()
}

#[tauri::command]
fn workspace_clear_recent(app: tauri::AppHandle) -> Result<(), String> {
    write_recents(&app, &Recents::default())
}

// ---- entry point ---------------------------------------------------------

fn main() {
    argos_core::init_tracing();

    let state: AppState = Arc::new(OnceCell::new());
    let active_watcher = ActiveWatcher::default();
    let ws_registry: WsRegistry = Arc::new(Mutex::new(HashMap::new()));

    tauri::Builder::default()
        .manage(state)
        .manage(active_watcher)
        .manage(ws_registry)
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            core_version,
            ping,
            send_request,
            ws_connect,
            ws_send,
            ws_close,
            request_to_curl,
            curl_to_request,
            postman_import,
            postman_export,
            insomnia_import,
            bruno_import,
            openapi_import,
            import_detect,
            run_export_har,
            workspace_open,
            workspace_create,
            workspace_close,
            workspace_reload,
            workspace_list_recent,
            workspace_clear_recent,
            request_save,
            environment_save,
            environment_create,
            environment_delete,
            run_record,
            run_load,
            run_clear,
            slug,
            tree_create_folder,
            tree_create_request,
            tree_rename,
            tree_delete,
            tree_move,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Argos desktop app");
}

#[cfg(test)]
mod tests {
    use super::sniff_format;
    use std::path::Path;

    #[test]
    fn sniffs_postman_v21() {
        let head = r#"{ "info": { "name": "x", "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json" }, "item": [] }"#;
        assert_eq!(sniff_format(head, Path::new("x.json")), "postman");
    }

    #[test]
    fn sniffs_insomnia_v4() {
        let head = r#"{ "_type": "export", "__export_format": 4, "resources": [] }"#;
        assert_eq!(sniff_format(head, Path::new("x.json")), "insomnia");
    }

    #[test]
    fn sniffs_openapi_json() {
        let head = r#"{ "openapi": "3.0.3", "info": {} }"#;
        assert_eq!(sniff_format(head, Path::new("x.json")), "openapi");
    }

    #[test]
    fn sniffs_openapi_yaml() {
        let head = "openapi: 3.0.3\ninfo:\n  title: x\n";
        assert_eq!(sniff_format(head, Path::new("x.yaml")), "openapi");
    }

    #[test]
    fn sniffs_openapi_yaml_quoted_version() {
        let head = "openapi: \"3.1.0\"\n";
        assert_eq!(sniff_format(head, Path::new("x.yml")), "openapi");
    }

    #[test]
    fn unknown_when_not_recognised() {
        assert_eq!(sniff_format("hello world", Path::new("x.txt")), "unknown");
        assert_eq!(sniff_format("{}", Path::new("x.json")), "unknown");
    }
}
