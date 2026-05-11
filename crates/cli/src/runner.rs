//! Collection runner used by `argos run`.
//!
//! Walks the workspace tree (or a subtree under the target path),
//! resolves folder-level inheritance, executes pre-request and tests
//! scripts through `argos-scripting`, and prints a Mocha-like summary.

use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use argos_core::format::folder::Folder;
use argos_core::format::request::{AuthConfig, BodyDraft, KeyValue, RequestVariant, RestRequest};
use argos_core::{
    HttpBody, HttpClient, HttpHeader, HttpMethod, HttpRequest, Resolver, TreeNode, Workspace,
};
use argos_scripting::{
    run_pre_request, run_tests, ScriptBody, ScriptFormField, ScriptHeader, ScriptRequest,
    ScriptResponse, TestResult,
};

/// Options driving a single `argos run` invocation.
#[derive(Default)]
pub struct RunOptions {
    pub env_name: Option<String>,
    pub bail: bool,
    /// Per-row overrides for data-driven runs. Folded into the env map
    /// after `env_name` resolves, so iteration data wins over baseline
    /// environment values when names collide. Empty for non-iterated
    /// invocations.
    pub data_row: HashMap<String, String>,
}

/// Aggregate outcome of a run — printed to stdout and returned for
/// the CLI exit-code logic.
#[derive(Debug, Default)]
pub struct RunReport {
    pub requests: Vec<RequestOutcome>,
}

impl RunReport {
    /// Total number of executed requests (transport-OK or not).
    #[must_use]
    pub fn total(&self) -> usize {
        self.requests.len()
    }

    /// Number of requests that failed at the transport / script level
    /// or had at least one failing test.
    #[must_use]
    pub fn failed(&self) -> usize {
        self.requests.iter().filter(|r| !r.is_ok()).count()
    }
}

#[derive(Debug)]
pub struct RequestOutcome {
    #[allow(dead_code)]
    pub name: String,
    pub method: String,
    pub url: String,
    pub status: Option<u16>,
    pub duration_ms: u64,
    pub tests: Vec<TestResult>,
    /// Set when the transport or a script failed before the response.
    pub error: Option<String>,
}

impl RequestOutcome {
    /// `true` when the transport succeeded *and* every test passed.
    /// Public so reporters can apply the same rule the console prints.
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.error.is_none() && self.tests.iter().all(|t| t.passed)
    }

    #[must_use]
    pub fn failing_tests(&self) -> usize {
        self.tests.iter().filter(|t| !t.passed).count()
    }
}

/// Run `target` (an absolute path under the workspace) sequentially.
///
/// `target` may point to:
/// - the workspace root or its `collections/` subdir → run everything;
/// - a folder under `collections/` → run that subtree;
/// - a single `.argos.yaml` request file → run just that request.
///
/// # Errors
///
/// Returns an error if the workspace can't be opened, the target is
/// outside the workspace, or the env name (when supplied) doesn't
/// match a known environment.
pub async fn run(
    workspace_root: &Path,
    target: &Path,
    opts: RunOptions,
) -> anyhow::Result<RunReport> {
    let ws = Workspace::open(workspace_root)?;
    let mut env_map = resolve_env(&ws, opts.env_name.as_deref())?;
    for (k, v) in &opts.data_row {
        env_map.insert(k.clone(), v.clone());
    }
    let client = HttpClient::new()?;

    let mut report = RunReport::default();
    let nodes = locate_target(&ws.tree, target)?;
    for (node, ancestors) in nodes {
        run_node(&client, &node, &ancestors, &env_map, opts.bail, &mut report).await?;
        if opts.bail && report.requests.iter().any(|r| !r.is_ok()) {
            break;
        }
    }
    Ok(report)
}

