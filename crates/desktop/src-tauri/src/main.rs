// Prevents additional console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod watcher;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use std::collections::HashMap;

use argos_core::codegen::curl;
use argos_core::format::{slugify, RequestDraft};
use argos_core::{HttpClient, HttpRequest, HttpResponse, Resolver, Workspace};
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};
use tokio::sync::OnceCell;

use watcher::ActiveWatcher;

// ---- shared state --------------------------------------------------------

/// HTTP client built lazily on first use and reused for the lifetime of the app.
type AppState = Arc<OnceCell<HttpClient>>;

async fn http_client(state: &AppState) -> Result<&HttpClient, String> {
    state
        .get_or_try_init(|| async { HttpClient::new().map_err(|e| e.to_string()) })
        .await
}

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

/// Execute one HTTP request via `argos-core` and return the buffered response.
///
/// `env` carries the active environment's variables (plus secrets) so the
/// backend can resolve `{{name}}` placeholders before sending. Pass an empty
/// map for a "no env active" send — placeholders will then be left verbatim.
#[tauri::command]
async fn send_request(
    state: State<'_, AppState>,
    req: HttpRequest,
    env: Option<HashMap<String, String>>,
) -> Result<HttpResponse, String> {
    let resolved = resolve_request(req, env.unwrap_or_default());
    let client = http_client(&state).await?;
    client.execute(&resolved).await.map_err(|e| e.to_string())
}

/// Render the request as a `curl` invocation. Resolves `{{var}}` first using
/// the supplied environment so the curl preview matches what would be sent.
#[tauri::command]
fn request_to_curl(req: HttpRequest, env: Option<HashMap<String, String>>) -> String {
    let resolved = resolve_request(req, env.unwrap_or_default());
    curl::to_curl(&resolved)
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

/// Compute a filesystem-friendly slug from a human request name.
#[tauri::command]
fn slug(name: String) -> String {
    slugify(&name)
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

    tauri::Builder::default()
        .manage(state)
        .manage(active_watcher)
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            core_version,
            ping,
            send_request,
            request_to_curl,
            workspace_open,
            workspace_create,
            workspace_close,
            workspace_reload,
            workspace_list_recent,
            workspace_clear_recent,
            request_save,
            slug,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Argos desktop app");
}
