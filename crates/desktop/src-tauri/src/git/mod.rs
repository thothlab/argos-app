//! Built-in Git client.
//!
//! Argos workspaces are plain files in a git repository — that is the
//! whole premise of the format — so the app that edits them should be
//! able to commit them. This module is the backend for that: a thin,
//! stateless command surface over [`engine::Repo`].
//!
//! **Stateless on purpose.** Every command takes the workspace path and
//! rediscovers the repository root from it. The alternative — a cached
//! open repo in `AppState` — has to be invalidated when the user opens
//! another workspace, and the cost it saves is one `git rev-parse` per
//! call. The workspace may also sit *below* the repo root (a monorepo
//! with collections in a subfolder); `workspace_prefix` in the status
//! reports that, and paths are always repo-relative, as git prints them.

use std::path::Path;

pub mod engine;
pub mod model;

use engine::Repo;
use model::{BranchInfo, CommitDetails, CommitRow, FileDiff, RawOutput, RepoStatus, StashEntry};

fn repo(workspace: &str) -> Result<Repo, String> {
    Repo::discover(Path::new(workspace))
}

/// Whether `workspace` is inside a git repository at all — the Git
/// button asks before offering to open the panel.
#[tauri::command]
pub fn git_is_repo(workspace: String) -> bool {
    repo(&workspace).is_ok()
}

/// Initialise a repository at the workspace root. Refuses when one is
/// already there rather than nesting a second repo inside it.
#[tauri::command]
pub fn git_init(workspace: String) -> Result<RepoStatus, String> {
    if repo(&workspace).is_ok() {
        return Err("this folder is already in a git repository".into());
    }
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(&workspace)
        .args(["init", "-q"])
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    git_status(workspace)
}

#[tauri::command]
pub fn git_status(workspace: String) -> Result<RepoStatus, String> {
    let ws = Path::new(&workspace);
    repo(&workspace)?.status(ws)
}

#[tauri::command]
pub fn git_stage(workspace: String, paths: Vec<String>) -> Result<(), String> {
    repo(&workspace)?.stage(&paths)
}

#[tauri::command]
pub fn git_unstage(workspace: String, paths: Vec<String>) -> Result<(), String> {
    repo(&workspace)?.unstage(&paths)
}

/// Destructive: throws away working-tree changes. The UI confirms first.
#[tauri::command]
pub fn git_discard(workspace: String, paths: Vec<String>) -> Result<(), String> {
    repo(&workspace)?.discard(&paths)
}

/// Apply a hunk's patch text — staging (`cached`), unstaging
/// (`cached` + `reverse`) or discarding (`reverse`) a single hunk.
#[tauri::command]
pub fn git_apply_patch(
    workspace: String,
    patch: String,
    cached: bool,
    reverse: bool,
) -> Result<(), String> {
    repo(&workspace)?.apply_patch(&patch, cached, reverse)
}

#[tauri::command]
pub fn git_commit(
    workspace: String,
    paths: Vec<String>,
    message: String,
    amend: bool,
) -> Result<(), String> {
    repo(&workspace)?.commit(&paths, &message, amend)
}

/// The message of HEAD, for pre-filling the box when amending.
#[tauri::command]
pub fn git_head_message(workspace: String) -> Result<String, String> {
    let rows = repo(&workspace)?.log(None, 0, 1, None)?;
    let head = rows.first().ok_or("no commits yet")?;
    Ok(head.summary.clone())
}

#[tauri::command]
pub fn git_branches(workspace: String) -> Result<Vec<BranchInfo>, String> {
    repo(&workspace)?.branches()
}

#[tauri::command]
pub fn git_create_branch(
    workspace: String,
    name: String,
    from: Option<String>,
) -> Result<(), String> {
    repo(&workspace)?.create_branch(&name, from.as_deref())
}

#[tauri::command]
pub fn git_checkout(workspace: String, name: String, stash: bool) -> Result<(), String> {
    repo(&workspace)?.checkout(&name, stash)
}

