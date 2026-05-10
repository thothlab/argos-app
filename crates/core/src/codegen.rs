//! Code generators — turn an [`crate::HttpRequest`] into a copy-pasteable
//! snippet in a target language / tool.
//!
//! Each format lives in its own submodule. Generators are pure functions over
//! `&HttpRequest` returning `String`, no I/O, easy to test.
//!
//! v0.1 covers cURL only. Browser fetch / Node fetch / Python requests / Go
//! net/http / Rust reqwest / Java OkHttp arrive in T8.1.

pub mod curl;
