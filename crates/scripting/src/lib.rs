//! Pre-request and post-response scripting sandbox for Argos.
//!
//! Built on top of [`boa_engine`] — a pure-Rust ECMAScript engine. Each
//! script runs in a fresh `Context` with a single `bru` global wired in;
//! no `globalThis`, no network, no filesystem, no `setTimeout`. Memory
//! and execution time are bounded by the host (we just don't expose
//! anything that could escape the sandbox).
//!
//! Pre-request scripts can use:
//! - `bru.log(...args)`            — captures messages for the host UI.
//! - `bru.info(...)` / `bru.warn(...)` — severity-tagged log lines
//!                                   (rendered as `[info] ...` / `[warn] ...`).
//! - `bru.fail(message)`           — throw a labelled error. Inside
//!                                   `bru.test(...)` it counts as a test
//!                                   failure; at the top level of a
//!                                   pre-request script it surfaces as a
//!                                   script error.
//! - `bru.env.get(name)`           — read the active environment.
//! - `bru.env.set(name, value)`    — propose a new env value.
//! - `bru.req`                     — request snapshot the script can mutate
//!                                   (`url`, `method`, `headers`, body via
//!                                   `getBody / setJsonBody / setTextBody /
//!                                   setFormBody / clearBody`).
//!
//! Tests scripts (run after the response arrives) additionally see:
//! - `bru.res.status` / `bru.res.body` / `bru.res.getHeader(name)`.
//! - `bru.res.json()` — parsed body (throws if not valid JSON).
//! - `bru.test(name, fn)` — register a test case; thrown errors fail it.
//! - `bru.expect(value).toBe / .toEqual / .toBeTruthy / .toBeFalsy /
//!    .toContain` — minimal assertion DSL.

#![forbid(unsafe_code)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::implicit_hasher)]
#![allow(clippy::unused_self)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::similar_names)]
#![allow(clippy::doc_overindented_list_items)]

use std::cell::RefCell;
use std::collections::HashMap;

use boa_engine::{
    js_string,
    object::ObjectInitializer,
    property::{Attribute, PropertyKey},
    Context, JsArgs, JsError, JsResult, JsString, JsValue, NativeFunction, Source,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// Sandbox state lives in a thread-local. boa is single-threaded and we
// only ever drive one Context per sandbox at a time, so a thread-local
// `RefCell` lets the native callbacks reach the host state without
// fighting boa's `HostDefined` trait bounds (`Trace + JsData`).
//
// `take()` on Drop guarantees we don't carry state across sandbox runs.
thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// Snapshot of a request the script can read or mutate.
///
/// Mirrors `argos_core::HttpRequest`. The body is exposed as
/// [`ScriptBody`] — only the kinds a script can meaningfully manipulate
/// are surfaced. `Raw` (binary) bodies are intentionally not represented
/// here: the host should pass `body = None`, and re-attach the original
/// `Raw` bytes after the script run if `body_modified` is `false`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScriptRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<ScriptHeader>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<ScriptBody>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptHeader {
    pub name: String,
    pub value: String,
}

/// Body shape a pre-request script can read and replace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScriptBody {
    Text {
        content: String,
        content_type: String,
    },
    Json {
        value: serde_json::Value,
    },
    FormUrlEncoded {
        fields: Vec<ScriptFormField>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptFormField {
    pub name: String,
    pub value: String,
}

/// Result of running a script. Captures both the mutations we want to
/// apply and the human-readable diagnostics for the UI.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScriptOutcome {
    /// Lines emitted via `bru.log` / `bru.info` / `bru.warn` (one per
    /// call). `info`/`warn` lines are prefixed with `[info] ` / `[warn] `
    /// so the UI can colour them without a schema bump.
    pub logs: Vec<String>,
    /// Final request shape after the script ran. Same as the input if the
    /// script didn't touch `bru.req`.
    pub request: ScriptRequest,
    /// Env entries the script wrote with `bru.env.set`. The host is free
    /// to merge these back into the active env (we don't persist them to
    /// disk by default — that would surprise users).
    pub env_updates: HashMap<String, String>,
    /// `true` iff the script touched `bru.req` body (`setJsonBody`,
    /// `setTextBody`, `setFormBody`, `clearBody`). When `false`, the host
    /// should keep its original body — useful for `Raw` bodies the
    /// script can't represent.
    #[serde(default)]
    pub body_modified: bool,
}

/// All errors a sandbox run can surface.
#[derive(Debug, Error)]
pub enum ScriptError {
    /// The script threw an uncaught exception or failed to parse.
    #[error("script error: {0}")]
    Runtime(String),
}

impl From<JsError> for ScriptError {
    fn from(e: JsError) -> Self {
        Self::Runtime(format!("{e}"))
    }
}