fn resolve_env(ws: &Workspace, name: Option<&str>) -> anyhow::Result<HashMap<String, String>> {
    let Some(name) = name else {
        return Ok(HashMap::new());
    };
    let entry = ws
        .environments
        .iter()
        .find(|e| e.env.name == name)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "environment `{name}` not found in workspace (known: {})",
                ws.environments
                    .iter()
                    .map(|e| e.env.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
    let mut out = HashMap::new();
    for v in &entry.env.variables {
        if v.enabled {
            out.insert(v.name.clone(), v.value.clone());
        }
    }
    for v in &entry.env.secrets {
        if v.enabled {
            out.insert(v.name.clone(), v.value.clone());
        }
    }
    Ok(out)
}

/// Locate the subtree to run. Returns a flat sequence of
/// `(request_node, ancestors)` so the runner can apply folder
/// inheritance per request. Folders contribute themselves to the
/// `ancestors` list passed to their children.
fn locate_target(tree: &TreeNode, target: &Path) -> anyhow::Result<Vec<(TreeNode, Vec<Folder>)>> {
    let mut out = Vec::new();
    // Best-effort canonicalisation so symlinks compare consistently.
    let target_canon = std::fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
    if let Some(sub) = find_subtree(tree, &target_canon) {
        flatten_with_ancestors(sub, &mut Vec::new(), &mut out);
    } else {
        // Target is outside the tree (or workspace root). Fall back
        // to running everything.
        flatten_with_ancestors(tree, &mut Vec::new(), &mut out);
    }
    Ok(out)
}

fn find_subtree<'a>(node: &'a TreeNode, target: &Path) -> Option<&'a TreeNode> {
    let path = match node {
        TreeNode::Folder { path, .. } | TreeNode::Request { path, .. } => path,
    };
    let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if canon == target {
        return Some(node);
    }
    match node {
        TreeNode::Folder { children, .. } => {
            for c in children {
                if let Some(found) = find_subtree(c, target) {
                    return Some(found);
                }
            }
            None
        }
        TreeNode::Request { .. } => None,
    }
}

fn flatten_with_ancestors(
    node: &TreeNode,
    ancestors: &mut Vec<Folder>,
    out: &mut Vec<(TreeNode, Vec<Folder>)>,
) {
    match node {
        TreeNode::Request { .. } => out.push((node.clone(), ancestors.clone())),
        TreeNode::Folder { meta, children, .. } => {
            let pushed = if let Some(m) = meta {
                ancestors.push(m.clone());
                true
            } else {
                false
            };
            for c in children {
                flatten_with_ancestors(c, ancestors, out);
            }
            if pushed {
                ancestors.pop();
            }
        }
    }
}

async fn run_node(
    client: &HttpClient,
    node: &TreeNode,
    ancestors: &[Folder],
    env: &HashMap<String, String>,
    _bail: bool,
    report: &mut RunReport,
) -> anyhow::Result<()> {
    let TreeNode::Request { draft, .. } = node else {
        return Ok(());
    };
    let rest = match &draft.variant {
        RequestVariant::Rest(r) => r,
        // GraphQL execution lands in E5 chunk 2; WebSocket in chunk
        // 3. Skip non-REST entries silently so mixed-protocol
        // collections don't fail an `argos run`; the user-facing
        // notice goes to stderr.
        other => {
            eprintln!(
                "  ⤬ skipping {} ({} not executable yet)",
                draft.name,
                other.protocol_tag(),
            );
            return Ok(());
        }
    };

    // Merge ancestors → request. Ancestors are top→down already.
    let merged = apply_inheritance(rest, ancestors);

    let started = Instant::now();
    let outcome = execute_one(client, &draft.name, &merged, &draft.scripts(), env).await;
    let duration_ms = started.elapsed().as_millis() as u64;

    let row = match outcome {
        Ok((status, tests)) => RequestOutcome {
            name: draft.name.clone(),
            method: merged.method.as_str().to_string(),
            url: merged.url.clone(),
            status: Some(status),
            duration_ms,
            tests,
            error: None,
        },
        Err(e) => RequestOutcome {
            name: draft.name.clone(),
            method: merged.method.as_str().to_string(),
            url: merged.url.clone(),
            status: None,
            duration_ms,
            tests: Vec::new(),
            error: Some(e.to_string()),
        },
    };
    report.requests.push(row);
    Ok(())
}

async fn execute_one(
    client: &HttpClient,
    _name: &str,
    rest: &RestRequest,
    scripts: &argos_core::format::request::ScriptHooks,
    env: &HashMap<String, String>,
) -> anyhow::Result<(u16, Vec<TestResult>)> {
    let mut req = build_wire_request(rest);

    // Per-run env starts from the workspace env; scripts may mutate
    // it but their changes don't persist past this run.
    let mut run_env = env.clone();

    if let Some(script) = scripts
        .pre_request
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
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
        let outcome = run_pre_request(script, script_req, run_env.clone())?;
        if let Some(method) = parse_method(&outcome.request.method) {
            req.method = method;
        }
        req.url = outcome.request.url;
        req.headers = outcome
            .request
            .headers
            .into_iter()
            .map(|h| HttpHeader::new(h.name, h.value))
            .collect();
        if outcome.body_modified {
            req.body = outcome.request.body.map(script_body_to_http);
        }
        for (k, v) in outcome.env_updates {
            run_env.insert(k, v);
        }
        for k in outcome.env_unsets {
            run_env.remove(&k);
        }
    }

    let resolved = resolve_request(req, &run_env);
    let response = client.execute(&resolved).await?;

    let mut tests = Vec::new();
    if let Some(script) = scripts.tests.as_deref().filter(|s| !s.trim().is_empty()) {
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
        let outcome = run_tests(script, script_res, run_env)?;
        tests = outcome.tests;
    }
    Ok((response.status, tests))
}

