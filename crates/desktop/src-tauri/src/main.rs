// Prevents additional console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod watcher;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use std::collections::HashMap;

use argos_core::codegen::curl;
use argos_core::format::{slugify, Environment, Folder, RequestDraft};
use argos_core::{HttpClient, HttpMethod, HttpRequest, HttpResponse, Resolver, Workspace};
use argos_scripting::{
    run_pre_request, run_tests, ScriptBody, ScriptFormField, ScriptHeader, ScriptRequest,
    ScriptResponse, TestResult,
};
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

/// Render the request as a `curl` invocation. Resolves `{{var}}` first using
/// the supplied environment so the curl preview matches what would be sent.
#[tauri::command]
fn request_to_curl(req: HttpRequest, env: Option<HashMap<String, String>>) -> String {
    let resolved = resolve_request(req, env.unwrap_or_default());
    curl::to_curl(&resolved)
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
    let req = RequestDraft::new_rest(&name, method.unwrap_or(HttpMethod::Get), "");
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