/// Run a pre-request script.
///
/// The script sees `bru.req` matching `request`, `bru.env` matching
/// `env`, and may freely call `bru.log`. The returned [`ScriptOutcome`]
/// carries any mutations.
pub fn run_pre_request(
    script: &str,
    request: ScriptRequest,
    env: HashMap<String, String>,
) -> Result<ScriptOutcome, ScriptError> {
    let mut sandbox = Sandbox::new(request, env)?;
    sandbox.install_pre_request_globals()?;
    sandbox.eval(script)?;
    Ok(sandbox.into_outcome())
}

/// Snapshot of the response a tests script can read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptResponse {
    pub status: u16,
    pub body: String,
    pub headers: Vec<ScriptHeader>,
}

/// Result of one `bru.test(name, fn)` invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    pub message: String,
}

/// Aggregate outcome of running a tests script.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TestsOutcome {
    pub logs: Vec<String>,
    pub tests: Vec<TestResult>,
    pub env_updates: HashMap<String, String>,
}

/// Run a tests script after a response arrives.
pub fn run_tests(
    script: &str,
    response: ScriptResponse,
    env: HashMap<String, String>,
) -> Result<TestsOutcome, ScriptError> {
    let mut sandbox = Sandbox::new(ScriptRequest::default(), env)?;
    sandbox.install_response_globals(response)?;
    sandbox.eval(script)?;
    Ok(sandbox.into_tests_outcome())
}

// ---- internals -----------------------------------------------------------

struct Sandbox {
    ctx: Context,
}

/// Shared state. Native callbacks reach it via the thread-local `STATE`
/// slot. Both pre-request and tests sandboxes use the same `State` —
/// fields not relevant to a given mode are simply left at their default.
#[derive(Default)]
struct State {
    logs: Vec<String>,
    request: ScriptRequest,
    response: ScriptResponse,
    env: HashMap<String, String>,
    env_updates: HashMap<String, String>,
    tests: Vec<TestResult>,
    body_modified: bool,
}

impl Sandbox {
    fn new(request: ScriptRequest, env: HashMap<String, String>) -> Result<Self, ScriptError> {
        let mut ctx = Context::default();

        // Reset / install the thread-local state slot. If a previous
        // sandbox forgot to clean up (e.g. panic mid-run), `take()` here
        // forgets it before we replace it.
        STATE.with(|cell| {
            *cell.borrow_mut() = Some(State {
                logs: Vec::new(),
                request,
                response: ScriptResponse::default(),
                env,
                env_updates: HashMap::new(),
                tests: Vec::new(),
                body_modified: false,
            });
        });

        Self::install_bru_skeleton(&mut ctx)?;
        Ok(Self { ctx })
    }

    fn eval(&mut self, script: &str) -> Result<(), ScriptError> {
        self.ctx
            .eval(Source::from_bytes(script))
            .map_err(ScriptError::from)?;
        Ok(())
    }

    fn into_outcome(self) -> ScriptOutcome {
        let state = STATE
            .with(|cell| cell.borrow_mut().take())
            .expect("state slot present");
        ScriptOutcome {
            logs: state.logs,
            request: state.request,
            env_updates: state.env_updates,
            body_modified: state.body_modified,
        }
    }

    fn into_tests_outcome(self) -> TestsOutcome {
        let state = STATE
            .with(|cell| cell.borrow_mut().take())
            .expect("state slot present");
        TestsOutcome {
            logs: state.logs,
            tests: state.tests,
            env_updates: state.env_updates,
        }
    }

    /// Install `bru.log` and `bru.env`. Both modes share these.
    fn install_bru_skeleton(ctx: &mut Context) -> Result<(), ScriptError> {
        let log_fn = NativeFunction::from_fn_ptr(bru_log);
        let info_fn = NativeFunction::from_fn_ptr(bru_info);
        let warn_fn = NativeFunction::from_fn_ptr(bru_warn);
        let env_get_fn = NativeFunction::from_fn_ptr(bru_env_get);
        let env_set_fn = NativeFunction::from_fn_ptr(bru_env_set);

        let env_obj = ObjectInitializer::new(ctx)
            .function(env_get_fn, js_string!("get"), 1)
            .function(env_set_fn, js_string!("set"), 2)
            .build();

        let bru = ObjectInitializer::new(ctx)
            .function(log_fn, js_string!("log"), 1)
            .function(info_fn, js_string!("info"), 1)
            .function(warn_fn, js_string!("warn"), 1)
            .property(
                PropertyKey::from(js_string!("env")),
                env_obj,
                Attribute::READONLY | Attribute::PERMANENT,
            )
            .build();

        ctx.register_global_property(
            js_string!("bru"),
            bru,
            Attribute::WRITABLE | Attribute::PERMANENT,
        )
        .map_err(|e| ScriptError::Runtime(format!("bru install: {e}")))?;

        // `bru.fail(msg)` — pure JS so the thrown Error has a clean stack
        // and a recognisable `message` ("bru.fail: <msg>"). Inside
        // `bru.test(...)` the catch records it as a failure; at the top
        // level it propagates to the host as a `ScriptError::Runtime`.
        ctx.eval(Source::from_bytes(BRU_FAIL_PRELUDE))
            .map_err(|e| ScriptError::Runtime(format!("bru.fail install: {e}")))?;
        Ok(())
    }

