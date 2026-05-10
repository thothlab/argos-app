// Prevents additional console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;

use argos_core::codegen::curl;
use argos_core::{HttpClient, HttpRequest, HttpResponse};
use tauri::State;
use tokio::sync::OnceCell;

/// HTTP client built lazily on first use and reused for the lifetime of the app.
///
/// reqwest's connection pool benefits from being shared across requests, and
/// building the client is non-trivial (TLS init, etc.). We hide the OnceCell
/// behind a Tauri-managed state so commands can `.get()` it.
type AppState = Arc<OnceCell<HttpClient>>;

async fn http_client(state: &AppState) -> Result<&HttpClient, String> {
    state
        .get_or_try_init(|| async { HttpClient::new().map_err(|e| e.to_string()) })
        .await
}

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

/// Execute one HTTP request via `argos-core` and return the buffered response.
///
/// Errors are stringified for transport across the IPC boundary; the UI maps
/// them to a friendly error state.
#[tauri::command]
async fn send_request(
    state: State<'_, AppState>,
    req: HttpRequest,
) -> Result<HttpResponse, String> {
    let client = http_client(&state).await?;
    client.execute(&req).await.map_err(|e| e.to_string())
}

/// Render the request as a `curl` invocation — useful for the UI's
/// "Copy as cURL" affordance.
#[tauri::command]
fn request_to_curl(req: HttpRequest) -> String {
    curl::to_curl(&req)
}

fn main() {
    argos_core::init_tracing();

    let state: AppState = Arc::new(OnceCell::new());

    tauri::Builder::default()
        .manage(state)
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            core_version,
            ping,
            send_request,
            request_to_curl,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Argos desktop app");
}
