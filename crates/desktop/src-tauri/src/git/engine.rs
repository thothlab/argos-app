//! The git backend: shell out to the system `git`.
//!
//! Ported from `my-git` (Graft), whose engine made the same call and for
//! the same reason: the user's `git` already has their SSH keys, their
//! credential helper, their hooks, their commit signing and their
//! `includeIf` config. A library-backed engine reimplements all of that
//! badly, and the first thing it breaks is a push against a private
//! remote.
//!
//! Every invocation goes through [`Repo::git`] (fails on non-zero exit,
//! carrying git's stderr verbatim) or [`Repo::exec_raw`] (returns the
//! exit code as data — used for the console, where a non-zero exit *is*
//! the answer the user asked for). Both run with `GIT_TERMINAL_PROMPT=0`
//! and no editor: an interactive prompt inside a webview app is a hang.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::model::{
    BranchInfo, CommitDetails, CommitRow, DiffLine, FileDiff, FileState, FileStatus, Hunk,
    Operation, RawOutput, RefKind, RefLabel, RepoStatus, StashEntry,
};

/// Field separator inside `--format` strings: a byte no commit message,
/// author name or ref can contain.
const SEP: &str = "\u{1f}";
/// Record separator for multi-line `--format` output.
const EOR: &str = "\u{1e}";

pub type Result<T> = std::result::Result<T, String>;

#[derive(Debug)]
pub struct Repo {
    root: PathBuf,
}