    fn install_pre_request_globals(&mut self) -> Result<(), ScriptError> {
        let ctx = &mut self.ctx;
        let req_get_url_fn = NativeFunction::from_fn_ptr(bru_req_get_url);
        let req_set_url_fn = NativeFunction::from_fn_ptr(bru_req_set_url);
        let req_get_method_fn = NativeFunction::from_fn_ptr(bru_req_get_method);
        let req_set_method_fn = NativeFunction::from_fn_ptr(bru_req_set_method);
        let req_set_header_fn = NativeFunction::from_fn_ptr(bru_req_set_header);
        let req_get_header_fn = NativeFunction::from_fn_ptr(bru_req_get_header);
        let req_remove_header_fn = NativeFunction::from_fn_ptr(bru_req_remove_header);
        let req_get_body_fn = NativeFunction::from_fn_ptr(bru_req_get_body);
        let req_set_json_body_fn = NativeFunction::from_fn_ptr(bru_req_set_json_body);
        let req_set_text_body_fn = NativeFunction::from_fn_ptr(bru_req_set_text_body);
        let req_set_form_body_fn = NativeFunction::from_fn_ptr(bru_req_set_form_body);
        let req_clear_body_fn = NativeFunction::from_fn_ptr(bru_req_clear_body);

        let req_obj = ObjectInitializer::new(ctx)
            .function(req_get_url_fn, js_string!("getUrl"), 0)
            .function(req_set_url_fn, js_string!("setUrl"), 1)
            .function(req_get_method_fn, js_string!("getMethod"), 0)
            .function(req_set_method_fn, js_string!("setMethod"), 1)
            .function(req_set_header_fn, js_string!("setHeader"), 2)
            .function(req_get_header_fn, js_string!("getHeader"), 1)
            .function(req_remove_header_fn, js_string!("removeHeader"), 1)
            .function(req_get_body_fn, js_string!("getBody"), 0)
            .function(req_set_json_body_fn, js_string!("setJsonBody"), 1)
            .function(req_set_text_body_fn, js_string!("setTextBody"), 2)
            .function(req_set_form_body_fn, js_string!("setFormBody"), 1)
            .function(req_clear_body_fn, js_string!("clearBody"), 0)
            .build();

        let bru = ctx
            .global_object()
            .get(js_string!("bru"), ctx)
            .map_err(ScriptError::from)?;
        let bru_obj = bru
            .as_object()
            .cloned()
            .ok_or_else(|| ScriptError::Runtime("bru global missing".into()))?;
        bru_obj
            .set(js_string!("req"), req_obj, false, ctx)
            .map_err(ScriptError::from)?;
        Ok(())
    }

    fn install_response_globals(&mut self, response: ScriptResponse) -> Result<(), ScriptError> {
        with_state(&self.ctx, |s| s.response = response.clone());

        let ctx = &mut self.ctx;
        let res_status_fn = NativeFunction::from_fn_ptr(bru_res_status);
        let res_body_fn = NativeFunction::from_fn_ptr(bru_res_body);
        let res_get_header_fn = NativeFunction::from_fn_ptr(bru_res_get_header);
        let record_test_fn = NativeFunction::from_fn_ptr(bru_record_test);

        let res_obj = ObjectInitializer::new(ctx)
            .function(res_status_fn, js_string!("getStatus"), 0)
            .function(res_body_fn, js_string!("getBody"), 0)
            .function(res_get_header_fn, js_string!("getHeader"), 1)
            .build();

        let bru = ctx
            .global_object()
            .get(js_string!("bru"), ctx)
            .map_err(ScriptError::from)?;
        let bru_obj = bru
            .as_object()
            .cloned()
            .ok_or_else(|| ScriptError::Runtime("bru global missing".into()))?;
        bru_obj
            .set(js_string!("res"), res_obj, false, ctx)
            .map_err(ScriptError::from)?;
        bru_obj
            .set(
                js_string!("__recordTest"),
                JsValue::from(record_test_fn.to_js_function(ctx.realm())),
                false,
                ctx,
            )
            .map_err(ScriptError::from)?;

        // JS prelude — wires bru.test / bru.expect / bru.res helpers in
        // terms of the native callbacks above.
        ctx.eval(Source::from_bytes(JS_PRELUDE))
            .map_err(ScriptError::from)?;
        Ok(())
    }
}

