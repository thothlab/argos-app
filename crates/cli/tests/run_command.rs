//! End-to-end test for `argos run` — spins up an httpmock server,
//! writes a workspace with one request that targets it, then runs the
//! CLI and asserts on the exit code + stdout.

use std::path::Path;
use std::process::Command;

use argos_core::format::request::{KeyValue, RestRequest, ScriptHooks};
use argos_core::http::HttpMethod;
use argos_core::Workspace;
use httpmock::prelude::*;

fn cli_bin() -> &'static str {
    env!("CARGO_BIN_EXE_argos")
}

fn write_workspace(root: &Path, request_url: &str, tests_script: Option<&str>) {
    Workspace::create(root, "run-test").unwrap();
    let collections_root = root.join("collections");
    std::fs::create_dir_all(&collections_root).unwrap();

    let draft = argos_core::format::RequestDraft {
        kind: argos_core::format::Kind::Request,
        name: "List".into(),
        description: None,
        variant: argos_core::format::request::RequestVariant::Rest(RestRequest {
            method: HttpMethod::Get,
            url: request_url.into(),
            query: vec![],
            headers: vec![KeyValue {
                name: "Accept".into(),
                value: "application/json".into(),
                enabled: true,
            }],
            auth: None,
            body: None,
        }),
        scripts: ScriptHooks {
            pre_request: None,
            tests: tests_script.map(str::to_string),
        },
        schema_ref: None,
    };
    draft
        .save(&collections_root.join("list.argos.yaml"))
        .unwrap();
}

#[test]
fn run_exits_zero_when_all_requests_succeed() {
    let server = MockServer::start();
    let _m = server.mock(|when, then| {
        when.method(GET).path("/users");
        then.status(200)
            .header("Content-Type", "application/json")
            .body("[]");
    });

    let dir = tempfile::tempdir().unwrap();
    write_workspace(
        dir.path(),
        &format!("{}/users", server.base_url()),
        Some("bru.test('status 200', () => { bru.expect(bru.res.status).toBe(200); });"),
    );

    let out = Command::new(cli_bin())
        .arg("run")
        .arg(dir.path().join("collections"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "expected exit 0 — stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("✓"), "missing ✓ in: {stdout}");
    assert!(
        stdout.contains("requests passed"),
        "missing summary in: {stdout}"
    );
}

#[test]
fn run_exits_nonzero_when_a_test_fails() {
    let server = MockServer::start();
    let _m = server.mock(|when, then| {
        when.method(GET).path("/users");
        then.status(500).body("boom");
    });

    let dir = tempfile::tempdir().unwrap();
    write_workspace(
        dir.path(),
        &format!("{}/users", server.base_url()),
        Some("bru.test('status 200', () => { bru.expect(bru.res.status).toBe(200); });"),
    );

    let out = Command::new(cli_bin())
        .arg("run")
        .arg(dir.path().join("collections"))
        .output()
        .unwrap();
    assert!(!out.status.success(), "expected non-zero exit");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("✗"), "missing ✗ in: {stdout}");
}

#[test]
fn run_honours_env_override_flag() {
    let server = MockServer::start();
    let _m = server.mock(|when, then| {
        when.method(GET).path("/health");
        then.status(200);
    });

    // Workspace uses `{{baseUrl}}/health`; the env file declares the
    // baseUrl, and we pass `--env prod` so the run picks it up.
    let dir = tempfile::tempdir().unwrap();
    Workspace::create(dir.path(), "envtest").unwrap();
    let collections = dir.path().join("collections");
    std::fs::create_dir_all(&collections).unwrap();
    let draft =
        argos_core::format::RequestDraft::new_rest("Ping", HttpMethod::Get, "{{baseUrl}}/health");
    draft.save(&collections.join("ping.argos.yaml")).unwrap();

    let env_dir = dir.path().join("environments");
    std::fs::create_dir_all(&env_dir).unwrap();
    let mut env = argos_core::format::Environment::new("prod");
    env.variables.push(argos_core::format::EnvVar {
        name: "baseUrl".into(),
        value: server.base_url(),
        enabled: true,
    });
    env.save(&env_dir.join("prod.env.argos.yaml")).unwrap();

    let out = Command::new(cli_bin())
        .arg("run")
        .arg(&collections)
        .arg("--env")
        .arg("prod")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "expected exit 0 — stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn run_with_unknown_env_errors_out() {
    let dir = tempfile::tempdir().unwrap();
    write_workspace(dir.path(), "https://127.0.0.1:1/x", None);

    let out = Command::new(cli_bin())
        .arg("run")
        .arg(dir.path().join("collections"))
        .arg("--env")
        .arg("nope")
        .output()
        .unwrap();
    assert!(!out.status.success(), "expected non-zero exit");
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("nope") || stderr.contains("environment"));
}
