//! WASM bindings for `argos-core`.
//!
//! Used by the web app (PWA) and VS Code extension. Exposes a thin JS API
//! over the same Rust core that powers the desktop and CLI.

use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

/// Returns the current `argos-core` version string.
#[wasm_bindgen]
#[must_use]
pub fn version() -> String {
    argos_core::version().to_string()
}