#[tauri::command]
pub fn git_delete_branch(workspace: String, name: String, force: bool) -> Result<(), String> {
    repo(&workspace)?.delete_branch(&name, force)
}

#[tauri::command]
pub fn git_rename_branch(workspace: String, from: String, to: String) -> Result<(), String> {
    repo(&workspace)?.rename_branch(&from, &to)
}

#[tauri::command]
pub fn git_merge(workspace: String, name: String, no_ff: bool) -> Result<(), String> {
    repo(&workspace)?.merge(&name, no_ff)
}

#[tauri::command]
pub fn git_rebase(workspace: String, onto: String, autosquash: bool) -> Result<(), String> {
    repo(&workspace)?.rebase(&onto, autosquash)
}

#[tauri::command]
pub fn git_cherry_pick(workspace: String, commit: String) -> Result<(), String> {
    repo(&workspace)?.cherry_pick(&commit)
}

#[tauri::command]
pub fn git_revert(workspace: String, commit: String) -> Result<(), String> {
    repo(&workspace)?.revert(&commit)
}

/// `mode` is `soft` / `mixed` / `hard` — `hard` discards working-tree
/// changes, so the UI names it explicitly in its confirmation.
#[tauri::command]
pub fn git_reset(workspace: String, commit: String, mode: String) -> Result<(), String> {
    repo(&workspace)?.reset(&commit, &mode)
}

/// `action` is `continue` / `abort` / `skip` for whatever operation
/// (merge, rebase, cherry-pick, revert) is currently in flight.
#[tauri::command]
pub fn git_operation_step(workspace: String, action: String) -> Result<(), String> {
    repo(&workspace)?.operation_step(&action)
}

#[tauri::command]
pub fn git_push(workspace: String, mode: String) -> Result<(), String> {
    repo(&workspace)?.push(&mode)
}

#[tauri::command]
pub fn git_pull(workspace: String, rebase: bool) -> Result<(), String> {
    repo(&workspace)?.pull(rebase)
}

#[tauri::command]
pub fn git_fetch(workspace: String) -> Result<(), String> {
    repo(&workspace)?.fetch()
}

#[tauri::command]
pub fn git_stash_list(workspace: String) -> Result<Vec<StashEntry>, String> {
    repo(&workspace)?.stash_list()
}

#[tauri::command]
pub fn git_stash_push(
    workspace: String,
    message: String,
    include_untracked: bool,
) -> Result<(), String> {
    repo(&workspace)?.stash_push(&message, include_untracked)
}

/// `action` is `apply` / `pop` / `drop`.
#[tauri::command]
pub fn git_stash_action(workspace: String, name: String, action: String) -> Result<(), String> {
    repo(&workspace)?.stash_action(&name, &action)
}

/// History page. `rev` picks the starting ref (`--all` for every ref),
/// `path` limits it to one file's history.
#[tauri::command]
pub fn git_log(
    workspace: String,
    rev: Option<String>,
    skip: u32,
    limit: u32,
    path: Option<String>,
) -> Result<Vec<CommitRow>, String> {
    repo(&workspace)?.log(rev.as_deref(), skip, limit, path.as_deref())
}

#[tauri::command]
pub fn git_commit_details(workspace: String, id: String) -> Result<CommitDetails, String> {
    repo(&workspace)?.commit_details(&id)
}

/// `base` is `worktree`, `index`, or a commit id.
#[tauri::command]
pub fn git_diff(workspace: String, path: String, base: String) -> Result<FileDiff, String> {
    repo(&workspace)?.diff(&path, &base)
}

/// The console escape hatch: run an arbitrary `git` argv in the repo.
///
/// This is not a sandbox — it runs at the user's own privilege, like the
/// terminal they would otherwise switch to. It is what makes the panel a
/// *client* rather than a set of buttons: anything the UI does not model
/// is still one command away.
#[tauri::command]
pub fn git_exec(workspace: String, args: Vec<String>) -> Result<RawOutput, String> {
    repo(&workspace)?.exec_raw(&args)
}
