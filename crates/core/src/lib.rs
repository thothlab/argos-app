//! # argos-core
//!
//! Shared core for Argos — fast, git-native API client.
//!
//! This crate contains all platform-agnostic logic:
//! - HTTP / GraphQL / WebSocket / gRPC / SSE / MQTT engines
//! - YAML file-format parsers (workspace, collection, request, environment)
//! - Sandboxed scripting runtime (QuickJS) with `bru.*` and `pm.*` shims
//! - Schema validation (OpenAPI, GraphQL)
//! - Mock server (axum)
//! - Run history / time-travel / diff
//!
//! Frontends (Tauri desktop, WASM web, VS Code extension, CLI) are thin
//! wrappers over this crate.

#![warn(missing_docs)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::doc_markdown)]

/// Crate version (matches `Cargo.toml`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Returns the current `argos-core` version string.
///
/// Used by all frontends for diagnostics and version-mismatch detection
/// between core and shell.
#[must_use]
pub fn version() -> &'static str {
    VERSION
}

/// Initialise tracing for the current process.
///
/// Frontends should call this once at startup. Idempotent — subsequent
/// calls are no-ops.
pub fn init_tracing() {
    use tracing_subscriber::EnvFilter;

    // Best-effort — if a global subscriber is already set, this is a no-op.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(true)
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_non_empty() {
        assert!(!version().is_empty());
    }
}
