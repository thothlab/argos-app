//! Generate a `fetch` snippet for browser or Node 18+ runtimes.
//!
//! Output is `await`-style — top-level `await` works in modern
//! browsers and Node REPLs. The two runtimes share 100% of the code;
//! the only difference is the leading comment + the response-print
//! line. We don't pre-import `node-fetch` — Node 18+ ships `fetch`
//! natively.

use std::fmt::Write as _;

use crate::http::{HttpBody, HttpRequest};

use super::util::{default_content_type, full_url, has_content_type, js_string};

/// JS runtime hint. Drives the leading comment so a paste lands in
/// the right console; the actual code is identical.
#[derive(Debug, Clone, Copy)]
pub enum Runtime {
    /// Browser `fetch` — devtools console or a `.html` `<script>`.
    Browser,
    /// Node.js 18+ native `fetch`.
    Node,
}

/// Render the request as a `fetch` snippet. See [`Runtime`].
#[must_use]
pub fn to_fetch(req: &HttpRequest, runtime: Runtime) -> String {
    let mut out = String::with_capacity(256);
    let banner = match runtime {
        Runtime::Browser => "// Paste into the browser devtools console.",
        Runtime::Node => "// Run with Node 18+ (uses the built-in fetch).",
    };
    let _ = writeln!(out, "{banner}");

    let url = full_url(req);
    let _ = writeln!(out, "const res = await fetch({}, {{", js_string(&url));
    let _ = writeln!(out, "  method: {},", js_string(req.method.as_str()));

    // Headers (sorted by name for deterministic output) + body.
    let mut headers: Vec<(String, String)> = req
        .headers
        .iter()
        .map(|h| (h.name.clone(), h.value.clone()))
        .collect();
    let needs_ct = req.body.is_some() && !has_content_type(&req.headers);
    if let Some(body) = &req.body {
        if needs_ct {
            if let Some(ct) = default_content_type(body) {
                headers.push(("Content-Type".into(), ct.to_string()));
            }
        }
    }
    out.push_str("  headers: {\n");
    for (k, v) in &headers {
        let _ = writeln!(out, "    {}: {},", js_string(k), js_string(v));
    }
    out.push_str("  },\n");

    if let Some(body) = &req.body {
        append_body(&mut out, body);
    }

    out.push_str("});\n");
    match runtime {
        Runtime::Browser => out.push_str("console.log(res.status, await res.text());\n"),
        Runtime::Node => out.push_str("console.log(res.status, await res.text());\n"),
    }
    out
}

fn append_body(out: &mut String, body: &HttpBody) {
    match body {
        HttpBody::Text { content, .. } => {
            let _ = writeln!(out, "  body: {},", js_string(content));
        }
        HttpBody::Json { value } => {
            // Emit as a JS object literal via JSON.stringify so the
            // request body is a string at runtime (`fetch` wants a
            // string, not an object).
            let json = serde_json::to_string(value).unwrap_or_default();
            let _ = writeln!(out, "  body: JSON.stringify({}),", json_inline(&json));
        }
        HttpBody::FormUrlEncoded { fields } => {
            out.push_str("  body: new URLSearchParams({\n");
            for (k, v) in fields {
                let _ = writeln!(out, "    {}: {},", js_string(k), js_string(v));
            }
            out.push_str("  }),\n");
        }
        HttpBody::Raw { .. } => {
            out.push_str("  // TODO: binary body — not supported in this generator.\n");
        }
    }
}

/// Inline a JSON document directly as a JS value (no string
/// wrapping) — used inside `JSON.stringify(...)`. JSON is a subset
/// of JS object syntax, so this is safe verbatim.
fn json_inline(json: &str) -> String {
    json.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{HttpBody, HttpHeader, HttpMethod, HttpRequest};
    use serde_json::json;

    fn req(method: HttpMethod, url: &str) -> HttpRequest {
        HttpRequest {
            method,
            url: url.into(),
            query: Vec::new(),
            headers: Vec::new(),
            body: None,
            timeout: None,
        }
    }

    #[test]
    fn renders_get_with_query_and_headers() {
        let mut r = req(HttpMethod::Get, "https://api.example.com/u");
        r.query.push(("q".into(), "widgets".into()));
        r.headers.push(HttpHeader::new("Accept", "application/json"));
        let s = to_fetch(&r, Runtime::Browser);
        assert!(s.contains("\"https://api.example.com/u?q=widgets\""));
        assert!(s.contains("method: \"GET\""));
        assert!(s.contains("\"Accept\": \"application/json\""));
        assert!(s.contains("// Paste into the browser"));
    }

    #[test]
    fn renders_post_json_body() {
        let mut r = req(HttpMethod::Post, "https://x/u");
        r.body = Some(HttpBody::Json {
            value: json!({"name": "Alice", "n": 3}),
        });
        let s = to_fetch(&r, Runtime::Node);
        assert!(s.contains("// Run with Node 18+"));
        assert!(s.contains("method: \"POST\""));
        assert!(s.contains("\"Content-Type\": \"application/json\""));
        // serde_json without `preserve_order` alphabetises map keys; assert
        // each key independently so the test isn't tied to insertion order.
        assert!(s.contains("body: JSON.stringify("));
        assert!(s.contains(r#""name":"Alice""#));
        assert!(s.contains(r#""n":3"#));
    }

    #[test]
    fn does_not_overwrite_user_content_type() {
        let mut r = req(HttpMethod::Post, "https://x");
        r.headers
            .push(HttpHeader::new("Content-Type", "application/vnd.api+json"));
        r.body = Some(HttpBody::Json { value: json!({}) });
        let s = to_fetch(&r, Runtime::Browser);
        // The user's content type wins; default isn't appended.
        let ct_count = s.matches("Content-Type").count();
        assert_eq!(ct_count, 1, "snippet: {s}");
        assert!(s.contains("application/vnd.api+json"));
    }

    #[test]
    fn renders_form_body_as_urlsearchparams() {
        let mut r = req(HttpMethod::Post, "https://x/login");
        r.body = Some(HttpBody::FormUrlEncoded {
            fields: vec![
                ("user".into(), "alice".into()),
                ("pass".into(), "s3cret".into()),
            ],
        });
        let s = to_fetch(&r, Runtime::Browser);
        assert!(s.contains("new URLSearchParams"));
        assert!(s.contains("\"user\": \"alice\""));
        assert!(s.contains("\"pass\": \"s3cret\""));
    }

    #[test]
    fn escapes_quotes_and_newlines_in_strings() {
        let mut r = req(HttpMethod::Get, "https://x/\"q\"");
        r.headers.push(HttpHeader::new("X-Note", "line1\nline2"));
        let s = to_fetch(&r, Runtime::Browser);
        assert!(s.contains(r#"\"q\""#));
        assert!(s.contains("\"line1\\nline2\""));
    }

    #[test]
    fn raw_body_emits_todo_comment() {
        let mut r = req(HttpMethod::Post, "https://x");
        r.body = Some(HttpBody::Raw {
            bytes: vec![1, 2, 3],
            content_type: "application/octet-stream".into(),
        });
        let s = to_fetch(&r, Runtime::Browser);
        assert!(s.contains("// TODO: binary body"));
    }
}