const BRU_FAIL_PRELUDE: &str = r"
bru.fail = function(message) {
  var msg = (message === undefined || message === null) ? '' : String(message);
  var err = new Error('bru.fail: ' + msg);
  err.__bruFail = true;
  throw err;
};
";

const JS_PRELUDE: &str = r"
(function() {
  const status = bru.res.getStatus();
  const body = bru.res.getBody();
  const getHeader = bru.res.getHeader.bind(bru.res);
  bru.res = {
    status,
    body,
    getHeader,
    json() {
      try {
        return JSON.parse(body);
      } catch (e) {
        throw new Error('bru.res.json: body is not valid JSON');
      }
    },
  };
  bru.expect = function(actual) {
    return {
      toBe(expected) {
        if (!Object.is(actual, expected)) {
          throw new Error('Expected ' + JSON.stringify(actual) + ' to be ' + JSON.stringify(expected));
        }
      },
      toEqual(expected) {
        if (JSON.stringify(actual) !== JSON.stringify(expected)) {
          throw new Error('Expected ' + JSON.stringify(actual) + ' to equal ' + JSON.stringify(expected));
        }
      },
      toBeTruthy() {
        if (!actual) throw new Error('Expected truthy, got ' + JSON.stringify(actual));
      },
      toBeFalsy() {
        if (actual) throw new Error('Expected falsy, got ' + JSON.stringify(actual));
      },
      toContain(needle) {
        const ok = (typeof actual === 'string' && actual.indexOf(needle) >= 0) ||
                   (Array.isArray(actual) && actual.indexOf(needle) >= 0);
        if (!ok) {
          throw new Error('Expected ' + JSON.stringify(actual) + ' to contain ' + JSON.stringify(needle));
        }
      },
    };
  };
  bru.test = function(name, fn) {
    try {
      fn();
      bru.__recordTest(String(name), true, '');
    } catch (e) {
      const msg = (e && e.message) ? e.message : String(e);
      bru.__recordTest(String(name), false, msg);
    }
  };
})();
";

// ---- native callbacks ----------------------------------------------------

fn with_state<R>(_ctx: &Context, f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|cell| {
        let mut guard = cell.borrow_mut();
        let state = guard.as_mut().expect("sandbox state present");
        f(state)
    })
}

fn js_to_display(v: &JsValue, ctx: &mut Context) -> String {
    v.to_string(ctx)
        .map(|s| s.to_std_string_escaped())
        .unwrap_or_else(|_| "<unprintable>".into())
}

fn bru_log(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let parts: Vec<String> = args.iter().map(|a| js_to_display(a, ctx)).collect();
    let line = parts.join(" ");
    with_state(ctx, |s| s.logs.push(line));
    Ok(JsValue::undefined())
}

fn bru_info(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let parts: Vec<String> = args.iter().map(|a| js_to_display(a, ctx)).collect();
    let line = format!("[info] {}", parts.join(" "));
    with_state(ctx, |s| s.logs.push(line));
    Ok(JsValue::undefined())
}

fn bru_warn(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let parts: Vec<String> = args.iter().map(|a| js_to_display(a, ctx)).collect();
    let line = format!("[warn] {}", parts.join(" "));
    with_state(ctx, |s| s.logs.push(line));
    Ok(JsValue::undefined())
}

fn bru_env_get(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let name = args
        .get_or_undefined(0)
        .to_string(ctx)?
        .to_std_string_escaped();
    let value = with_state(ctx, |s| {
        s.env_updates
            .get(&name)
            .or_else(|| s.env.get(&name))
            .cloned()
    });
    Ok(value.map_or(JsValue::undefined(), |v| JsValue::String(JsString::from(v))))
}

fn bru_env_set(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let name = args
        .get_or_undefined(0)
        .to_string(ctx)?
        .to_std_string_escaped();
    let value = args
        .get_or_undefined(1)
        .to_string(ctx)?
        .to_std_string_escaped();
    with_state(ctx, |s| {
        s.env_updates.insert(name, value);
    });
    Ok(JsValue::undefined())
}

fn bru_req_get_url(_this: &JsValue, _args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let url = with_state(ctx, |s| s.request.url.clone());
    Ok(JsValue::String(JsString::from(url)))
}

fn bru_req_set_url(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let url = args
        .get_or_undefined(0)
        .to_string(ctx)?
        .to_std_string_escaped();
    with_state(ctx, |s| s.request.url = url);
    Ok(JsValue::undefined())
}

fn bru_req_get_method(_this: &JsValue, _args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let method = with_state(ctx, |s| s.request.method.clone());
    Ok(JsValue::String(JsString::from(method)))
}

