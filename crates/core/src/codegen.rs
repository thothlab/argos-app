//! Code generators — turn an [`crate::HttpRequest`] into a copy-pasteable
//! snippet in a target language / tool.
//!
//! Each format lives in its own submodule. Generators are pure functions over
//! `&HttpRequest` returning `String`, no I/O, easy to test.
//!
//! Supported targets:
//!   - `curl` (always available — `T1.6`)
//!   - `fetch` for the browser console and Node.js 18+ (`T8.1.1` / `T8.1.2`)
//!   - Python `requests` (`T8.1.3`)
//!   - Go `net/http` (`T8.1.4`)
//!   - Rust `reqwest::blocking` (`T8.1.5`)
//!
//! Java OkHttp (`T8.1.6`) is deferred — Argos's ICP is web / Go / Python.

pub mod curl;
pub mod fetch_js;
pub mod go;
pub mod python;
pub mod rust;
mod util;