impl Repo {
    /// Repository containing `path`, or an error naming what is actually
    /// wrong.
    ///
    /// The `metadata` pre-flight keeps the two non-git failures from
    /// being reported as git ones: a folder that is gone (a remembered
    /// workspace on an unmounted volume) and a folder macOS refuses to
    /// let this app read (TCC). "not a git repository" is a true
    /// sentence about the wrong thing in both cases.
    pub fn discover(path: &Path) -> Result<Self> {
        if let Err(e) = std::fs::metadata(path) {
            let shown = path.display();
            return Err(match e.kind() {
                std::io::ErrorKind::NotFound => format!("no such folder: {shown}"),
                std::io::ErrorKind::PermissionDenied => format!(
                    "{shown} cannot be read: macOS has not granted Argos access to that folder"
                ),
                _ => format!("{shown}: {e}"),
            });
        }
        let out = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(["rev-parse", "--show-toplevel"])
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .map_err(|e| format!("cannot run git: {e}"))?;
        if !out.status.success() {
            return Err(format!("{} is not inside a git repository", path.display()));
        }
        Ok(Self {
            root: PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()),
        })
    }

    fn command(&self) -> Command {
        let mut c = Command::new("git");
        c.arg("-C")
            .arg(&self.root)
            .stdin(Stdio::null())
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_EDITOR", "false")
            .env("GIT_SEQUENCE_EDITOR", "false")
            .env_remove("GIT_ASKPASS")
            .env_remove("SSH_ASKPASS");
        c
    }

    /// Run git, capturing raw stdout. Non-zero exit becomes an error
    /// carrying git's own stderr — the UI shows it verbatim rather than
    /// inventing friendlier prose that hides which ref was rejected.
    fn git_bytes(&self, args: &[&str]) -> Result<Vec<u8>> {
        let out = self
            .command()
            .args(args)
            .output()
            .map_err(|e| format!("cannot run git: {e}"))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(if stderr.is_empty() {
                format!("git {} failed", args.join(" "))
            } else {
                stderr
            });
        }
        Ok(out.stdout)
    }

    fn git(&self, args: &[&str]) -> Result<String> {
        Ok(String::from_utf8_lossy(&self.git_bytes(args)?).to_string())
    }

    /// Run an arbitrary command the user typed in the console panel.
    ///
    /// A non-zero exit is **not** an error here: `git branch -D` on an
    /// unmerged branch prints exactly what the user opened the console
    /// to read. Only a failure to spawn git at all is an `Err`.
    pub fn exec_raw(&self, args: &[String]) -> Result<RawOutput> {
        let out = self
            .command()
            .args(args)
            .output()
            .map_err(|e| format!("cannot run git: {e}"))?;
        Ok(RawOutput {
            stdout: String::from_utf8_lossy(&out.stdout).to_string(),
            stderr: String::from_utf8_lossy(&out.stderr).to_string(),
            exit_code: out.status.code().unwrap_or(-1),
        })
    }

    // ── status ──────────────────────────────────────────────────────────

    /// Working-tree status: branch, tracking counts, unfinished
    /// operation and every changed file.
    pub fn status(&self, workspace: &Path) -> Result<RepoStatus> {
        // porcelain=v2 gives per-side staging + rename detail; --branch
        // adds the branch/ahead/behind headers; -z makes paths NUL-safe.
        let out = self.git_bytes(&["status", "--porcelain=v2", "--branch", "-z", "-unormal"])?;

        let mut branch = String::from("(unknown)");
        let mut upstream = None;
        let (mut ahead, mut behind) = (0u32, 0u32);
        let mut detached = false;
        let mut files = Vec::new();

        // Records are NUL-terminated; a rename record ('2') is followed
        // by an extra token holding its source path, so we index and
        // look ahead rather than iterate.
        let tokens: Vec<&[u8]> = out.split(|&c| c == 0).collect();
        let mut i = 0;
        while i < tokens.len() {
            let tok = tokens[i];
            if tok.is_empty() {
                i += 1;
                continue;
            }
            let s = String::from_utf8_lossy(tok);
            match s.as_bytes()[0] as char {
                '#' => {
                    let rest = &s[2..];
                    if let Some(v) = rest.strip_prefix("branch.head ") {
                        if v == "(detached)" {
                            detached = true;
                        } else {
                            branch = v.to_string();
                        }
                    } else if let Some(v) = rest.strip_prefix("branch.upstream ") {
                        upstream = Some(v.to_string());
                    } else if let Some(v) = rest.strip_prefix("branch.ab ") {
                        for part in v.split_whitespace() {
                            if let Some(n) = part.strip_prefix('+') {
                                ahead = n.parse().unwrap_or(0);
                            } else if let Some(n) = part.strip_prefix('-') {
                                behind = n.parse().unwrap_or(0);
                            }
                        }
                    }
                    i += 1;
                }
                '1' => {
                    // "1 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>"
                    let f: Vec<&str> = s.splitn(9, ' ').collect();
                    if f.len() == 9 {
                        files.push(make_status(f[1], f[8].to_string(), None, false));
                    }
                    i += 1;
                }
                '2' => {
                    // "2 <XY> … <Xscore> <path>" + next token = source path
                    let f: Vec<&str> = s.splitn(10, ' ').collect();
                    let orig = tokens
                        .get(i + 1)
                        .map(|t| String::from_utf8_lossy(t).to_string());
                    if f.len() == 10 {
                        files.push(make_status(f[1], f[9].to_string(), orig, true));
                    }
                    i += 2; // consume the source-path token
                }
                'u' => {
                    // unmerged: "u <XY> … <path>"
                    let f: Vec<&str> = s.splitn(11, ' ').collect();
                    if f.len() == 11 {
                        files.push(FileStatus {
                            path: f[10].to_string(),
                            status: FileState::Conflicted,
                            old_path: None,
                            staged: false,
                            unstaged: true,
                        });
                    }
                    i += 1;
                }
                '?' => {
                    files.push(FileStatus {
                        path: s[2..].to_string(),
                        status: FileState::Untracked,
                        old_path: None,
                        staged: false,
                        unstaged: true,
                    });
                    i += 1;
                }
                _ => i += 1,
            }
        }

        files.sort_by(|a, b| a.path.cmp(&b.path));

        Ok(RepoStatus {
            repo_path: self.root.display().to_string(),
            workspace_prefix: relative_prefix(&self.root, workspace),
            branch,
            upstream,
            ahead,
            behind,
            detached,
            operation: self.operation(),
            user_email: self.user_email(),
            files,
        })
    }

    /// Unfinished merge / rebase / cherry-pick / revert, detected from
    /// the marker files git itself keeps in `.git`.
    fn operation(&self) -> Operation {
        let git_dir = self.git_dir();
        let has = |p: &str| git_dir.join(p).exists();
        if has("rebase-merge") || has("rebase-apply") {
            Operation::Rebase
        } else if has("MERGE_HEAD") {
            Operation::Merge
        } else if has("CHERRY_PICK_HEAD") {
            Operation::CherryPick
        } else if has("REVERT_HEAD") {
            Operation::Revert
        } else {
            Operation::None
        }
    }

    /// `.git` for this repo — resolved rather than assumed, so worktrees
    /// and submodules (where `.git` is a file pointing elsewhere) work.
    fn git_dir(&self) -> PathBuf {
        self.git(&["rev-parse", "--absolute-git-dir"])
            .map_or_else(|_| self.root.join(".git"), |s| PathBuf::from(s.trim()))
    }

    /// `user.email` as this repository resolves it. A repository without
    /// an identity is not an error — it just has no "my commits" to mark.
    fn user_email(&self) -> Option<String> {
        let e = self.git(&["config", "--get", "user.email"]).ok()?;
        let e = e.trim().to_string();
        (!e.is_empty()).then_some(e)
    }

    // ── staging ─────────────────────────────────────────────────────────

    /// Stage exactly these paths. Existing files are `git add`ed;
    /// worktree deletions are staged via `git rm`. Never `git add -A` or
    /// `git add <dir>` — both sweep in files the user did not pick.
    pub fn stage(&self, paths: &[String]) -> Result<()> {
        let (deleted, existing): (Vec<&String>, Vec<&String>) =
            paths.iter().partition(|p| !self.root.join(p).exists());
        if !existing.is_empty() {
            let mut args = vec!["add", "--"];
            args.extend(existing.iter().map(|s| s.as_str()));
            self.git(&args)?;
        }
        if !deleted.is_empty() {
            let mut args = vec!["rm", "-q", "--"];
            args.extend(deleted.iter().map(|s| s.as_str()));
            self.git(&args)?;
        }
        Ok(())
    }

    /// Unstage paths, leaving the working tree untouched.
    pub fn unstage(&self, paths: &[String]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let mut args = vec!["restore", "--staged", "--"];
        args.extend(paths.iter().map(String::as_str));
        self.git(&args)?;
        Ok(())
    }

    /// Throw away local changes. A tracked file is restored from HEAD;
    /// a file absent from HEAD is unstaged and deleted from disk.
    ///
    /// Destructive and unrecoverable — the UI confirms before calling.
    pub fn discard(&self, paths: &[String]) -> Result<()> {
        for p in paths {
            let in_head = self.git(&["cat-file", "-e", &format!("HEAD:{p}")]).is_ok();
            if in_head {
                self.git(&["checkout", "HEAD", "--", p])?;
            } else {
                // Ignore a failure to unstage: an untracked file was
                // never in the index and `restore --staged` refuses it.
                let _ = self.git(&["restore", "--staged", "--", p]);
                let full = self.root.join(p);
                if full.is_dir() {
                    std::fs::remove_dir_all(&full).map_err(|e| format!("{p}: {e}"))?;
                } else if full.exists() {
                    std::fs::remove_file(&full).map_err(|e| format!("{p}: {e}"))?;
                }
            }
        }
        Ok(())
    }

    /// Apply a patch produced by [`Self::diff`] — hunk-level staging,
    /// unstaging (`reverse` + `cached`) and discarding.
    pub fn apply_patch(&self, patch: &str, cached: bool, reverse: bool) -> Result<()> {
        let mut args: Vec<&str> = vec!["apply", "--whitespace=nowarn"];
        if cached {
            args.push("--cached");
        }
        if reverse {
            args.push("--reverse");
        }
        args.push("-");

        use std::io::Write as _;
        let mut child = self
            .command()
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("cannot run git apply: {e}"))?;
        child
            .stdin
            .take()
            .ok_or("git apply: no stdin")?
            .write_all(patch.as_bytes())
            .map_err(|e| format!("git apply: {e}"))?;
        let out = child
            .wait_with_output()
            .map_err(|e| format!("git apply: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }

    /// Stage the given paths (when any) and commit the index.
    ///
    /// With no paths the index is committed as it stands, which is what
    /// the "commit staged" button means.
    pub fn commit(&self, paths: &[String], message: &str, amend: bool) -> Result<()> {
        if message.trim().is_empty() {
            return Err("commit message cannot be empty".into());
        }
        if !paths.is_empty() {
            self.stage(paths)?;
        }
        let mut args = vec!["commit", "-m", message];
        if amend {
            args.push("--amend");
        }
        self.git(&args)?;
        Ok(())
    }

    // ── branches ────────────────────────────────────────────────────────

    fn current_branch(&self) -> Result<String> {
        Ok(self
            .git(&["rev-parse", "--abbrev-ref", "HEAD"])?
            .trim()
            .to_string())
    }

    pub fn branches(&self) -> Result<Vec<BranchInfo>> {
        let format = format!(
            "--format=%(refname){SEP}%(refname:short){SEP}%(HEAD){SEP}%(upstream:short){SEP}%(upstream:track)"
        );
        let out = self.git(&["for-each-ref", &format, "refs/heads", "refs/remotes"])?;
        let mut v = Vec::new();
        for line in out.lines() {
            let f: Vec<&str> = line.split(SEP).collect();
            if f.len() < 3 {
                continue;
            }
            let (full, short, head) = (f[0], f[1], f[2]);
            let is_remote = full.starts_with("refs/remotes/");
            if is_remote && short.ends_with("/HEAD") {
                continue; // the origin/HEAD symref is not a branch
            }
            let (ahead, behind) = parse_track(f.get(4).copied().unwrap_or(""));
            v.push(BranchInfo {
                name: short.to_string(),
                is_remote,
                is_current: head == "*",
                upstream: f.get(3).filter(|s| !s.is_empty()).map(|s| (*s).to_string()),
                ahead,
                behind,
            });
        }
        Ok(v)
    }

    /// Create a branch from HEAD (or `from`) and switch to it.
    pub fn create_branch(&self, name: &str, from: Option<&str>) -> Result<()> {
        let mut args = vec!["checkout", "-b", name];
        if let Some(f) = from {
            args.push(f);
        }
        self.git(&args)?;
        Ok(())
    }

    /// Switch branch. `stash` shelves tracked+untracked changes first so
    /// a dirty tree does not block the switch; the UI offers that choice
    /// only after git has refused.
    pub fn checkout(&self, name: &str, stash: bool) -> Result<()> {
        if stash {
            self.git(&[
                "stash",
                "push",
                "-u",
                "-m",
                &format!("argos: switching to {name}"),
            ])?;
        }
        self.git(&["checkout", name])?;
        Ok(())
    }

    pub fn delete_branch(&self, name: &str, force: bool) -> Result<()> {
        // A remote-tracking name means "delete it on the remote", which
        // is a different command and a different blast radius.
        if let Some((remote, branch)) = name.split_once('/') {
            if self.remotes()?.iter().any(|r| r == remote) {
                self.git(&["push", remote, "--delete", branch])?;
                return Ok(());
            }
        }
        self.git(&["branch", if force { "-D" } else { "-d" }, name])?;
        Ok(())
    }

    pub fn rename_branch(&self, from: &str, to: &str) -> Result<()> {
        self.git(&["branch", "-m", from, to])?;
        Ok(())
    }

    pub fn remotes(&self) -> Result<Vec<String>> {
        Ok(self
            .git(&["remote"])?
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect())
    }

    // ── history-rewriting operations ────────────────────────────────────

    pub fn merge(&self, name: &str, no_ff: bool) -> Result<()> {
        let mut args = vec!["merge"];
        if no_ff {
            args.push("--no-ff");
        }
        args.push(name);
        self.git(&args)?;
        Ok(())
    }

    pub fn rebase(&self, onto: &str, interactive_autosquash: bool) -> Result<()> {
        let mut args = vec!["rebase"];
        if interactive_autosquash {
            // `GIT_SEQUENCE_EDITOR=false` would abort an interactive
            // rebase, so autosquash runs non-interactively instead.
            args.push("--autosquash");
        }
        args.push(onto);
        self.git(&args)?;
        Ok(())
    }

    pub fn cherry_pick(&self, commit: &str) -> Result<()> {
        self.git(&["cherry-pick", commit])?;
        Ok(())
    }

    pub fn revert(&self, commit: &str) -> Result<()> {
        self.git(&["revert", "--no-edit", commit])?;
        Ok(())
    }

    /// Reset HEAD to `commit`. `mode` is `soft` / `mixed` / `hard`; the
    /// destructive one is spelled out by the caller, never defaulted to.
    pub fn reset(&self, commit: &str, mode: &str) -> Result<()> {
        let flag = match mode {
            "soft" => "--soft",
            "mixed" => "--mixed",
            "hard" => "--hard",
            other => return Err(format!("unknown reset mode: {other}")),
        };
        self.git(&["reset", flag, commit])?;
        Ok(())
    }

    /// Continue or abort whatever operation is in flight.
    pub fn operation_step(&self, action: &str) -> Result<()> {
        let op = self.operation();
        let verb = match action {
            "continue" | "abort" | "skip" => action,
            other => return Err(format!("unknown operation action: {other}")),
        };
        let arg = format!("--{verb}");
        match op {
            Operation::Rebase => self.git(&["rebase", &arg])?,
            Operation::CherryPick => self.git(&["cherry-pick", &arg])?,
            Operation::Revert => self.git(&["revert", &arg])?,
            Operation::Merge => {
                // `git merge --continue` needs a message; --no-edit
                // keeps the generated one instead of opening an editor
                // that cannot open.
                if verb == "continue" {
                    self.git(&["commit", "--no-edit"])?
                } else {
                    self.git(&["merge", "--abort"])?
                }
            }
            Operation::None => return Err("no operation in progress".into()),
        };
        Ok(())
    }

    // ── remotes ─────────────────────────────────────────────────────────

    /// `mode`: `normal`, `upstream` (`-u origin <branch>`), `force`
    /// (`--force-with-lease`) or `force-hard` (bare `--force`).
    ///
    /// There is deliberately no catch-all arm: an unknown mode used to
    /// fall through to a plain push, so a typo looked like a working
    /// button that quietly did the *safe* thing — and the two forcing
    /// modes differ precisely in what they may destroy.
    pub fn push(&self, mode: &str) -> Result<()> {
        match mode {
            "normal" => self.git(&["push"])?,
            "upstream" => {
                let br = self.current_branch()?;
                self.git(&["push", "-u", "origin", &br])?
            }
            "force" => self.git(&["push", "--force-with-lease"])?,
            "force-hard" => self.git(&["push", "--force"])?,
            other => return Err(format!("unknown push mode: {other}")),
        };
        Ok(())
    }

    pub fn pull(&self, rebase: bool) -> Result<()> {
        if rebase {
            self.git(&["pull", "--rebase"])?;
        } else {
            self.git(&["pull"])?;
        }
        Ok(())
    }

    pub fn fetch(&self) -> Result<()> {
        self.git(&["fetch", "--prune"])?;
        Ok(())
    }

    // ── stash ───────────────────────────────────────────────────────────

    pub fn stash_list(&self) -> Result<Vec<StashEntry>> {
        let format = format!("--format=%gd{SEP}%gs{SEP}%at");
        let out = self.git(&["stash", "list", &format])?;
        Ok(out
            .lines()
            .filter_map(|l| {
                let f: Vec<&str> = l.split(SEP).collect();
                (f.len() == 3).then(|| StashEntry {
                    name: f[0].to_string(),
                    message: f[1].to_string(),
                    timestamp: f[2].parse().unwrap_or(0),
                })
            })
            .collect())
    }

    pub fn stash_push(&self, message: &str, include_untracked: bool) -> Result<()> {
        let mut args = vec!["stash", "push"];
        if include_untracked {
            args.push("-u");
        }
        if !message.trim().is_empty() {
            args.push("-m");
            args.push(message);
        }
        self.git(&args)?;
        Ok(())
    }

    /// `action`: `apply`, `pop` or `drop` on a `stash@{n}` name.
    pub fn stash_action(&self, name: &str, action: &str) -> Result<()> {
        match action {
            "apply" | "pop" | "drop" => self.git(&["stash", action, name])?,
            other => return Err(format!("unknown stash action: {other}")),
        };
        Ok(())
    }

    // ── history ─────────────────────────────────────────────────────────

    /// History rows, newest first. `rev` selects the starting ref
    /// (`None` = current HEAD, `Some("--all")` = every ref).
    pub fn log(
        &self,
        rev: Option<&str>,
        skip: u32,
        limit: u32,
        path: Option<&str>,
    ) -> Result<Vec<CommitRow>> {
        let format =
            format!("--format=%H{SEP}%h{SEP}%s{SEP}%an{SEP}%ae{SEP}%at{SEP}%P{SEP}%D{EOR}");
        let skip_arg = format!("--skip={skip}");
        let max_arg = format!("--max-count={limit}");
        let mut args = vec!["log", &format, &skip_arg, &max_arg];
        if let Some(r) = rev {
            args.push(r);
        }
        if let Some(p) = path {
            args.push("--");
            args.push(p);
        }
        let out = self.git(&args)?;
        let remotes = self.remotes().unwrap_or_default();
        Ok(out
            .split(EOR)
            .filter_map(|rec| parse_commit_row(rec.trim_start_matches('\n'), &remotes))
            .collect())
    }

    /// One commit: its row, full body and the files it touched.
    pub fn commit_details(&self, id: &str) -> Result<CommitDetails> {
        let rows = self.log(Some(id), 0, 1, None)?;
        let row = rows.into_iter().next().ok_or("no such commit")?;
        let body = self
            .git(&["show", "-s", "--format=%b", id])?
            .trim()
            .to_string();

        // A merge commit is compared against its first parent — one of
        // several valid readings, and the one every other client shows.
        let out = self.git(&[
            "show",
            "--name-status",
            "--format=",
            "-m",
            "--first-parent",
            id,
        ])?;
        let mut files = Vec::new();
        for line in out.lines() {
            let mut parts = line.split('\t');
            let (Some(code), Some(path)) = (parts.next(), parts.next()) else {
                continue;
            };
            let renamed_to = parts.next();
            files.push(FileStatus {
                path: renamed_to.unwrap_or(path).to_string(),
                status: match code.chars().next().unwrap_or('M') {
                    'A' => FileState::Added,
                    'D' => FileState::Deleted,
                    'R' => FileState::Renamed,
                    _ => FileState::Modified,
                },
                old_path: renamed_to.map(|_| path.to_string()),
                staged: true,
                unstaged: false,
            });
        }
        Ok(CommitDetails { row, body, files })
    }

    /// Diff of one file.
    ///
    /// `base` is `worktree` (index → working tree), `index` (HEAD →
    /// index) or a commit id (that commit against its first parent).
    pub fn diff(&self, path: &str, base: &str) -> Result<FileDiff> {
        let raw = match base {
            "worktree" => {
                // An untracked file has nothing to diff against, so show
                // it as an addition against the empty tree instead of
                // the empty diff git would print.
                if self
                    .git(&["ls-files", "--error-unmatch", "--", path])
                    .is_err()
                {
                    // `diff --no-index` exits 1 whenever the two sides
                    // differ — which is always, here — so the exit code
                    // has to be read as data rather than as failure.
                    self.exec_raw(&[
                        "diff".into(),
                        "--no-color".into(),
                        "--no-index".into(),
                        "--".into(),
                        "/dev/null".into(),
                        path.to_string(),
                    ])?
                    .stdout
                } else {
                    self.git(&["diff", "--no-color", "--", path])?
                }
            }
            "index" => self.git(&["diff", "--no-color", "--cached", "--", path])?,
            commit => self.git(&[
                "show",
                "--no-color",
                "--format=",
                "--first-parent",
                commit,
                "--",
                path,
            ])?,
        };
        Ok(parse_diff(path, &raw))
    }
}

/// Ahead/behind out of `%(upstream:track)` — `[ahead 2, behind 1]`.
fn parse_track(track: &str) -> (u32, u32) {
    let mut ahead = 0;
    let mut behind = 0;
    for part in track.trim_matches(['[', ']']).split(", ") {
        if let Some(n) = part.strip_prefix("ahead ") {
            ahead = n.parse().unwrap_or(0);
        } else if let Some(n) = part.strip_prefix("behind ") {
            behind = n.parse().unwrap_or(0);
        }
    }
    (ahead, behind)
}

/// Workspace path relative to the repo root, `""` when they coincide.
fn relative_prefix(root: &Path, workspace: &Path) -> String {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let ws = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf());
    ws.strip_prefix(&root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
}

/// Build a `FileStatus` from a porcelain-v2 `<XY>` field. `X` is the
/// index (staged) side, `Y` the worktree side; `.` means unchanged.
fn make_status(xy: &str, path: String, old_path: Option<String>, renamed: bool) -> FileStatus {
    let b = xy.as_bytes();
    let x = *b.first().unwrap_or(&b'.') as char;
    let y = *b.get(1).unwrap_or(&b'.') as char;
    let status = if renamed || x == 'R' || y == 'R' {
        FileState::Renamed
    } else if x == 'A' || y == 'A' {
        FileState::Added
    } else if x == 'D' || y == 'D' {
        FileState::Deleted
    } else {
        FileState::Modified
    };
    FileStatus {
        path,
        status,
        old_path,
        staged: x != '.',
        unstaged: y != '.',
    }
}

fn parse_commit_row(rec: &str, remotes: &[String]) -> Option<CommitRow> {
    let rec = rec.trim_matches('\n');
    if rec.is_empty() {
        return None;
    }
    let f: Vec<&str> = rec.split(SEP).collect();
    if f.len() < 8 {
        return None;
    }
    Some(CommitRow {
        id: f[0].to_string(),
        short_id: f[1].to_string(),
        summary: f[2].to_string(),
        author: f[3].to_string(),
        email: f[4].to_string(),
        timestamp: f[5].parse().unwrap_or(0),
        parents: f[6]
            .split_whitespace()
            .map(std::string::ToString::to_string)
            .collect(),
        refs: parse_refs(f[7], remotes),
    })
}

/// Parse a `%D` decoration — `HEAD -> main, origin/main, tag: v1` — into
/// typed labels. `remotes` is the only way to tell `origin/main` from a
/// local branch whose name merely contains a slash.
fn parse_refs(deco: &str, remotes: &[String]) -> Vec<RefLabel> {
    let mut out = Vec::new();
    for raw in deco.split(", ") {
        let t = raw.trim();
        if t.is_empty() {
            continue;
        }
        let (name, kind) = if let Some(tag) = t.strip_prefix("tag: ") {
            (tag.trim(), RefKind::Tag)
        } else if let Some(branch) = t.strip_prefix("HEAD -> ") {
            (branch.trim(), RefKind::Head)
        } else if t == "HEAD" {
            (t, RefKind::Head)
        } else if remotes.iter().any(|r| t.starts_with(&format!("{r}/"))) {
            (t, RefKind::Remote)
        } else {
            (t, RefKind::Local)
        };
        out.push(RefLabel {
            name: name.to_string(),
            kind,
        });
    }
    out
}

fn parse_hunk_header(h: &str) -> (u32, u32) {
    // "@@ -a,b +c,d @@ section"
    let (mut old_no, mut new_no) = (1u32, 1u32);
    for tok in h.split_whitespace() {
        if let Some(r) = tok.strip_prefix('-') {
            old_no = r.split(',').next().unwrap_or("1").parse().unwrap_or(1);
        } else if let Some(r) = tok.strip_prefix('+') {
            new_no = r.split(',').next().unwrap_or("1").parse().unwrap_or(1);
        }
    }
    (old_no, new_no)
}

/// Parse `git diff` output for a single file into hunks, keeping each
/// hunk's exact applicable patch text (file header + hunk) so hunk-level
/// staging is byte-exact rather than reconstructed.
fn parse_diff(path: &str, raw: &str) -> FileDiff {
    if raw.contains("Binary files ") || raw.contains("GIT binary patch") {
        return FileDiff {
            path: path.into(),
            binary: true,
            ..FileDiff::default()
        };
    }
    let lines: Vec<&str> = raw.split('\n').collect();
    let Some(first) = lines.iter().position(|l| l.starts_with("@@")) else {
        return FileDiff {
            path: path.into(),
            ..FileDiff::default()
        };
    };
    let header = lines[..first].join("\n");

    let mut hunks = Vec::new();
    let mut i = first;
    while i < lines.len() {
        if !lines[i].starts_with("@@") {
            i += 1;
            continue;
        }
        let start = i;
        let mut j = i + 1;
        while j < lines.len() && !lines[j].starts_with("@@") {
            j += 1;
        }
        let block = &lines[start..j];

        let (mut old_no, mut new_no) = parse_hunk_header(block[0]);
        let mut dls = Vec::new();
        for &l in &block[1..] {
            if l.is_empty() || l.starts_with('\\') {
                continue; // trailing artifact / "\ No newline at end of file"
            }
            let origin = l.as_bytes()[0] as char;
            let content = l[1..].to_string();
            match origin {
                '+' => {
                    dls.push(DiffLine {
                        origin: "+".into(),
                        content,
                        old_no: None,
                        new_no: Some(new_no),
                    });
                    new_no += 1;
                }
                '-' => {
                    dls.push(DiffLine {
                        origin: "-".into(),
                        content,
                        old_no: Some(old_no),
                        new_no: None,
                    });
                    old_no += 1;
                }
                _ => {
                    dls.push(DiffLine {
                        origin: " ".into(),
                        content,
                        old_no: Some(old_no),
                        new_no: Some(new_no),
                    });
                    old_no += 1;
                    new_no += 1;
                }
            }
        }

        let mut patch = String::with_capacity(header.len() + 64);
        patch.push_str(&header);
        patch.push('\n');
        patch.push_str(&block.join("\n"));
        if !patch.ends_with('\n') {
            patch.push('\n');
        }
        hunks.push(Hunk {
            header: block[0].to_string(),
            lines: dls,
            patch,
        });
        i = j;
    }

    FileDiff {
        path: path.into(),
        binary: false,
        hunks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .expect("git");
        assert!(
            out.status.success(),
            "git {args:?}: {:?}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// A throwaway repo with one commit on `main`.
    fn scratch_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        run(p, &["init", "-q", "-b", "main"]);
        run(p, &["config", "user.email", "t@example.com"]);
        run(p, &["config", "user.name", "Test"]);
        run(p, &["config", "commit.gpgsign", "false"]);
        std::fs::write(p.join("a.txt"), "one\n").unwrap();
        run(p, &["add", "a.txt"]);
        run(p, &["commit", "-qm", "init"]);
        dir
    }

    #[test]
    fn discover_reports_a_non_repo_plainly() {
        let dir = tempfile::tempdir().unwrap();
        let err = Repo::discover(dir.path()).unwrap_err();
        assert!(err.contains("not inside a git repository"), "{err}");
    }

    #[test]
    fn discover_missing_folder_is_not_a_git_error() {
        let err = Repo::discover(Path::new("/no/such/folder/anywhere")).unwrap_err();
        assert!(err.starts_with("no such folder"), "{err}");
    }

    #[test]
    fn status_reports_branch_and_file_states() {
        let dir = scratch_repo();
        let p = dir.path();
        std::fs::write(p.join("a.txt"), "two\n").unwrap();
        std::fs::write(p.join("b.txt"), "new\n").unwrap();

        let st = Repo::discover(p).unwrap().status(p).unwrap();
        assert_eq!(st.branch, "main");
        assert!(!st.detached);
        assert_eq!(st.operation, Operation::None);
        assert_eq!(st.workspace_prefix, "");

        let a = st.files.iter().find(|f| f.path == "a.txt").unwrap();
        assert_eq!(a.status, FileState::Modified);
        assert!(a.unstaged && !a.staged);

        let b = st.files.iter().find(|f| f.path == "b.txt").unwrap();
        assert_eq!(b.status, FileState::Untracked);
    }

    #[test]
    fn workspace_prefix_is_relative_to_the_repo_root() {
        let dir = scratch_repo();
        let nested = dir.path().join("collections");
        std::fs::create_dir(&nested).unwrap();
        let st = Repo::discover(&nested).unwrap().status(&nested).unwrap();
        assert_eq!(st.workspace_prefix, "collections");
    }

    #[test]
    fn stage_unstage_round_trip() {
        let dir = scratch_repo();
        let p = dir.path();
        std::fs::write(p.join("a.txt"), "two\n").unwrap();
        let repo = Repo::discover(p).unwrap();

        repo.stage(&["a.txt".into()]).unwrap();
        let a = |st: &RepoStatus| {
            st.files
                .iter()
                .find(|f| f.path == "a.txt")
                .cloned()
                .unwrap()
        };
        assert!(a(&repo.status(p).unwrap()).staged);

        repo.unstage(&["a.txt".into()]).unwrap();
        assert!(!a(&repo.status(p).unwrap()).staged);
    }

    #[test]
    fn stage_handles_a_deleted_file() {
        let dir = scratch_repo();
        let p = dir.path();
        std::fs::remove_file(p.join("a.txt")).unwrap();
        let repo = Repo::discover(p).unwrap();
        repo.stage(&["a.txt".into()]).unwrap();
        let st = repo.status(p).unwrap();
        let a = st.files.iter().find(|f| f.path == "a.txt").unwrap();
        assert_eq!(a.status, FileState::Deleted);
        assert!(a.staged);
    }

    #[test]
    fn discard_restores_modified_and_removes_new() {
        let dir = scratch_repo();
        let p = dir.path();
        std::fs::write(p.join("a.txt"), "changed\n").unwrap();
        std::fs::write(p.join("added.txt"), "x\n").unwrap();
        run(p, &["add", "added.txt"]);

        let repo = Repo::discover(p).unwrap();
        repo.discard(&["a.txt".into(), "added.txt".into()]).unwrap();

        assert_eq!(std::fs::read_to_string(p.join("a.txt")).unwrap(), "one\n");
        assert!(!p.join("added.txt").exists());
        assert!(repo.status(p).unwrap().files.is_empty());
    }

    #[test]
    fn commit_then_log_sees_it() {
        let dir = scratch_repo();
        let p = dir.path();
        std::fs::write(p.join("a.txt"), "two\n").unwrap();
        let repo = Repo::discover(p).unwrap();
        repo.commit(&["a.txt".into()], "second", false).unwrap();

        let rows = repo.log(None, 0, 10, None).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].summary, "second");
        assert_eq!(rows[0].author, "Test");
        assert!(rows[0].refs.iter().any(|r| r.name == "main"));
        assert_eq!(rows[1].parents.len(), 0);
        assert_eq!(rows[0].parents, vec![rows[1].id.clone()]);
    }

    #[test]
    fn commit_refuses_an_empty_message() {
        let dir = scratch_repo();
        let repo = Repo::discover(dir.path()).unwrap();
        assert!(repo.commit(&[], "   ", false).is_err());
    }

    #[test]
    fn commit_details_lists_touched_files() {
        let dir = scratch_repo();
        let p = dir.path();
        std::fs::write(p.join("b.txt"), "new\n").unwrap();
        let repo = Repo::discover(p).unwrap();
        repo.commit(&["b.txt".into()], "add b\n\nwith a body", false)
            .unwrap();

        let head = repo.log(None, 0, 1, None).unwrap().remove(0);
        let d = repo.commit_details(&head.id).unwrap();
        assert_eq!(d.body, "with a body");
        assert_eq!(d.files.len(), 1);
        assert_eq!(d.files[0].path, "b.txt");
        assert_eq!(d.files[0].status, FileState::Added);
    }

    #[test]
    fn branches_list_and_switch() {
        let dir = scratch_repo();
        let p = dir.path();
        let repo = Repo::discover(p).unwrap();
        repo.create_branch("feature", None).unwrap();

        let bs = repo.branches().unwrap();
        let feature = bs.iter().find(|b| b.name == "feature").unwrap();
        assert!(feature.is_current && !feature.is_remote);
        assert!(bs.iter().any(|b| b.name == "main" && !b.is_current));

        repo.checkout("main", false).unwrap();
        assert_eq!(repo.status(p).unwrap().branch, "main");

        repo.delete_branch("feature", true).unwrap();
        assert!(!repo.branches().unwrap().iter().any(|b| b.name == "feature"));
    }

    #[test]
    fn merge_brings_a_branch_in() {
        let dir = scratch_repo();
        let p = dir.path();
        let repo = Repo::discover(p).unwrap();
        repo.create_branch("feature", None).unwrap();
        std::fs::write(p.join("c.txt"), "c\n").unwrap();
        repo.commit(&["c.txt".into()], "add c", false).unwrap();
        repo.checkout("main", false).unwrap();
        repo.merge("feature", true).unwrap();

        assert!(p.join("c.txt").exists());
        assert_eq!(repo.status(p).unwrap().operation, Operation::None);
    }

    #[test]
    fn stash_round_trip() {
        let dir = scratch_repo();
        let p = dir.path();
        std::fs::write(p.join("a.txt"), "stashed\n").unwrap();
        let repo = Repo::discover(p).unwrap();

        repo.stash_push("wip", false).unwrap();
        assert_eq!(std::fs::read_to_string(p.join("a.txt")).unwrap(), "one\n");
        let list = repo.stash_list().unwrap();
        assert_eq!(list.len(), 1);
        assert!(list[0].message.contains("wip"));

        repo.stash_action(&list[0].name, "pop").unwrap();
        assert_eq!(
            std::fs::read_to_string(p.join("a.txt")).unwrap(),
            "stashed\n"
        );
        assert!(repo.stash_list().unwrap().is_empty());
    }

    #[test]
    fn diff_of_a_worktree_change_has_lines_and_an_appliable_patch() {
        let dir = scratch_repo();
        let p = dir.path();
        std::fs::write(p.join("a.txt"), "one\ntwo\n").unwrap();
        let repo = Repo::discover(p).unwrap();

        let d = repo.diff("a.txt", "worktree").unwrap();
        assert!(!d.binary);
        assert_eq!(d.hunks.len(), 1);
        let added: Vec<_> = d.hunks[0]
            .lines
            .iter()
            .filter(|l| l.origin == "+")
            .collect();
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].content, "two");

        // The carried patch text is what hunk-level staging applies.
        repo.apply_patch(&d.hunks[0].patch, true, false).unwrap();
        let st = repo.status(p).unwrap();
        assert!(st.files.iter().find(|f| f.path == "a.txt").unwrap().staged);
    }

    #[test]
    fn diff_of_an_untracked_file_reads_as_an_addition() {
        let dir = scratch_repo();
        let p = dir.path();
        std::fs::write(p.join("new.txt"), "hello\n").unwrap();
        let d = Repo::discover(p)
            .unwrap()
            .diff("new.txt", "worktree")
            .unwrap();
        assert!(d
            .hunks
            .iter()
            .any(|h| h.lines.iter().any(|l| l.origin == "+")));
    }

    #[test]
    fn exec_raw_returns_a_failure_as_data() {
        let dir = scratch_repo();
        let out = Repo::discover(dir.path())
            .unwrap()
            .exec_raw(&["branch".into(), "-D".into(), "nope".into()])
            .unwrap();
        assert_ne!(out.exit_code, 0);
        assert!(out.stderr.contains("nope"));
    }

    #[test]
    fn parse_track_reads_ahead_and_behind() {
        assert_eq!(parse_track("[ahead 2, behind 1]"), (2, 1));
        assert_eq!(parse_track("[ahead 3]"), (3, 0));
        assert_eq!(parse_track(""), (0, 0));
    }

    #[test]
    fn parse_refs_separates_remote_from_local() {
        let remotes = vec!["origin".to_string()];
        let refs = parse_refs("HEAD -> main, origin/main, tag: v1, origin-ish", &remotes);
        assert_eq!(
            refs[0],
            RefLabel {
                name: "main".into(),
                kind: RefKind::Head
            }
        );
        assert_eq!(refs[1].kind, RefKind::Remote);
        assert_eq!(
            refs[2],
            RefLabel {
                name: "v1".into(),
                kind: RefKind::Tag
            }
        );
        assert_eq!(refs[3].kind, RefKind::Local);
    }
}