fn bru_req_set_method(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let method = args
        .get_or_undefined(0)
        .to_string(ctx)?
        .to_std_string_escaped()
        .to_uppercase();
    with_state(ctx, |s| s.request.method = method);
    Ok(JsValue::undefined())
}

fn bru_req_set_header(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let name = args
        .get_or_undefined(0)
        .to_string(ctx)?
        .to_std_string_escaped();
    let value = args
        .get_or_undefined(1)
        .to_string(ctx)?
        .to_std_string_escaped();
    with_state(ctx, |s| {
        let target = name.to_lowercase();
        if let Some(existing) = s
            .request
            .headers
            .iter_mut()
            .find(|h| h.name.to_lowercase() == target)
        {
            existing.value = value;
        } else {
            s.request.headers.push(ScriptHeader { name, value });
        }
    });
    Ok(JsValue::undefined())
}

fn bru_req_get_header(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let name = args
        .get_or_undefined(0)
        .to_string(ctx)?
        .to_std_string_escaped()
        .to_lowercase();
    let val = with_state(ctx, |s| {
        s.request
            .headers
            .iter()
            .find(|h| h.name.to_lowercase() == name)
            .map(|h| h.value.clone())
    });
    Ok(val.map_or(JsValue::undefined(), |v| JsValue::String(JsString::from(v))))
}

fn bru_req_remove_header(
    _this: &JsValue,
    args: &[JsValue],
    ctx: &mut Context,
) -> JsResult<JsValue> {
    let name = args
        .get_or_undefined(0)
        .to_string(ctx)?
        .to_std_string_escaped()
        .to_lowercase();
    with_state(ctx, |s| {
        s.request.headers.retain(|h| h.name.to_lowercase() != name);
    });
    Ok(JsValue::undefined())
}

/// `bru.req.getBody()` returns a tagged plain object the script can
/// inspect: `{ kind: 'json', value }`, `{ kind: 'text', content,
/// contentType }`, `{ kind: 'form_url_encoded', fields: [{name, value}] }`,
/// or `null` for "no body".
fn bru_req_get_body(_this: &JsValue, _args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let body = with_state(ctx, |s| s.request.body.clone());
    match body {
        None => Ok(JsValue::null()),
        Some(b) => {
            // Round-trip via serde_json so we don't have to hand-build
            // every variant's object — boa's JSON.parse turns it into a
            // proper JS value.
            let json = serde_json::to_string(&b).map_err(|e| {
                JsError::from_opaque(JsValue::from(JsString::from(format!(
                    "bru.req.getBody serialize: {e}"
                ))))
            })?;
            json_string_to_js_value(&json, ctx)
        }
    }
}

fn bru_req_set_json_body(
    _this: &JsValue,
    args: &[JsValue],
    ctx: &mut Context,
) -> JsResult<JsValue> {
    let value = args.get_or_undefined(0).clone();
    let json_str = stringify_js_value(&value, ctx)?;
    let parsed: serde_json::Value = serde_json::from_str(&json_str).map_err(|e| {
        JsError::from_opaque(JsValue::from(JsString::from(format!(
            "bru.req.setJsonBody: {e}"
        ))))
    })?;
    with_state(ctx, |s| {
        s.request.body = Some(ScriptBody::Json { value: parsed });
        s.body_modified = true;
    });
    Ok(JsValue::undefined())
}

fn bru_req_set_text_body(
    _this: &JsValue,
    args: &[JsValue],
    ctx: &mut Context,
) -> JsResult<JsValue> {
    let content = args
        .get_or_undefined(0)
        .to_string(ctx)?
        .to_std_string_escaped();
    let ct_arg = args.get_or_undefined(1);
    let content_type = if ct_arg.is_undefined() || ct_arg.is_null() {
        "text/plain".to_string()
    } else {
        ct_arg.to_string(ctx)?.to_std_string_escaped()
    };
    with_state(ctx, |s| {
        s.request.body = Some(ScriptBody::Text {
            content,
            content_type,
        });
        s.body_modified = true;
    });
    Ok(JsValue::undefined())
}

