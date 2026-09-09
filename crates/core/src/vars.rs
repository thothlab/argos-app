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
//! ## Built-in (dynamic) helpers
//!
//! A handful of `$`-prefixed names are computed instead of looked up —
//! see [`BUILTIN_VARS`] for the full, user-facing list. They exist so a
//! field that must differ on every send (`request_id`, a nonce, a
//! cache-buster) can be written once instead of edited by hand before
//! each run.
//!
//! **Each value is generated once per [`Resolver`]**, i.e. once per
//! request. `{{$randomUuid}}` in a header and in the body therefore carry
//! the *same* id — which is the point for a correlation id — while the
//! next send produces a fresh one. When a single request genuinely needs
//! two independent values of the same kind, disambiguate them with a
//! `#label` suffix: `{{$randomUuid#outer}}` and `{{$randomUuid#inner}}`.

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
    /// Values already generated for `$`-builtins, keyed by the full
    /// placeholder name (`#label` suffix included). Keeps one request's
    /// `{{$randomUuid}}` identical everywhere it appears.
    generated: HashMap<String, String>,
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
            generated: HashMap::new(),
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
        // Scanning bytes and collecting into a byte buffer keeps multi-byte
        // UTF-8 intact: `{{`, `}}` are ASCII, so every splice point falls on
        // a character boundary and the untouched bytes are copied verbatim.
        let mut out: Vec<u8> = Vec::with_capacity(input.len());
        let bytes = input.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if i + 1 < bytes.len() && bytes[i] == b'{' && bytes[i + 1] == b'{' {
                // Find the closing `}}`.
                if let Some(end) = find_close(bytes, i + 2) {
                    let name = std::str::from_utf8(&bytes[i + 2..end]).unwrap_or("").trim();
                    if let Some(value) = self.lookup(name) {
                        out.extend_from_slice(value.as_bytes());
                    } else {
                        // Leave the placeholder verbatim and remember the miss.
                        out.extend_from_slice(b"{{");
                        out.extend_from_slice(name.as_bytes());
                        out.extend_from_slice(b"}}");
                        if !self.missing.iter().any(|m| m == name) {
                            self.missing.push(name.to_string());
                        }
                    }
                    i = end + 2;
                    continue;
                }
            }
            out.push(bytes[i]);
            i += 1;
        }
        // Safe: input was valid UTF-8 and every inserted value is a `String`.
        String::from_utf8(out)
            .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
    }

    /// Explicit env vars win over built-ins, so a workspace can pin
    /// `$timestamp` to a fixed value in a test environment.
    fn lookup(&mut self, name: &str) -> Option<String> {
        if let Some(value) = self.vars.get(name) {
            return Some(value.clone());
        }
        if let Some(cached) = self.generated.get(name) {
            return Some(cached.clone());
        }
        let value = builtin(name)?;
        self.generated.insert(name.to_string(), value.clone());
        Some(value)
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

/// One dynamic variable as shown to the user (autocomplete, docs).
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct BuiltinVar {
    /// Placeholder name including the leading `$`, without braces.
    pub name: &'static str,
    /// One-line description for the UI.
    pub description: &'static str,
    /// Example of a generated value.
    pub example: &'static str,
}

/// Every dynamic variable the resolver understands, in the order the UI
/// should list them. Aliases are listed too so a Postman-imported
/// collection's `{{$guid}}` is discoverable as-is.
pub const BUILTIN_VARS: &[BuiltinVar] = &[
    BuiltinVar {
        name: "$randomUuid",
        description: "Random UUID v4 - new on every send",
        example: "9f1c2e6a-1f3b-4d59-9c2f-7c2a6c0b0f11",
    },
    BuiltinVar {
        name: "$guid",
        description: "Alias of $randomUuid (Postman compatibility)",
        example: "9f1c2e6a-1f3b-4d59-9c2f-7c2a6c0b0f11",
    },
    BuiltinVar {
        name: "$randomUUID",
        description: "Alias of $randomUuid (Postman compatibility)",
        example: "9f1c2e6a-1f3b-4d59-9c2f-7c2a6c0b0f11",
    },
    BuiltinVar {
        name: "$randomInt",
        description: "Random 32-bit unsigned integer",
        example: "2147018233",
    },
    BuiltinVar {
        name: "$randomHex",
        description: "Random 16-character hex string - short request id",
        example: "3f8a1c92b70de451",
    },
    BuiltinVar {
        name: "$randomAlphaNumeric",
        description: "Random 16-character alphanumeric string",
        example: "k3Rt9xQ1mZ0aVb7L",
    },
    BuiltinVar {
        name: "$timestamp",
        description: "Unix epoch in milliseconds",
        example: "1757462400123",
    },
    BuiltinVar {
        name: "$timestampSeconds",
        description: "Unix epoch in seconds",
        example: "1757462400",
    },
    BuiltinVar {
        name: "$isoTimestamp",
        description: "RFC 3339 / ISO 8601 timestamp at UTC",
        example: "2026-09-10T08:00:00.123+00:00",
    },
];

/// Compute a `$`-builtin. A `#label` suffix only disambiguates cache
/// entries in [`Resolver`] and is stripped before matching.
fn builtin(name: &str) -> Option<String> {
    let base = name.split('#').next().unwrap_or(name);
    match base {
        "$timestamp" => Some(Utc::now().timestamp_millis().to_string()),
        "$timestampSeconds" => Some(Utc::now().timestamp().to_string()),
        "$isoTimestamp" => Some(Utc::now().to_rfc3339()),
        "$randomUuid" | "$guid" | "$randomUUID" => Some(Uuid::new_v4().to_string()),
        "$randomInt" => Some(rand_u32().to_string()),
        "$randomHex" => Some(random_string(16, b"0123456789abcdef")),
        "$randomAlphaNumeric" => Some(random_string(
            16,
            b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789",
        )),
        _ => None,
    }
}

/// `len` characters drawn from `alphabet` via [`rand_u32`].
fn random_string(len: usize, alphabet: &[u8]) -> String {
    (0..len)
        .map(|_| alphabet[rand_u32() as usize % alphabet.len()] as char)
        .collect()
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
    fn preserves_non_ascii_text() {
        let mut r = Resolver::new([("host", "https://пример.рф")]);
        assert_eq!(
            r.resolve("{{host}}/путь?q=тест 🚀"),
            "https://пример.рф/путь?q=тест 🚀"
        );
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
    fn builtin_is_stable_within_one_request() {
        let mut r = Resolver::new::<_, String, String>([]);
        let header = r.resolve("{{$randomUuid}}");
        let body = r.resolve("{\"request_id\":\"{{$randomUuid}}\"}");
        assert!(body.contains(&header), "same id must reach header and body");
        assert_eq!(r.resolve("{{$timestamp}}"), r.resolve("{{$timestamp}}"));
    }

    #[test]
    fn builtin_differs_between_requests() {
        let mut a = Resolver::new::<_, String, String>([]);
        let mut b = Resolver::new::<_, String, String>([]);
        assert_ne!(a.resolve("{{$randomUuid}}"), b.resolve("{{$randomUuid}}"));
    }

    #[test]
    fn label_suffix_yields_independent_values() {
        let mut r = Resolver::new::<_, String, String>([]);
        let one = r.resolve("{{$randomUuid#outer}}");
        let two = r.resolve("{{$randomUuid#inner}}");
        assert_ne!(one, two);
        assert_eq!(one, r.resolve("{{$randomUuid#outer}}"));
        assert!(r.missing().is_empty());
    }

    #[test]
    fn postman_aliases_resolve() {
        let mut r = Resolver::new::<_, String, String>([]);
        for name in ["$guid", "$randomUUID", "$randomHex", "$randomAlphaNumeric"] {
            let out = r.resolve(&format!("{{{{{name}}}}}"));
            assert!(!out.contains("{{"), "{name} left unresolved: {out}");
        }
        assert!(r.missing().is_empty());
    }

    #[test]
    fn random_helpers_have_expected_shape() {
        let mut r = Resolver::new::<_, String, String>([]);
        let hex = r.resolve("{{$randomHex}}");
        assert_eq!(hex.len(), 16);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
        let alnum = r.resolve("{{$randomAlphaNumeric}}");
        assert_eq!(alnum.len(), 16);
        assert!(alnum.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn env_var_overrides_builtin() {
        let mut r = Resolver::new([("$timestamp", "0")]);
        assert_eq!(r.resolve("{{$timestamp}}"), "0");
    }

    #[test]
    fn every_catalogued_builtin_resolves() {
        let mut r = Resolver::new::<_, String, String>([]);
        for v in BUILTIN_VARS {
            let out = r.resolve(&format!("{{{{{}}}}}", v.name));
            assert!(
                !out.is_empty() && !out.contains("{{"),
                "{} unresolved",
                v.name
            );
        }
        assert!(r.missing().is_empty());
    }

    #[test]
    fn unknown_dollar_name_stays_missing() {
        let mut r = Resolver::new::<_, String, String>([]);
        assert_eq!(r.resolve("{{$nope}}"), "{{$nope}}");
        assert_eq!(r.missing(), &["$nope"]);
    }
}
