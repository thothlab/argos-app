//! Smoke tests for the `argos` binary.
//!
//! We build the CLI with `cargo run -p argos-cli` against a tempdir
//! workspace populated via the `argos_core::Workspace::create` API, so
//! we don't need a fixture committed to the repo.

use std::path::Path;
use std::process::Command;

use argos_core::{format::RequestDraft, http::HttpMethod, Workspace};

/// Resolve the path to the freshly-built `argos` CLI binary. Cargo
/// exports `CARGO_BIN_EXE_argos` for integration tests in the
/// containing crate.
fn cli_bin() -> &'static str {
    env!("CARGO_BIN_EXE_argos")
}

fn make_workspace(root: &Path) {
    let ws = Workspace::create(root, "argos-cli-test").unwrap();
    let collections_root = ws.root.join("collections");
    std::fs::create_dir_all(&collections_root).unwrap();

    // One request so `list` has something to print.
    let r = RequestDraft::new_rest(
        "List users",
        HttpMethod::Get,
        "https://api.example.com/users",
    );
    r.save(&collections_root.join("list-users.argos.yaml"))
        .unwrap();
}

#[test]
fn version_subcommand_runs_without_args() {
    let out = Command::new(cli_bin()).output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.starts_with("argos "), "got: {stdout}");
}

#[test]
fn list_prints_workspace_tree() {
    let dir = tempfile::tempdir().unwrap();
    make_workspace(dir.path());

    let out = Command::new(cli_bin())
        .arg("list")
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.contains("argos-cli-test"),
        "name missing in: {stdout}"
    );
    assert!(
        stdout.contains("List users"),
        "request missing in: {stdout}"
    );
    assert!(stdout.contains("GET"), "method missing in: {stdout}");
}

#[test]
fn validate_succeeds_for_clean_workspace() {
    let dir = tempfile::tempdir().unwrap();
    make_workspace(dir.path());

    let out = Command::new(cli_bin())
        .arg("validate")
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("valid"), "no `valid` marker in: {stdout}");
}

#[test]
fn validate_fails_for_missing_workspace() {
    let dir = tempfile::tempdir().unwrap();
    // Don't create a workspace — directory exists but lacks argos.yaml.

    let out = Command::new(cli_bin())
        .arg("validate")
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(!out.status.success(), "expected non-zero exit");
}

#[test]
fn workspace_can_come_from_global_flag() {
    let dir = tempfile::tempdir().unwrap();
    make_workspace(dir.path());

    let out = Command::new(cli_bin())
        .arg("--workspace")
        .arg(dir.path())
        .arg("list")
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("argos-cli-test"));
}