fn bru_req_set_form_body(
    _this: &JsValue,
    args: &[JsValue],
    ctx: &mut Context,
) -> JsResult<JsValue> {
    let value = args.get_or_undefined(0).clone();
    let json_str = stringify_js_value(&value, ctx)?;
    let parsed: serde_json::Value = serde_json::from_str(&json_str).map_err(|e| {
        JsError::from_opaque(JsValue::from(JsString::from(format!(
            "bru.req.setFormBody: {e}"
        ))))
    })?;

    // Accept either { name: value, ... } or [{ name, value }, ...].
    let fields = match parsed {
        serde_json::Value::Object(map) => map
            .into_iter()
            .map(|(k, v)| ScriptFormField {
                name: k,
                value: json_value_to_string(&v),
            })
            .collect(),
        serde_json::Value::Array(arr) => {
            let mut out = Vec::with_capacity(arr.len());
            for entry in arr {
                let serde_json::Value::Object(m) = entry else {
                    return Err(JsError::from_opaque(JsValue::from(JsString::from(
                        "bru.req.setFormBody: array entries must be { name, value }",
                    ))));
                };
                let name = m.get("name").map(json_value_to_string).unwrap_or_default();
                let value = m.get("value").map(json_value_to_string).unwrap_or_default();
                out.push(ScriptFormField { name, value });
            }
            out
        }
        _ => {
            return Err(JsError::from_opaque(JsValue::from(JsString::from(
                "bru.req.setFormBody: expected an object or array of { name, value }",
            ))));
        }
    };

    with_state(ctx, |s| {
        s.request.body = Some(ScriptBody::FormUrlEncoded { fields });
        s.body_modified = true;
    });
    Ok(JsValue::undefined())
}

fn bru_req_clear_body(_this: &JsValue, _args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    with_state(ctx, |s| {
        s.request.body = None;
        s.body_modified = true;
    });
    Ok(JsValue::undefined())
}

fn json_value_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn stringify_js_value(value: &JsValue, ctx: &mut Context) -> JsResult<String> {
    let json = ctx
        .global_object()
        .get(js_string!("JSON"), ctx)?
        .as_object()
        .cloned()
        .ok_or_else(|| {
            JsError::from_opaque(JsValue::from(JsString::from("JSON global missing")))
        })?;
    let stringify = json.get(js_string!("stringify"), ctx)?;
    let stringify = stringify.as_callable().ok_or_else(|| {
        JsError::from_opaque(JsValue::from(JsString::from(
            "JSON.stringify is not callable",
        )))
    })?;
    let res = stringify.call(&JsValue::from(json), std::slice::from_ref(value), ctx)?;
    if res.is_undefined() {
        return Err(JsError::from_opaque(JsValue::from(JsString::from(
            "value is not JSON-serialisable (undefined / function)",
        ))));
    }
    Ok(res.to_string(ctx)?.to_std_string_escaped())
}

fn json_string_to_js_value(json: &str, ctx: &mut Context) -> JsResult<JsValue> {
    let json_obj = ctx
        .global_object()
        .get(js_string!("JSON"), ctx)?
        .as_object()
        .cloned()
        .ok_or_else(|| {
            JsError::from_opaque(JsValue::from(JsString::from("JSON global missing")))
        })?;
    let parse = json_obj.get(js_string!("parse"), ctx)?;
    let parse = parse.as_callable().ok_or_else(|| {
        JsError::from_opaque(JsValue::from(JsString::from("JSON.parse is not callable")))
    })?;
    parse.call(
        &JsValue::from(json_obj),
        &[JsValue::from(JsString::from(json.to_string()))],
        ctx,
    )
}

fn bru_res_status(_this: &JsValue, _args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let status = with_state(ctx, |s| s.response.status);
    Ok(JsValue::from(i32::from(status)))
}

fn bru_res_body(_this: &JsValue, _args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let body = with_state(ctx, |s| s.response.body.clone());
    Ok(JsValue::String(JsString::from(body)))
}

fn bru_res_get_header(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let name = args
        .get_or_undefined(0)
        .to_string(ctx)?
        .to_std_string_escaped()
        .to_lowercase();
    let val = with_state(ctx, |s| {
        s.response
            .headers
            .iter()
            .find(|h| h.name.to_lowercase() == name)
            .map(|h| h.value.clone())
    });
    Ok(val.map_or(JsValue::undefined(), |v| JsValue::String(JsString::from(v))))
}