fn apply_inheritance(rest: &RestRequest, ancestors: &[Folder]) -> RestRequest {
    let mut merged = rest.clone();

    // Headers: ancestors apply earliest-first; request-level headers
    // override by name (case-insensitive).
    let mut combined: Vec<KeyValue> = Vec::new();
    for f in ancestors {
        for h in &f.headers {
            if h.enabled {
                upsert_kv(
                    &mut combined,
                    KeyValue {
                        name: h.name.clone(),
                        value: h.value.clone(),
                        enabled: true,
                    },
                );
            }
        }
    }
    for h in &rest.headers {
        if h.enabled {
            upsert_kv(&mut combined, h.clone());
        }
    }
    merged.headers = combined;

    // Auth: if request says Inherit (or has no auth), walk ancestors
    // for the nearest non-inherit auth.
    let want_inherit = matches!(rest.auth, Some(AuthConfig::Inherit) | None);
    if want_inherit {
        for f in ancestors.iter().rev() {
            if let Some(a) = &f.auth {
                if !matches!(a, AuthConfig::Inherit) {
                    merged.auth = Some(a.clone());
                    break;
                }
            }
        }
    }

    merged
}

fn upsert_kv(list: &mut Vec<KeyValue>, kv: KeyValue) {
    if let Some(existing) = list
        .iter_mut()
        .find(|e| e.name.eq_ignore_ascii_case(&kv.name))
    {
        existing.value = kv.value;
        existing.enabled = kv.enabled;
    } else {
        list.push(kv);
    }
}

fn build_wire_request(rest: &RestRequest) -> HttpRequest {
    let headers: Vec<HttpHeader> = rest
        .headers
        .iter()
        .filter(|h| h.enabled)
        .map(|h| HttpHeader::new(h.name.clone(), h.value.clone()))
        .collect();
    let query: Vec<(String, String)> = rest
        .query
        .iter()
        .filter(|q| q.enabled)
        .map(|q| (q.name.clone(), q.value.clone()))
        .collect();
    let body = rest.body.as_ref().map(body_draft_to_wire);

    // Auth layers materialise as either a header (Bearer / Basic /
    // ApiKey-header) or a query/cookie param. We do that here so the
    // scripting sandbox sees the same shape the engine will send.
    let mut req = HttpRequest {
        method: rest.method,
        url: rest.url.clone(),
        headers,
        query,
        body,
        timeout: None,
    };
    apply_auth(&mut req, rest.auth.as_ref());
    req
}

fn body_draft_to_wire(body: &BodyDraft) -> HttpBody {
    match body {
        BodyDraft::Text {
            content,
            content_type,
        } => HttpBody::Text {
            content: content.clone(),
            content_type: content_type.clone(),
        },
        BodyDraft::Json { value } => HttpBody::Json {
            value: value.clone(),
        },
        BodyDraft::FormUrlEncoded { fields } => HttpBody::FormUrlEncoded {
            fields: fields
                .iter()
                .filter(|f| f.enabled)
                .map(|f| (f.name.clone(), f.value.clone()))
                .collect(),
        },
    }
}

fn apply_auth(req: &mut HttpRequest, auth: Option<&AuthConfig>) {
    match auth {
        None | Some(AuthConfig::Inherit) => {}
        Some(AuthConfig::Bearer { token }) => {
            upsert_header(
                &mut req.headers,
                "Authorization",
                &format!("Bearer {token}"),
            );
        }
        Some(AuthConfig::Basic { username, password }) => {
            let raw = format!("{username}:{password}");
            let encoded = base64_encode(raw.as_bytes());
            upsert_header(
                &mut req.headers,
                "Authorization",
                &format!("Basic {encoded}"),
            );
        }
        Some(AuthConfig::ApiKey {
            name,
            value,
            location,
        }) => match location {
            argos_core::format::request::ApiKeyLocation::Header => {
                upsert_header(&mut req.headers, name, value);
            }
            argos_core::format::request::ApiKeyLocation::Query => {
                req.query.push((name.clone(), value.clone()));
            }
            argos_core::format::request::ApiKeyLocation::Cookie => {
                // Append to existing Cookie header so we don't stomp.
                if let Some(existing) = req
                    .headers
                    .iter_mut()
                    .find(|h| h.name.eq_ignore_ascii_case("cookie"))
                {
                    if existing.value.is_empty() {
                        existing.value = format!("{name}={value}");
                    } else {
                        existing.value = format!("{}; {name}={value}", existing.value);
                    }
                } else {
                    req.headers
                        .push(HttpHeader::new("Cookie", format!("{name}={value}")));
                }
            }
        },
    }
}

