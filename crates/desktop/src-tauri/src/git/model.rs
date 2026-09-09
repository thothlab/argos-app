//! Types crossing the Tauri boundary for the built-in Git client.
//!
//! Shapes follow `my-git` (Graft), whose engine this module is ported
//! from — same porcelain parsing, same `patch`-carrying hunks — so the
//! two projects' frontends can stay recognisably similar. Everything is
//! `camelCase` on the wire because the UI is TypeScript.

use serde::{Deserialize, Serialize};

/// git status of a changed file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileState {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
    Conflicted,
}

/// A changed file with its status and index/worktree staging flags.
///
/// `staged` and `unstaged` are independent: one edit can be half staged,
/// and the UI shows the file in both sections rather than picking one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStatus {
    pub path: String,
    pub status: FileState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    /// Has staged (index) changes relative to HEAD.
    pub staged: bool,
    /// Has unstaged (worktree) changes relative to the index.
    pub unstaged: bool,
}

/// An unfinished merge / rebase / cherry-pick / revert.
///
/// Travels with the status rather than as a separate call: a UI that
/// asks separately can render a "commit" button for a tree that is
/// mid-rebase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Operation {
    None,
    Merge,
    Rebase,
    CherryPick,
    Revert,
}

/// Everything the Git panel needs to render its header and file lists.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoStatus {
    /// Repository top-level, which may sit *above* the Argos workspace.
    pub repo_path: String,
    /// Workspace path relative to the repo root, `""` when they coincide.
    /// The UI uses it to offer "only this workspace" scoping.
    pub workspace_prefix: String,
    pub branch: String,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub detached: bool,
    pub operation: Operation,
    pub user_email: Option<String>,
    pub files: Vec<FileStatus>,
}

/// A branch (local or remote-tracking) for the branch list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchInfo {
    pub name: String,
    pub is_remote: bool,
    pub is_current: bool,
    pub upstream: Option<String>,
    /// Commits this branch is ahead / behind its upstream, when it has one.
    pub ahead: u32,
    pub behind: u32,
}

/// Kind of a ref label parsed out of a commit's `%D` decoration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RefKind {
    Head,
    Local,
    Remote,
    Tag,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefLabel {
    pub name: String,
    pub kind: RefKind,
}

/// One row of history.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitRow {
    pub id: String,
    pub short_id: String,
    pub summary: String,
    pub author: String,
    pub email: String,
    /// Author date, Unix seconds — formatting is the UI's business.
    pub timestamp: i64,
    pub parents: Vec<String>,
    pub refs: Vec<RefLabel>,
}

/// A commit with its body and touched files.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitDetails {
    pub row: CommitRow,
    pub body: String,
    pub files: Vec<FileStatus>,
}

/// One line of a diff hunk. `origin` is `" "`, `"+"` or `"-"`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    pub origin: String,
    pub content: String,
    pub old_no: Option<u32>,
    pub new_no: Option<u32>,
}

/// A diff hunk. `patch` is the **exact, self-contained** patch text (file
/// header + this hunk) so the UI can hand it straight back to `git apply`
/// for hunk-level staging without lossy reconstruction.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
    pub patch: String,
}

/// A file's diff against a chosen base.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub path: String,
    pub binary: bool,
    pub hunks: Vec<Hunk>,
}

/// One `git stash` entry.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StashEntry {
    /// `stash@{0}` — the addressable name, which shifts as entries are dropped.
    pub name: String,
    pub message: String,
    pub timestamp: i64,
}

/// Both streams and the exit code of an arbitrary `git` invocation.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}
