//! Generate a Python `requests` snippet.
//!
//! `requests` is chosen over `httpx` because it ships pre-installed
//! on more systems and the syntax is what most Python developers
//! recognise on sight. Snippet is top-to-bottom executable — copy,
//! save to `snippet.py`, run.

use std::fmt::Write as _;

use crate::http::{HttpBody, HttpRequest};

use super::util::{default_content_type, full_url, has_content_type, py_string};

/// Render as a Python `requests` snippet.
#[must_use]
pub fn to_python(req: &HttpRequest) -> String {
    let mut out = String::with_capacity(256);
    out.push_str("import requests\n\n");

    let url = full_url(req);
    let _ = writeln!(out, "url = {}", py_string(&url));

    // Headers — dedupe Content-Type so we don't shadow the user.
    let mut headers: Vec<(String, String)> = req
        .headers
        .iter()
        .map(|h| (h.name.clone(), h.value.clone()))
        .collect();
    if let Some(body) = &req.body {
        if !has_content_type(&req.headers) {
            if let Some(ct) = default_content_type(body) {
                headers.push(("Content-Type".into(), ct.to_string()));
            }
        }
    }
    if !headers.is_empty() {
        out.push_str("headers = {\n");
        for (k, v) in &headers {
            let _ = writeln!(out, "    {}: {},", py_string(k), py_string(v));
        }
        out.push_str("}\n");
    } else {
        out.push_str("headers = {}\n");
    }

    // Body shape drives the `requests` kwarg.
    let body_kwarg = body_to_python(&mut out, req);

    let _ = writeln!(
        out,
        "\nresp = requests.request({}, url, headers=headers{body_kwarg})",
        py_string(req.method.as_str()),
    );
    out.push_str("print(resp.status_code, resp.text)\n");
    out
}

fn body_to_python(out: &mut String, req: &HttpRequest) -> String {
    let Some(body) = &req.body else {
        return String::new();
    };
    match body {
        HttpBody::Text { content, .. } => {
            let _ = writeln!(out, "data = {}", py_string(content));
            ", data=data".to_string()
        }
        HttpBody::Json { value } => {
            let pretty = serde_json::to_string_pretty(value).unwrap_or_default();
            // Indent each line by 4 spaces so the dict literal aligns
            // with the previous statements.
            let indented = pretty
                .lines()
                .map(|l| format!("    {l}"))
                .collect::<Vec<_>>()
                .join("\n");
            let _ = writeln!(out, "payload = (\n{indented}\n)");
            ", json=payload".to_string()
        }
        HttpBody::FormUrlEncoded { fields } => {
            out.push_str("form = {\n");
            for (k, v) in fields {
                let _ = writeln!(out, "    {}: {},", py_string(k), py_string(v));
            }
            out.push_str("}\n");
            ", data=form".to_string()
        }
        HttpBody::Raw { .. } => {
            out.push_str("# TODO: binary body — not supported in this generator.\n");
            String::new()
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
        let mut r = req(HttpMethod::Get, "https://api.example.com/u");
        r.query.push(("q".into(), "widgets".into()));
        r.headers.push(HttpHeader::new("Accept", "application/json"));
        let s = to_python(&r);
        assert!(s.contains("import requests"));
        assert!(s.contains("url = 'https://api.example.com/u?q=widgets'"));
        assert!(s.contains("'Accept': 'application/json'"));
        assert!(s.contains("requests.request('GET'"));
    }

    #[test]
    fn renders_post_with_json_body() {
        let mut r = req(HttpMethod::Post, "https://x/u");
        r.body = Some(HttpBody::Json {
            value: json!({"name": "Alice"}),
        });
        let s = to_python(&r);
        assert!(s.contains("'Content-Type': 'application/json'"));
        assert!(s.contains("payload = "));
        assert!(s.contains("json=payload"));
    }

    #[test]
    fn renders_form_urlencoded() {
        let mut r = req(HttpMethod::Post, "https://x");
        r.body = Some(HttpBody::FormUrlEncoded {
            fields: vec![("user".into(), "alice".into())],
        });
        let s = to_python(&r);
        assert!(s.contains("form = {"));
        assert!(s.contains("'user': 'alice'"));
        assert!(s.contains("data=form"));
    }

    #[test]
    fn user_content_type_is_preserved() {
        let mut r = req(HttpMethod::Post, "https://x");
        r.headers
            .push(HttpHeader::new("Content-Type", "application/vnd.foo"));
        r.body = Some(HttpBody::Json { value: json!({}) });
        let s = to_python(&r);
        assert_eq!(s.matches("Content-Type").count(), 1);
        assert!(s.contains("application/vnd.foo"));
    }

    #[test]
    fn escapes_single_quote_in_strings() {
        let mut r = req(HttpMethod::Get, "https://x");
        r.headers.push(HttpHeader::new("X-Note", "it's fine"));
        let s = to_python(&r);
        assert!(s.contains(r"'it\'s fine'"));
    }
}
