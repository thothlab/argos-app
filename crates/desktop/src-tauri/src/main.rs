// Prevents additional console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Returns the embedded `argos-core` version string.
///
/// Used by the UI to detect version mismatch between core and shell.
#[tauri::command]
fn core_version() -> String {
    argos_core::version().to_string()
}

/// Returns a friendly health-check string.
#[tauri::command]
fn ping() -> &'static str {
    "pong"
}

fn main() {
    argos_core::init_tracing();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![core_version, ping])
        .run(tauri::generate_context!())
        .expect("error while running Argos desktop app");
}
