//! Workspace file watcher.
//!
//! Background thread driven by `notify-debouncer-mini` that watches the
//! workspace root recursively. When `*.argos.yaml` files change (writes,
//! renames, deletions, new files) we emit an `argos:workspace-changed`
//! Tauri event with the workspace root as payload. The frontend reacts by
//! re-fetching the tree.
//!
//! Only one watcher runs at a time — switching workspace stops the previous
//! one cleanly. The frontend calls `watch_workspace_start(path)` after a
//! successful open / create, and `watch_workspace_stop()` on close.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode, DebouncedEvent};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

const DEBOUNCE: Duration = Duration::from_millis(400);
pub const EVENT_NAME: &str = "argos:workspace-changed";

/// Event payload sent to the frontend on relevant file activity.
#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceChangedEvent {
    pub root: PathBuf,
    /// Paths that triggered the notification — useful for richer UI later
    /// (e.g. flashing the changed row in the tree).
    pub paths: Vec<PathBuf>,
}

/// Holder for the live watcher; dropping it cleanly shuts the background
/// thread.
pub struct WatcherState {
    /// Boxed-trait object so we can swap implementations later without
    /// changing the public command signatures.
    _debouncer: Box<dyn std::any::Any + Send + Sync>,
    /// Root directory being watched (kept for future diagnostics / status UI).
    #[allow(dead_code)]
    pub root: PathBuf,
}

/// Tauri-managed wrapper.
#[derive(Default)]
pub struct ActiveWatcher(pub Mutex<Option<WatcherState>>);

/// Start watching `root`. If a previous watcher exists, it is stopped first.
///
/// Errors are surfaced to the frontend as plain strings — the watcher is
/// best-effort, the user can always trigger a manual reload from the UI.
pub fn start(app: &AppHandle, root: &Path) -> Result<(), String> {
    let abs = root
        .canonicalize()
        .map_err(|e| format!("canonicalize {}: {e}", root.display()))?;

    // Drop any previous watcher first.
    if let Some(state) = app
        .state::<ActiveWatcher>()
        .0
        .lock()
        .ok()
        .and_then(|mut g| g.take())
    {
        drop(state);
    }

    let app_for_thread = app.clone();
    let root_for_thread = abs.clone();

    let mut debouncer = new_debouncer(DEBOUNCE, move |res: Result<Vec<DebouncedEvent>, _>| {
        let Ok(events) = res else { return };
        let interesting: Vec<PathBuf> = events
            .into_iter()
            .map(|e| e.path)
            .filter(|p| is_relevant(p))
            .collect();
        if interesting.is_empty() {
            return;
        }
        let payload = WorkspaceChangedEvent {
            root: root_for_thread.clone(),
            paths: interesting,
        };
        // Fire-and-forget — emit failure usually means the window has been
        // closed; nothing useful to do here.
        let _ = app_for_thread.emit(EVENT_NAME, payload);
    })
    .map_err(|e| format!("create debouncer: {e}"))?;

    debouncer
        .watcher()
        .watch(&abs, RecursiveMode::Recursive)
        .map_err(|e| format!("watch {}: {e}", abs.display()))?;

    let state = WatcherState {
        _debouncer: Box::new(debouncer),
        root: abs,
    };

    if let Ok(mut guard) = app.state::<ActiveWatcher>().0.lock() {
        *guard = Some(state);
    }
    Ok(())
}

/// Stop the current watcher (if any).
pub fn stop(app: &AppHandle) {
    if let Ok(mut guard) = app.state::<ActiveWatcher>().0.lock() {
        guard.take(); // drop runs the debouncer's shutdown
    }
}

/// Predicate for deciding whether a watcher event should trigger a reload.
///
/// We care about `.argos.yaml` files anywhere in the tree; everything else
/// (READMEs, OpenAPI specs, run history) is ignored so the UI doesn't churn.
fn is_relevant(path: &Path) -> bool {
    // Direct case: a YAML file we own.
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    if name == "argos.yaml" || name.ends_with(".argos.yaml") {
        return true;
    }
    // Directory events (create/delete) — match if the path *or any ancestor*
    // ends with `.argos.yaml` is unusual; we mostly care about the YAML
    // file events themselves, which notify reports separately.
    false
}