fn upsert_header(headers: &mut Vec<HttpHeader>, name: &str, value: &str) {
    if let Some(h) = headers
        .iter_mut()
        .find(|h| h.name.eq_ignore_ascii_case(name))
    {
        h.value = value.to_string();
    } else {
        headers.push(HttpHeader::new(name, value));
    }
}

fn resolve_request(mut req: HttpRequest, env: &HashMap<String, String>) -> HttpRequest {
    let mut r = Resolver::new(env.clone());
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
            HttpBody::Text {
                content,
                content_type,
            } => {
                *content = r.resolve(content);
                *content_type = r.resolve(content_type);
            }
            HttpBody::Json { value } => resolve_json(value, &mut r),
            HttpBody::FormUrlEncoded { fields } => {
                for (k, v) in fields {
                    *k = r.resolve(k);
                    *v = r.resolve(v);
                }
            }
            HttpBody::Raw { content_type, .. } => {
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

fn http_body_to_script(body: Option<&HttpBody>) -> Option<ScriptBody> {
    match body? {
        HttpBody::Text {
            content,
            content_type,
        } => Some(ScriptBody::Text {
            content: content.clone(),
            content_type: content_type.clone(),
        }),
        HttpBody::Json { value } => Some(ScriptBody::Json {
            value: value.clone(),
        }),
        HttpBody::FormUrlEncoded { fields } => Some(ScriptBody::FormUrlEncoded {
            fields: fields
                .iter()
                .map(|(n, v)| ScriptFormField {
                    name: n.clone(),
                    value: v.clone(),
                })
                .collect(),
        }),
        HttpBody::Raw { .. } => None,
    }
}

fn script_body_to_http(body: ScriptBody) -> HttpBody {
    match body {
        ScriptBody::Text {
            content,
            content_type,
        } => HttpBody::Text {
            content,
            content_type,
        },
        ScriptBody::Json { value } => HttpBody::Json { value },
        ScriptBody::FormUrlEncoded { fields } => HttpBody::FormUrlEncoded {
            fields: fields.into_iter().map(|f| (f.name, f.value)).collect(),
        },
    }
}

fn parse_method(s: &str) -> Option<HttpMethod> {
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

fn base64_encode(input: &[u8]) -> String {
    // Tiny base64 (same vendored impl as core/codegen/curl.rs) — kept
    // local so the CLI stays free of an extra dep.
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0b11) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(((b1 & 0b1111) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(b2 & 0b11_1111) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

// We thread `RequestDraft::scripts` through `execute_one` by taking
// a reference, which requires a stable view. Add a tiny accessor so
// the runner doesn't depend on the internal field order.
trait ScriptsAccess {
    fn scripts(&self) -> argos_core::format::request::ScriptHooks;
}
impl ScriptsAccess for argos_core::format::RequestDraft {
    fn scripts(&self) -> argos_core::format::request::ScriptHooks {
        self.scripts.clone()
    }
}

/// Pretty-print a [`RunReport`] in a Mocha-like layout to stdout.
pub fn print_report(report: &RunReport) {
    for r in &report.requests {
        let badge = if r.is_ok() { "✓" } else { "✗" };
        let status_str = r
            .status
            .map(|s| format!("{s}"))
            .unwrap_or_else(|| "ERR".to_string());
        println!(
            "  {badge} {method:<6} {url}  [{status_str}, {dur}ms]",
            method = r.method,
            url = r.url,
            dur = r.duration_ms,
        );
        if let Some(err) = &r.error {
            println!("      transport: {err}");
        }
        for t in &r.tests {
            let test_badge = if t.passed { "✓" } else { "✗" };
            println!("      {test_badge} {}", t.name);
            if !t.passed {
                println!("        {}", t.message);
            }
        }
    }
    let total = report.total();
    let failed = report.failed();
    let passed = total - failed;
    let failing_tests: usize = report
        .requests
        .iter()
        .map(RequestOutcome::failing_tests)
        .sum();
    let total_tests: usize = report.requests.iter().map(|r| r.tests.len()).sum();
    println!();
    println!(
        "{passed}/{total} requests passed, {pt}/{tt} tests passed",
        pt = total_tests - failing_tests,
        tt = total_tests,
    );
    if failed > 0 {
        println!("{failed} request(s) failed");
    }
}
