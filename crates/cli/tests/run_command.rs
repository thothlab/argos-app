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

#[test]
fn run_with_iteration_data_csv_runs_each_row() {
    // Mock /search returns 200 only when `q` is one of {alpha, beta},
    // 500 otherwise. The CSV drives three iterations, all of which
    // should succeed.
    let server = MockServer::start();
    let _ok = server.mock(|when, then| {
        when.method(GET).path("/search").query_param("q", "alpha");
        then.status(200).body("ok");
    });
    let _ok2 = server.mock(|when, then| {
        when.method(GET).path("/search").query_param("q", "beta");
        then.status(200).body("ok");
    });
    let _ok3 = server.mock(|when, then| {
        when.method(GET).path("/search").query_param("q", "gamma");
        then.status(200).body("ok");
    });

    let dir = tempfile::tempdir().unwrap();
    Workspace::create(dir.path(), "iter").unwrap();
    let collections = dir.path().join("collections");
    std::fs::create_dir_all(&collections).unwrap();
    let mut req = argos_core::format::RequestDraft::new_rest(
        "Search",
        HttpMethod::Get,
        format!("{}/search", server.base_url()),
    );
    {
        let argos_core::format::request::RequestVariant::Rest(rest) = &mut req.variant;
        rest.query.push(KeyValue {
            name: "q".into(),
            value: "{{q}}".into(),
            enabled: true,
        });
    }
    req.scripts.tests =
        Some("bru.test('status 200', () => { bru.expect(bru.res.status).toBe(200); });".into());
    req.save(&collections.join("search.argos.yaml")).unwrap();

    let data_path = dir.path().join("data.csv");
    std::fs::write(&data_path, "q\nalpha\nbeta\ngamma\n").unwrap();

    let out = Command::new(cli_bin())
        .arg("run")
        .arg(&collections)
        .arg("--iteration-data")
        .arg(&data_path)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "expected exit 0 — stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.contains("Iteration 1 of 3") && stdout.contains("Iteration 3 of 3"),
        "missing iteration banners in: {stdout}"
    );
}

#[test]
fn reporter_writes_json_and_junit_files() {
    let server = MockServer::start();
    let _m = server.mock(|when, then| {
        when.method(GET).path("/users");
        then.status(200).body("[]");
    });

    let dir = tempfile::tempdir().unwrap();
    write_workspace(
        dir.path(),
        &format!("{}/users", server.base_url()),
        Some("bru.test('status 200', () => { bru.expect(bru.res.status).toBe(200); });"),
    );

    let json_out = dir.path().join("report.json");
    let junit_out = dir.path().join("report.xml");
    let html_out = dir.path().join("report.html");

    let out = Command::new(cli_bin())
        .arg("run")
        .arg(dir.path().join("collections"))
        .arg("--reporter")
        .arg(format!("json={}", json_out.display()))
        .arg("--reporter")
        .arg(format!("junit={}", junit_out.display()))
        .arg("--reporter")
        .arg(format!("html={}", html_out.display()))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "expected exit 0 — stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let j = std::fs::read_to_string(&json_out).unwrap();
    let v: serde_json::Value = serde_json::from_str(&j).unwrap();
    assert_eq!(v["schema"], "argos.run.v1");
    assert_eq!(v["summary"]["requests_total"], 1);
    assert_eq!(v["iterations"][0]["requests"][0]["ok"], true);

    let xml = std::fs::read_to_string(&junit_out).unwrap();
    assert!(xml.starts_with("<?xml"));
    assert!(xml.contains("<testsuites"));
    assert!(xml.contains("<testcase"));

    let html = std::fs::read_to_string(&html_out).unwrap();
    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains("/users"));
}

#[test]
fn reporter_to_stdout_emits_payload() {
    let server = MockServer::start();
    let _m = server.mock(|when, then| {
        when.method(GET).path("/u");
        then.status(200);
    });
    let dir = tempfile::tempdir().unwrap();
    write_workspace(dir.path(), &format!("{}/u", server.base_url()), None);
    let out = Command::new(cli_bin())
        .arg("run")
        .arg(dir.path().join("collections"))
        .arg("--reporter")
        .arg("json")
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    // Console summary still printed; JSON appended.
    assert!(stdout.contains("requests passed"));
    let json_start = stdout.find("{\n").expect("json appended");
    let json_body = &stdout[json_start..];
    let v: serde_json::Value = serde_json::from_str(json_body).unwrap();
    assert_eq!(v["schema"], "argos.run.v1");
}

#[test]
fn reporter_unknown_format_errors_out() {
    let dir = tempfile::tempdir().unwrap();
    write_workspace(dir.path(), "https://127.0.0.1:1", None);
    let out = Command::new(cli_bin())
        .arg("run")
        .arg(dir.path().join("collections"))
        .arg("--reporter")
        .arg("yaml")
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("yaml") || stderr.contains("unknown reporter"));
}

#[test]
fn run_with_iteration_data_json_is_supported() {
    let server = MockServer::start();
    let _ok = server.mock(|when, then| {
        when.method(GET).path("/x").query_param("k", "1");
        then.status(200);
    });
    let _ok2 = server.mock(|when, then| {
        when.method(GET).path("/x").query_param("k", "2");
        then.status(200);
    });

    let dir = tempfile::tempdir().unwrap();
    Workspace::create(dir.path(), "iter-json").unwrap();
    let collections = dir.path().join("collections");
    std::fs::create_dir_all(&collections).unwrap();
    let mut req = argos_core::format::RequestDraft::new_rest(
        "X",
        HttpMethod::Get,
        format!("{}/x", server.base_url()),
    );
    {
        let argos_core::format::request::RequestVariant::Rest(rest) = &mut req.variant;
        rest.query.push(KeyValue {
            name: "k".into(),
            value: "{{k}}".into(),
            enabled: true,
        });
    }
    req.save(&collections.join("x.argos.yaml")).unwrap();

    let data_path = dir.path().join("data.json");
    std::fs::write(&data_path, r#"[{"k":"1"},{"k":2}]"#).unwrap();

    let out = Command::new(cli_bin())
        .arg("run")
        .arg(&collections)
        .arg("--iteration-data")
        .arg(&data_path)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "expected exit 0 — stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
