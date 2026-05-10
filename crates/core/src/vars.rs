//! `{{variable}}` substitution.
//!
//! All user-visible strings (URL, headers, query, body) can reference
//! environment variables via `{{name}}`. This module is the single place
//! that resolves them — request execution, codegen, mock-server matching
//! all go through `Resolver::resolve()`.
//!
//! Unknown placeholders are left as-is (`{{missing}}` → `{{missing}}`)
//! and reported via [`Resolver::missing`] so the UI can surface a warning
//! before send.
//!
//! ## Built-in helpers
//!
//! A handful of `$`-prefixed names are computed instead of looked up:
//! - `{{$timestamp}}` — Unix epoch milliseconds
//! - `{{$randomUuid}}` — UUIDv4
//! - `{{$randomInt}}` — random `u32` as decimal string
//! - `{{$isoTimestamp}}` — RFC 3339 / ISO 8601 timestamp at UTC

use std::collections::HashMap;

use chrono::Utc;
use uuid::Uuid;

/// Resolves `{{var}}` patterns inside arbitrary strings using a given map
/// of environment variables, plus a small set of built-in helpers.
#[derive(Debug, Default)]
pub struct Resolver {
    vars: HashMap<String, String>,
    /// Names referenced in inputs but missing from `vars` and not a built-in.
    missing: Vec<String>,
}

impl Resolver {
    /// Build a resolver from an iterable of (name, value) pairs.
    pub fn new<I, K, V>(vars: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        Self {
            vars: vars
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
            missing: Vec::new(),
        }
    }

    /// Names referenced but not found. Populated as a side effect of
    /// `resolve()`. De-duplicated.
    #[must_use]
    pub fn missing(&self) -> &[String] {
        &self.missing
    }

    /// Resolve all `{{name}}` placeholders in `input`. Returns the result;
    /// records unknown names in `self.missing`.
    ///
    /// The implementation is intentionally a single-pass scanner — no
    /// regex, no double-pass — so very large inputs (request bodies)
    /// stay cheap.
    pub fn resolve(&mut self, input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        let bytes = input.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if i + 1 < bytes.len() && bytes[i] == b'{' && bytes[i + 1] == b'{' {
                // Find the closing `}}`.
                if let Some(end) = find_close(bytes, i + 2) {
                    let name = std::str::from_utf8(&bytes[i + 2..end]).unwrap_or("").trim();
                    if let Some(value) = self.lookup(name) {
                        out.push_str(&value);
                    } else {
                        // Leave the placeholder verbatim and remember the miss.
                        out.push_str("{{");
                        out.push_str(name);
                        out.push_str("}}");
                        if !self.missing.iter().any(|m| m == name) {
                            self.missing.push(name.to_string());
                        }
                    }
                    i = end + 2;
                    continue;
                }
            }
            out.push(bytes[i] as char);
            i += 1;
        }
        out
    }

    fn lookup(&self, name: &str) -> Option<String> {
        if let Some(builtin) = builtin(name) {
            return Some(builtin);
        }
        self.vars.get(name).cloned()
    }
}

fn find_close(bytes: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    while i + 1 < bytes.len() {
        if bytes[i] == b'}' && bytes[i + 1] == b'}' {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn builtin(name: &str) -> Option<String> {
    match name {
        "$timestamp" =>
        {
            #[allow(clippy::cast_sign_loss)]
            Some(Utc::now().timestamp_millis().to_string())
        }
        "$isoTimestamp" => Some(Utc::now().to_rfc3339()),
        "$randomUuid" => Some(Uuid::new_v4().to_string()),
        "$randomInt" => Some(rand_u32().to_string()),
        _ => None,
    }
}

/// Tiny PRNG so we don't pull a `rand` dep in for one helper. Uses
/// `std::time::SystemTime` as seed and a xorshift step. Quality is fine
/// for `{{$randomInt}}` — never claimed to be cryptographic.
#[allow(clippy::cast_possible_truncation)]
fn rand_u32() -> u32 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEED: AtomicU64 = AtomicU64::new(0);
    let mut s = SEED.load(Ordering::Relaxed);
    if s == 0 {
        s = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0xDEAD_BEEF_CAFE_BABE, |d| d.as_nanos() as u64)
            | 1;
    }
    s ^= s << 13;
    s ^= s >> 7;
    s ^= s << 17;
    SEED.store(s, Ordering::Relaxed);
    (s & 0xFFFF_FFFF) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes_known_vars() {
        let mut r = Resolver::new([("base", "https://api.test"), ("token", "abc")]);
        assert_eq!(r.resolve("{{base}}/users"), "https://api.test/users");
        assert_eq!(r.resolve("Bearer {{token}}"), "Bearer abc");
        assert!(r.missing().is_empty());
    }

    #[test]
    fn leaves_unknown_vars_verbatim() {
        let mut r = Resolver::new::<_, String, String>([]);
        assert_eq!(r.resolve("{{baseUrl}}/users"), "{{baseUrl}}/users");
        assert_eq!(r.missing(), &["baseUrl"]);
    }

    #[test]
    fn deduplicates_missing_names() {
        let mut r = Resolver::new::<_, String, String>([]);
        r.resolve("{{a}} {{a}} {{a}}");
        assert_eq!(r.missing(), &["a"]);
    }

    #[test]
    fn handles_empty_and_whitespace_in_braces() {
        let mut r = Resolver::new([("name", "Alice")]);
        assert_eq!(r.resolve("{{ name }}"), "Alice");
        assert_eq!(r.resolve("{{}}"), "{{}}");
    }

    #[test]
    fn ignores_single_brace() {
        let mut r = Resolver::new::<_, String, String>([]);
        assert_eq!(r.resolve("{not a var}"), "{not a var}");
    }

    #[test]
    fn handles_unclosed_open() {
        let mut r = Resolver::new::<_, String, String>([]);
        assert_eq!(r.resolve("hello {{unclosed"), "hello {{unclosed");
    }

    #[test]
    fn builtins_resolve_without_explicit_vars() {
        let mut r = Resolver::new::<_, String, String>([]);
        let resolved = r.resolve("uuid={{$randomUuid}} ts={{$timestamp}}");
        assert!(resolved.starts_with("uuid="));
        assert!(resolved.contains(" ts="));
        // Built-ins don't show up as missing.
        assert!(r.missing().is_empty());
    }

    #[test]
    fn unknown_dollar_name_stays_missing() {
        let mut r = Resolver::new::<_, String, String>([]);
        assert_eq!(r.resolve("{{$nope}}"), "{{$nope}}");
        assert_eq!(r.missing(), &["$nope"]);
    }
}