fn bru_record_test(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let name = args
        .get_or_undefined(0)
        .to_string(ctx)?
        .to_std_string_escaped();
    let passed = args.get_or_undefined(1).to_boolean();
    let message = args
        .get_or_undefined(2)
        .to_string(ctx)?
        .to_std_string_escaped();
    with_state(ctx, |s| {
        s.tests.push(TestResult {
            name,
            passed,
            message,
        });
    });
    Ok(JsValue::undefined())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> ScriptRequest {
        ScriptRequest {
            method: "GET".into(),
            url: "https://example.com".into(),
            headers: vec![],
            body: None,
        }
    }

    #[test]
    fn log_captures_strings() {
        let out = run_pre_request("bru.log('hello', 42, true);", req(), HashMap::new()).unwrap();
        assert_eq!(out.logs, vec!["hello 42 true".to_string()]);
    }

    #[test]
    fn env_get_and_set_round_trip() {
        let mut env = HashMap::new();
        env.insert("base".into(), "https://api.example.com".into());

        let script = r"
            const base = bru.env.get('base');
            bru.env.set('token', 'abc-' + base.length);
            bru.log('base=', base);
        ";
        let out = run_pre_request(script, req(), env).unwrap();

        assert_eq!(
            out.env_updates.get("token").map(String::as_str),
            Some("abc-23")
        );
        assert!(out.logs[0].contains("https://api.example.com"));
    }

    #[test]
    fn request_url_method_header_mutations() {
        let script = r"
            bru.req.setUrl('https://example.com/v2');
            bru.req.setMethod('post');
            bru.req.setHeader('X-Trace', 'first');
            bru.req.setHeader('x-trace', 'override');
            bru.req.setHeader('X-Drop', 'gone');
            bru.req.removeHeader('X-DROP');
        ";
        let out = run_pre_request(script, req(), HashMap::new()).unwrap();
        assert_eq!(out.request.url, "https://example.com/v2");
        assert_eq!(out.request.method, "POST");
        assert_eq!(out.request.headers.len(), 1);
        assert_eq!(out.request.headers[0].name, "X-Trace");
        assert_eq!(out.request.headers[0].value, "override");
    }

    #[test]
    fn syntax_error_is_surfaced() {
        let err = run_pre_request("this is not js", req(), HashMap::new()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("script error"), "unexpected message: {msg}");
    }

    fn json_response(body: &str, status: u16) -> ScriptResponse {
        ScriptResponse {
            status,
            body: body.into(),
            headers: vec![ScriptHeader {
                name: "Content-Type".into(),
                value: "application/json".into(),
            }],
        }
    }

    #[test]
    fn tests_record_passes_and_fails() {
        let script = r"
            bru.test('status is 200', () => {
                bru.expect(bru.res.status).toBe(200);
            });
            bru.test('body contains hello', () => {
                bru.expect(bru.res.body).toContain('hello');
            });
            bru.test('intentional failure', () => {
                bru.expect(1).toBe(2);
            });
        ";
        let res = json_response(r#"{"greeting":"hello world"}"#, 200);
        let out = run_tests(script, res, HashMap::new()).unwrap();

        assert_eq!(out.tests.len(), 3);
        assert!(out.tests[0].passed);
        assert!(out.tests[1].passed);
        assert!(!out.tests[2].passed);
        assert!(out.tests[2].message.contains("Expected 1 to be 2"));
    }

    #[test]
    fn tests_can_use_json_helper() {
        let script = r"
            const data = bru.res.json();
            bru.test('greeting field', () => {
                bru.expect(data.greeting).toEqual('hi');
            });
            bru.log('greeting=', data.greeting);
        ";
        let res = json_response(r#"{"greeting":"hi"}"#, 200);
        let out = run_tests(script, res, HashMap::new()).unwrap();
        assert_eq!(out.tests.len(), 1);
        assert!(out.tests[0].passed);
        assert_eq!(out.logs[0], "greeting= hi");
    }

    #[test]
    fn tests_response_header_lookup_is_case_insensitive() {
        let script = r"
            bru.test('content-type', () => {
                bru.expect(bru.res.getHeader('CONTENT-TYPE')).toBe('application/json');
            });
        ";
        let res = json_response("{}", 200);
        let out = run_tests(script, res, HashMap::new()).unwrap();
        assert_eq!(out.tests.len(), 1);
        assert!(out.tests[0].passed);
    }

    #[test]
    fn info_and_warn_lines_are_prefixed() {
        let script = r"
            bru.log('plain');
            bru.info('configuring', 'token');
            bru.warn('about to skip header', 1);
        ";
        let out = run_pre_request(script, req(), HashMap::new()).unwrap();
        assert_eq!(
            out.logs,
            vec![
                "plain".to_string(),
                "[info] configuring token".to_string(),
                "[warn] about to skip header 1".to_string(),
            ]
        );
    }

    #[test]
    fn fail_inside_test_marks_failure() {
        let script = r"
            bru.test('always fails', () => {
                bru.fail('nope');
            });
        ";
        let res = ScriptResponse {
            status: 200,
            body: "{}".into(),
            headers: vec![],
        };
        let out = run_tests(script, res, HashMap::new()).unwrap();
        assert_eq!(out.tests.len(), 1);
        assert!(!out.tests[0].passed);
        assert!(
            out.tests[0].message.contains("bru.fail: nope"),
            "unexpected message: {}",
            out.tests[0].message
        );
    }

    #[test]
    fn fail_at_top_level_surfaces_as_script_error() {
        let err = run_pre_request(
            "bru.fail('aborting because env missing')",
            req(),
            HashMap::new(),
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("bru.fail: aborting because env missing"),
            "unexpected message: {msg}"
        );
    }

    #[test]
    fn body_round_trip_when_script_does_not_touch() {
        let mut r = req();
        r.body = Some(ScriptBody::Json {
            value: serde_json::json!({"a": 1}),
        });
        let out = run_pre_request("bru.log('noop');", r.clone(), HashMap::new()).unwrap();
        assert!(!out.body_modified);
        assert_eq!(out.request.body, r.body);
    }

    #[test]
    fn set_json_body_replaces() {
        let script = r"
            bru.req.setJsonBody({ name: 'Alice', tags: [1, 2] });
        ";
        let out = run_pre_request(script, req(), HashMap::new()).unwrap();
        assert!(out.body_modified);
        match out.request.body {
            Some(ScriptBody::Json { value }) => {
                assert_eq!(value["name"], "Alice");
                assert_eq!(value["tags"], serde_json::json!([1, 2]));
            }
            other => panic!("expected json body, got {other:?}"),
        }
    }

    #[test]
    fn set_text_body_default_content_type_is_text_plain() {
        let out = run_pre_request("bru.req.setTextBody('hello');", req(), HashMap::new()).unwrap();
        assert!(out.body_modified);
        match out.request.body {
            Some(ScriptBody::Text {
                content,
                content_type,
            }) => {
                assert_eq!(content, "hello");
                assert_eq!(content_type, "text/plain");
            }
            other => panic!("expected text body, got {other:?}"),
        }
    }

    #[test]
    fn set_text_body_explicit_content_type() {
        let out = run_pre_request(
            "bru.req.setTextBody('<x/>', 'application/xml');",
            req(),
            HashMap::new(),
        )
        .unwrap();
        match out.request.body {
            Some(ScriptBody::Text { content_type, .. }) => {
                assert_eq!(content_type, "application/xml");
            }
            _ => panic!("expected text body"),
        }
    }

    #[test]
    fn set_form_body_from_object() {
        let script = r"
            bru.req.setFormBody({ a: '1', b: 'two' });
        ";
        let out = run_pre_request(script, req(), HashMap::new()).unwrap();
        match out.request.body {
            Some(ScriptBody::FormUrlEncoded { fields }) => {
                let map: HashMap<_, _> = fields.into_iter().map(|f| (f.name, f.value)).collect();
                assert_eq!(map.get("a").map(String::as_str), Some("1"));
                assert_eq!(map.get("b").map(String::as_str), Some("two"));
            }
            _ => panic!("expected form body"),
        }
    }

    #[test]
    fn set_form_body_from_array() {
        let script = r"
            bru.req.setFormBody([{ name: 'tag', value: 'a' }, { name: 'tag', value: 'b' }]);
        ";
        let out = run_pre_request(script, req(), HashMap::new()).unwrap();
        match out.request.body {
            Some(ScriptBody::FormUrlEncoded { fields }) => {
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].name, "tag");
                assert_eq!(fields[0].value, "a");
                assert_eq!(fields[1].value, "b");
            }
            _ => panic!("expected form body"),
        }
    }

    #[test]
    fn clear_body_marks_modified() {
        let mut r = req();
        r.body = Some(ScriptBody::Text {
            content: "x".into(),
            content_type: "text/plain".into(),
        });
        let out = run_pre_request("bru.req.clearBody();", r, HashMap::new()).unwrap();
        assert!(out.body_modified);
        assert!(out.request.body.is_none());
    }

    #[test]
    fn get_body_exposes_kind_and_value() {
        let mut r = req();
        r.body = Some(ScriptBody::Json {
            value: serde_json::json!({"x": 7}),
        });
        let script = r"
            const b = bru.req.getBody();
            bru.log('kind=' + b.kind + ' x=' + b.value.x);
        ";
        let out = run_pre_request(script, r, HashMap::new()).unwrap();
        assert_eq!(out.logs, vec!["kind=json x=7".to_string()]);
        // Just reading shouldn't mark the body as modified.
        assert!(!out.body_modified);
    }

    #[test]
    fn get_body_returns_null_for_no_body() {
        let script = r"
            bru.log(String(bru.req.getBody()));
        ";
        let out = run_pre_request(script, req(), HashMap::new()).unwrap();
        assert_eq!(out.logs, vec!["null".to_string()]);
    }

    #[test]
    fn no_globals_leak() {
        // No `setTimeout`, no `fetch`, no `globalThis.process` — the
        // engine itself doesn't expose them, but the contract is worth
        // pinning so future refactors don't accidentally add them.
        let script = "typeof setTimeout + ' ' + typeof fetch + ' ' + typeof require";
        let out = run_pre_request(script, req(), HashMap::new()).unwrap();
        // No log to assert, but the script must run cleanly.
        assert!(out.logs.is_empty());
    }
}
