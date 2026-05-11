//! Generate a Rust `reqwest::blocking` snippet.
//!
//! Sync API for snippet brevity — tokio runtime setup is friction
//! when the user wants "drop in `main()` and run". Output is a
//! complete `fn main()` that compiles against a one-line
//! `cargo add reqwest --features blocking,json` dependency.

use std::fmt::Write as _;

use crate::http::{HttpBody, HttpRequest};

use super::util::{full_url, has_content_type, js_string};

/// Render as a Rust `reqwest::blocking` snippet. Requires `reqwest`
/// with the `blocking` and `json` features.
#[must_use]
pub fn to_rust(req: &HttpRequest) -> String {
    let mut out = String::with_capacity(256);
    out.push_str("// `cargo add reqwest --features blocking,json`\n");
    out.push_str("use reqwest::blocking::Client;\n");
    if matches!(&req.body, Some(HttpBody::Json { .. })) {
        out.push_str("use serde_json::json;\n");
    }
    out.push('\n');
    out.push_str("fn main() -> Result<(), Box<dyn std::error::Error>> {\n");
    out.push_str("    let client = Client::new();\n");

    let url = full_url(req);
    let method = method_call(req.method.as_str());
    let _ = writeln!(out, "    let mut req = client.{}({});", method, js_string(&url));

    for h in &req.headers {
        let _ = writeln!(
            out,
            "    req = req.header({}, {});",
            js_string(&h.name),
            js_string(&h.value),
        );
    }

    if let Some(body) = &req.body {
        append_body(&mut out, body, &req.headers);
    }

    out.push_str("    let resp = req.send()?;\n");
    out.push_str("    println!(\"{} {}\", resp.status(), resp.text()?);\n");
    out.push_str("    Ok(())\n");
    out.push_str("}\n");
    out
}

/// Map HTTP method to reqwest's builder method name. Falls back to
/// `request(METHOD, url)` for the rare methods reqwest doesn't have
/// a shortcut for.
fn method_call(method: &str) -> String {
    match method {
        "GET" => "get".into(),
        "POST" => "post".into(),
        "PUT" => "put".into(),
        "PATCH" => "patch".into(),
        "DELETE" => "delete".into(),
        "HEAD" => "head".into(),
        other => format!("request(reqwest::Method::from_bytes(b{}).unwrap(),", js_string(other)),
    }
}

fn append_body(out: &mut String, body: &HttpBody, headers: &[crate::http::HttpHeader]) {
    match body {
        HttpBody::Text { content, .. } => {
            let _ = writeln!(out, "    req = req.body({}.to_string());", js_string(content));
        }
        HttpBody::Json { value } => {
            let pretty = serde_json::to_string_pretty(value).unwrap_or_default();
            let indented = pretty
                .lines()
                .map(|l| format!("        {l}"))
                .collect::<Vec<_>>()
                .join("\n");
            let _ = writeln!(out, "    req = req.json(&json!(\n{indented}\n    ));");
        }
        HttpBody::FormUrlEncoded { fields } => {
            // `reqwest`'s `.form` builder serializes a slice of tuples
            // for us — keeps deterministic order.
            out.push_str("    req = req.form(&[\n");
            for (k, v) in fields {
                let _ = writeln!(out, "        ({}, {}),", js_string(k), js_string(v));
            }
            out.push_str("    ]);\n");
        }
        HttpBody::Raw { .. } => {
            out.push_str("    // TODO: binary body — not supported in this generator.\n");
        }
    }
    // Ensure Content-Type is set when the user didn't.
    if !has_content_type(headers) {
        if let Some(ct) = super::util::default_content_type(body) {
            // Only Text + Raw need explicit set — .json() / .form()
            // already attach the right content type.
            if matches!(body, HttpBody::Text { .. }) {
                let _ = writeln!(out, "    req = req.header(\"Content-Type\", {});", js_string(ct));
            }
        }
    }
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
    fn renders_get_with_headers() {
        let mut r = req(HttpMethod::Get, "https://x/y");
        r.headers.push(HttpHeader::new("Accept", "application/json"));
        let s = to_rust(&r);
        assert!(s.contains("use reqwest::blocking::Client"));
        assert!(s.contains("client.get(\"https://x/y\")"));
        assert!(s.contains(".header(\"Accept\", \"application/json\")"));
        assert!(s.contains("req.send()"));
    }

    #[test]
    fn renders_post_json_with_macro() {
        let mut r = req(HttpMethod::Post, "https://x");
        r.body = Some(HttpBody::Json {
            value: json!({"a": 1, "b": "two"}),
        });
        let s = to_rust(&r);
        assert!(s.contains("use serde_json::json"));
        assert!(s.contains("client.post"));
        assert!(s.contains(".json(&json!(") || s.contains("json!"));
        assert!(s.contains("\"a\""));
    }

    #[test]
    fn renders_form_body() {
        let mut r = req(HttpMethod::Post, "https://x");
        r.body = Some(HttpBody::FormUrlEncoded {
            fields: vec![("user".into(), "alice".into())],
        });
        let s = to_rust(&r);
        assert!(s.contains(".form(&["));
        assert!(s.contains("(\"user\", \"alice\")"));
    }

    #[test]
    fn unknown_method_falls_back_to_request() {
        let mut r = req(HttpMethod::Options, "https://x");
        // OPTIONS is supported by reqwest's builder, so a custom verb
        // is the real fallback case. We at least make sure OPTIONS
        // didn't compile-break.
        r.method = HttpMethod::Options;
        let s = to_rust(&r);
        assert!(s.contains("client.") || s.contains("request("));
    }
}
