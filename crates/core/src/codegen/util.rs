//! Shared helpers for the language generators.
//!
//! Each codegen module emits a self-contained snippet — `import` /
//! `use` lines, request build, response print. Auth headers are
//! already materialised on the `HttpRequest` by the engine, so the
//! generators don't need to handle Bearer / Basic / ApiKey separately.

use crate::http::{HttpBody, HttpHeader, HttpRequest};

/// Fold the `query` vector back into the URL. Same shape the engine
/// uses on the wire, so the snippet hits the exact URL the user sees
/// in the preview.
#[must_use]
pub(crate) fn merge_query(base: &str, query: &[(String, String)]) -> String {
    if query.is_empty() {
        return base.to_string();
    }
    if let Ok(mut u) = url::Url::parse(base) {
        {
            let mut pairs = u.query_pairs_mut();
            for (k, v) in query {
                pairs.append_pair(k, v);
            }
        }
        return u.to_string();
    }
    // Fallback for partial URLs (user is mid-typing).
    let sep = if base.contains('?') { '&' } else { '?' };
    let parts: Vec<String> = query
        .iter()
        .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
        .collect();
    format!("{base}{sep}{}", parts.join("&"))
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                use std::fmt::Write as _;
                let _ = write!(out, "%{byte:02X}");
            }
        }
    }
    out
}

/// `true` if `headers` already carries a `Content-Type` (any casing).
/// Generators consult this so they don't double up the header when
/// the user has set one explicitly.
#[must_use]
pub(crate) fn has_content_type(headers: &[HttpHeader]) -> bool {
    headers
        .iter()
        .any(|h| h.name.eq_ignore_ascii_case("content-type"))
}

/// Pick the Content-Type a body would default to, if any. Mirrors
/// what the engine sets. Returns `None` for `Raw` bodies — those
/// keep whatever the user typed.
#[must_use]
pub(crate) fn default_content_type(body: &HttpBody) -> Option<&str> {
    match body {
        HttpBody::Text { content_type, .. } | HttpBody::Raw { content_type, .. } => {
            Some(content_type.as_str())
        }
        HttpBody::Json { .. } => Some("application/json"),
        HttpBody::FormUrlEncoded { .. } => Some("application/x-www-form-urlencoded"),
    }
}

/// JS / JSON-style string literal — `"…"` with `\` escapes.
/// Suitable for fetch, Java, Rust string literals (no raw-string
/// escapes needed for hashes etc).
#[must_use]
pub(crate) fn js_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str(r#"\""#),
            '\\' => out.push_str(r"\\"),
            '\n' => out.push_str(r"\n"),
            '\r' => out.push_str(r"\r"),
            '\t' => out.push_str(r"\t"),
            c if (c as u32) < 0x20 => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Python single-quoted string literal — mirrors `js_string` but uses
/// single quotes (Python convention) and supports `\u` escapes.
#[must_use]
pub(crate) fn py_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        match c {
            '\'' => out.push_str(r"\'"),
            '\\' => out.push_str(r"\\"),
            '\n' => out.push_str(r"\n"),
            '\r' => out.push_str(r"\r"),
            '\t' => out.push_str(r"\t"),
            c if (c as u32) < 0x20 => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\x{:02x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('\'');
    out
}

/// Go string literal — double-quoted, JS-ish escapes (Go's `\xNN`
/// matches JSON for low ASCII).
#[must_use]
pub(crate) fn go_string(s: &str) -> String {
    // Go's escape rules match JSON for everything in this range.
    js_string(s)
}

/// Returns the full URL with query merged in (engine-equivalent
/// shape). Most generators want this once at the top.
#[must_use]
pub(crate) fn full_url(req: &HttpRequest) -> String {
    merge_query(&req.url, &req.query)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_string_escapes_quotes_newlines_and_low_ascii() {
        assert_eq!(js_string("hi"), r#""hi""#);
        assert_eq!(js_string(r#"a"b"#), r#""a\"b""#);
        assert_eq!(js_string("line1\nline2"), r#""line1\nline2""#);
        assert_eq!(js_string("tab\there"), r#""tab\there""#);
        //  is a control char; the helper emits a \uNNNN escape.
        assert!(js_string("").contains("\\u0001"));
    }

    #[test]
    fn py_string_uses_single_quotes_and_unicode_escape() {
        assert_eq!(py_string("hi"), "'hi'");
        assert_eq!(py_string("it's"), r"'it\'s'");
        assert_eq!(py_string("a\nb"), r"'a\nb'");
    }

    #[test]
    fn merge_query_folds_into_parsed_url() {
        let out = merge_query(
            "https://x/path",
            &[("a".into(), "1".into()), ("b".into(), "two words".into())],
        );
        assert!(out.contains("a=1"));
        assert!(out.contains("b=two+words") || out.contains("b=two%20words"));
    }

    #[test]
    fn merge_query_handles_mid_typed_url() {
        let out = merge_query("/relative", &[("q".into(), "hi".into())]);
        assert!(out.starts_with("/relative?"));
        assert!(out.contains("q=hi"));
    }

    #[test]
    fn merge_query_is_a_noop_for_empty_input() {
        assert_eq!(merge_query("https://x/y", &[]), "https://x/y");
    }
}
