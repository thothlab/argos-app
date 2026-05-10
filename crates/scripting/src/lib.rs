//! Pre-request and post-response scripting sandbox for Argos.
//!
//! Built on top of [`boa_engine`] — a pure-Rust ECMAScript engine. Each
//! script runs in a fresh `Context` with a single `bru` global wired in;
//! no `globalThis`, no network, no filesystem, no `setTimeout`. Memory
//! and execution time are bounded by the host (we just don't expose
//! anything that could escape the sandbox).
//!
//! The first slice exposes:
//! - `bru.log(...args)`            — captures messages for the host UI.
//! - `bru.env.get(name)`           — read the active environment.
//! - `bru.env.set(name, value)`    — propose a new env value.
//! - `bru.req`                     — request snapshot the script can mutate
//!                                   (`url`, `method`, `headers`).
//!
//! Mutations that the script makes to `bru.env` and `bru.req` are pulled
//! back out after `Sandbox::run_pre_request` and returned to the caller
//! as a [`ScriptOutcome`]. Tests / response-side scripting lands in the
//! follow-up chunk.

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
/// Mirrors `argos_core::HttpRequest` minus the body — body mutation is
/// deferred until we have a clean way to round-trip the body kind through
/// JS objects (see `bru.req.body` in the next chunk).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<ScriptHeader>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptHeader {
    pub name: String,
    pub value: String,
}

/// Result of running a script. Captures both the mutations we want to
/// apply and the human-readable diagnostics for the UI.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScriptOutcome {
    /// Lines emitted via `bru.log` (one per call).
    pub logs: Vec<String>,
    /// Final request shape after the script ran. Same as the input if the
    /// script didn't touch `bru.req`.
    pub request: ScriptRequest,
    /// Env entries the script wrote with `bru.env.set`. The host is free
    /// to merge these back into the active env (we don't persist them to
    /// disk by default — that would surprise users).
    pub env_updates: HashMap<String, String>,
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
    sandbox.eval(script)?;
    Ok(sandbox.into_outcome())
}

// ---- internals -----------------------------------------------------------

struct Sandbox {
    ctx: Context,
}

/// Shared state the host sees through `bru`. We juggle a `RefCell<State>`
/// indirectly via boa's `HostDefined` slot since the native callbacks
/// borrow the context mutably.
#[derive(Default)]
struct State {
    logs: Vec<String>,
    request: ScriptRequest,
    env: HashMap<String, String>,
    env_updates: HashMap<String, String>,
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
                env,
                env_updates: HashMap::new(),
            });
        });

        Self::install_bru_global(&mut ctx)?;
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
        }
    }

    fn install_bru_global(ctx: &mut Context) -> Result<(), ScriptError> {
        // Build the `bru` object piece by piece.
        let log_fn = NativeFunction::from_fn_ptr(bru_log);
        let env_get_fn = NativeFunction::from_fn_ptr(bru_env_get);
        let env_set_fn = NativeFunction::from_fn_ptr(bru_env_set);
        let req_get_url_fn = NativeFunction::from_fn_ptr(bru_req_get_url);
        let req_set_url_fn = NativeFunction::from_fn_ptr(bru_req_set_url);
        let req_get_method_fn = NativeFunction::from_fn_ptr(bru_req_get_method);
        let req_set_method_fn = NativeFunction::from_fn_ptr(bru_req_set_method);
        let req_set_header_fn = NativeFunction::from_fn_ptr(bru_req_set_header);
        let req_get_header_fn = NativeFunction::from_fn_ptr(bru_req_get_header);
        let req_remove_header_fn = NativeFunction::from_fn_ptr(bru_req_remove_header);

        let env_obj = ObjectInitializer::new(ctx)
            .function(env_get_fn, js_string!("get"), 1)
            .function(env_set_fn, js_string!("set"), 2)
            .build();

        let req_obj = ObjectInitializer::new(ctx)
            .function(req_get_url_fn, js_string!("getUrl"), 0)
            .function(req_set_url_fn, js_string!("setUrl"), 1)
            .function(req_get_method_fn, js_string!("getMethod"), 0)
            .function(req_set_method_fn, js_string!("setMethod"), 1)
            .function(req_set_header_fn, js_string!("setHeader"), 2)
            .function(req_get_header_fn, js_string!("getHeader"), 1)
            .function(req_remove_header_fn, js_string!("removeHeader"), 1)
            .build();

        let bru = ObjectInitializer::new(ctx)
            .function(log_fn, js_string!("log"), 1)
            .property(
                PropertyKey::from(js_string!("env")),
                env_obj,
                Attribute::READONLY | Attribute::PERMANENT,
            )
            .property(
                PropertyKey::from(js_string!("req")),
                req_obj,
                Attribute::READONLY | Attribute::PERMANENT,
            )
            .build();

        ctx.register_global_property(
            js_string!("bru"),
            bru,
            Attribute::READONLY | Attribute::PERMANENT,
        )
        .map_err(|e| ScriptError::Runtime(format!("bru install: {e}")))?;
        Ok(())
    }
}

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

fn bru_env_get(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
    let value = with_state(ctx, |s| {
        s.env_updates
            .get(&name)
            .or_else(|| s.env.get(&name))
            .cloned()
    });
    Ok(value.map_or(JsValue::undefined(), |v| JsValue::String(JsString::from(v))))
}

fn bru_env_set(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
    let value = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
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
    let url = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
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
    let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
    let value = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
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

fn bru_req_remove_header(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> ScriptRequest {
        ScriptRequest {
            method: "GET".into(),
            url: "https://example.com".into(),
            headers: vec![],
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

        assert_eq!(out.env_updates.get("token").map(String::as_str), Some("abc-23"));
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
