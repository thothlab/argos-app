//! Server-side PII redaction for incoming crash reports.
//!
//! The client's opt-in modal promises: "no URLs, no headers, no body
//! content". The reality is that panic messages, JS error strings, and
//! backtraces can quietly carry any of those — a `panic!()` formatted
//! with a request URL, an `Error` whose `.message` includes the failing
//! response body, a stack frame from a file under `/Users/<name>/`.
//!
//! This module scrubs the most common leaks before the report ever
//! touches disk. It is intentionally conservative — we only mask
//! patterns that are clearly secrets / personal — because over-eager
//! redaction makes reports useless.
//!
//! Rules applied in `redact`, in order:
//!
//! 1. `/Users/<name>/…` → `/Users/<user>/…` (macOS home).
//! 2. `/home/<name>/…` → `/home/<user>/…` (Linux home).
//! 3. `C:\Users\<name>\…` → `C:\Users\<user>\…` (Windows home).
//! 4. `Bearer <token>` → `Bearer <redacted>` (auth headers in text).
//! 5. `Basic <b64>` → `Basic <redacted>`.
//! 6. `api_key=… / token=… / password=… / secret=… / signature=…`
//!    → `<name>=<redacted>` (URL query / form pairs).
//! 7. JWTs (`eyJ…` 3-segment base64url) → `<jwt-redacted>`.

use once_cell::sync::Lazy;
use regex::Regex;

static USER_PATH_MACOS: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"/Users/[^/\s"']+"#).unwrap());
static USER_PATH_LINUX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"/home/[^/\s"']+"#).unwrap());
static USER_PATH_WINDOWS: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)C:\\Users\\[^\\\s"']+"#).unwrap());
static BEARER_TOKEN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\bBearer\s+[A-Za-z0-9._\-+/=]+").unwrap());
static BASIC_AUTH: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\bBasic\s+[A-Za-z0-9+/=]+").unwrap());
static SECRET_PARAM: Lazy<Regex> = Lazy::new(|| {
    // matches name=value in URL query strings, form bodies, or
    // standalone text. Stops at common terminators.
    Regex::new(
        r#"(?i)\b(api[_-]?key|apikey|access[_-]?token|refresh[_-]?token|token|password|passwd|secret|signature|sig)\s*[:=]\s*[^\s"'&<>,;]+"#,
    )
    .unwrap()
});
static JWT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\beyJ[A-Za-z0-9_-]{4,}\.[A-Za-z0-9_-]{4,}\.[A-Za-z0-9_-]+").unwrap());

/// Apply every redaction rule to `input` and return the scrubbed copy.
/// Idempotent — running twice on the same string is a no-op for any
/// content that's already been sanitised.
#[must_use]
pub fn redact(input: &str) -> String {
    // Order matters only for JWT vs SECRET_PARAM (a JWT shouldn't
    // appear as a query value in practice, but if it did the
    // SECRET_PARAM rule would catch it first anyway). Paths first
    // because the other rules don't touch them.
    let s = USER_PATH_MACOS.replace_all(input, "/Users/<user>");
    let s = USER_PATH_LINUX.replace_all(&s, "/home/<user>");
    let s = USER_PATH_WINDOWS.replace_all(&s, r"C:\Users\<user>");
    let s = BEARER_TOKEN.replace_all(&s, "Bearer <redacted>");
    let s = BASIC_AUTH.replace_all(&s, "Basic <redacted>");
    let s = SECRET_PARAM.replace_all(&s, "$1=<redacted>");
    let s = JWT.replace_all(&s, "<jwt-redacted>");
    s.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_home_path_is_anonymised() {
        let r = redact("at /Users/shaukat/Documents/foo/bar.rs:42");
        assert!(r.contains("/Users/<user>/Documents/foo/bar.rs:42"));
        assert!(!r.contains("shaukat"));
    }

    #[test]
    fn linux_home_path_is_anonymised() {
        let r = redact("at /home/alice/.config/argos/x.yaml");
        assert!(r.contains("/home/<user>/.config/argos/x.yaml"));
        assert!(!r.contains("alice"));
    }

    #[test]
    fn windows_home_path_is_anonymised() {
        let r = redact(r"at C:\Users\Alice\AppData\Roaming\argos");
        assert!(r.contains(r"C:\Users\<user>\AppData\Roaming\argos"));
        assert!(!r.contains("Alice"));
    }

    #[test]
    fn bearer_token_is_redacted() {
        let r = redact("header Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.foo.bar123");
        assert!(r.contains("Bearer <redacted>"));
        assert!(!r.contains("eyJhbGciOiJIUzI1NiJ9.foo.bar123"));
    }

    #[test]
    fn basic_auth_is_redacted() {
        let r = redact("auth: Basic dXNlcjpwYXNz");
        assert!(r.contains("Basic <redacted>"));
        assert!(!r.contains("dXNlcjpwYXNz"));
    }

    #[test]
    fn query_string_api_key_is_redacted() {
        let r = redact("GET https://api.example.com/v1/users?api_key=abc123XYZ&page=1");
        assert!(r.contains("api_key=<redacted>"));
        assert!(!r.contains("abc123XYZ"));
        // Non-sensitive params survive.
        assert!(r.contains("page=1"));
    }

    #[test]
    fn variant_secret_names_are_redacted() {
        for name in ["password", "secret", "access_token", "refresh-token", "ApiKey"] {
            let payload = format!("payload {name}=hunter2foobarbaz");
            let r = redact(&payload);
            assert!(r.contains("hunter2foobarbaz") == false, "{name} not redacted: {r}");
        }
    }

    #[test]
    fn jwt_is_redacted() {
        let token = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.signature-bytes-here";
        let r = redact(&format!("token was {token}"));
        assert!(r.contains("<jwt-redacted>"));
        assert!(!r.contains(token));
    }

    #[test]
    fn multiple_pii_items_all_redacted() {
        let msg = "panic at /Users/shaukat/x.rs with Authorization: Bearer XYZ and api_key=ABC";
        let r = redact(msg);
        assert!(!r.contains("shaukat"));
        assert!(!r.contains("Bearer XYZ"));
        assert!(!r.contains("ABC"));
    }

    #[test]
    fn idempotent_on_already_clean_text() {
        let clean = "thread panicked at 'assertion failed: x > 0' at crates/core/src/foo.rs:42";
        assert_eq!(redact(clean), clean);
    }

    #[test]
    fn redact_is_idempotent() {
        let once = redact("at /Users/shaukat/x.rs with Bearer ABC");
        let twice = redact(&once);
        assert_eq!(once, twice);
    }
}
