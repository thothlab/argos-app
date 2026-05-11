//! Generate a Go `net/http` snippet — runnable as a `main.go`.
//!
//! No client reuse, no helper packages — the snippet is meant to be
//! pasted into a fresh file and run as-is. Response body is read via
//! `io.ReadAll`; we don't `defer resp.Body.Close()` for brevity, but
//! a comment notes the omission.

use std::fmt::Write as _;

use crate::http::{HttpBody, HttpRequest};

use super::util::{default_content_type, full_url, go_string, has_content_type};

/// Render as a Go `net/http` program. The output is a complete
/// `main` — pasted into a `.go` file it runs `go run snippet.go`
/// without modification.
#[must_use]
pub fn to_go(req: &HttpRequest) -> String {
    let url = full_url(req);
    let method = req.method.as_str();

    let mut imports: Vec<&str> = vec!["fmt", "io", "net/http"];

    // Body construction strategy (and matching imports).
    let (body_setup, body_arg, extra_imports) = build_body(req);
    for imp in extra_imports {
        if !imports.contains(&imp) {
            imports.push(imp);
        }
    }

    let mut out = String::with_capacity(384);
    out.push_str("package main\n\n");
    out.push_str("import (\n");
    for imp in &imports {
        let _ = writeln!(out, "\t{}", go_string(imp));
    }
    out.push_str(")\n\n");
    out.push_str("func main() {\n");

    if !body_setup.is_empty() {
        // Indent the precomputed body setup by one tab.
        for line in body_setup.lines() {
            let _ = writeln!(out, "\t{line}");
        }
    }

    let _ = writeln!(
        out,
        "\treq, err := http.NewRequest({}, {}, {})",
        go_string(method),
        go_string(&url),
        body_arg,
    );
    out.push_str("\tif err != nil { panic(err) }\n");

    // Headers — including a synthetic Content-Type when the user
    // didn't add one.
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
    for (k, v) in &headers {
        let _ = writeln!(
            out,
            "\treq.Header.Set({}, {})",
            go_string(k),
            go_string(v),
        );
    }

    out.push_str("\tresp, err := http.DefaultClient.Do(req)\n");
    out.push_str("\tif err != nil { panic(err) }\n");
    out.push_str("\t// defer resp.Body.Close()  // add this when reusing the client.\n");
    out.push_str("\tbody, _ := io.ReadAll(resp.Body)\n");
    out.push_str("\tfmt.Println(resp.StatusCode, string(body))\n");
    out.push_str("}\n");
    out
}

/// Returns `(setup_code, body_arg, extra_imports)`.
///
/// `setup_code` runs before `http.NewRequest`; `body_arg` is the
/// expression for the `body io.Reader` arg (`nil` when absent);
/// `extra_imports` adds packages like `bytes` or `strings`.
fn build_body(req: &HttpRequest) -> (String, String, Vec<&'static str>) {
    let Some(body) = &req.body else {
        return (String::new(), "nil".into(), Vec::new());
    };
    match body {
        HttpBody::Text { content, .. } => (
            format!("body := strings.NewReader({})\n", go_string(content)),
            "body".into(),
            vec!["strings"],
        ),
        HttpBody::Json { value } => {
            let json = serde_json::to_string(value).unwrap_or_default();
            (
                format!("body := bytes.NewBufferString({})\n", go_string(&json)),
                "body".into(),
                vec!["bytes"],
            )
        }
        HttpBody::FormUrlEncoded { fields } => {
            let mut setup = String::from("form := url.Values{}\n");
            for (k, v) in fields {
                let _ = writeln!(setup, "form.Add({}, {})", go_string(k), go_string(v));
            }
            setup.push_str("body := strings.NewReader(form.Encode())\n");
            (setup, "body".into(), vec!["net/url", "strings"])
        }
        HttpBody::Raw { .. } => (
            "// TODO: binary body — not supported in this generator.\n".to_string(),
            "nil".into(),
            Vec::new(),
        ),
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
    fn emits_runnable_main_for_get() {
        let mut r = req(HttpMethod::Get, "https://x/y");
        r.headers.push(HttpHeader::new("Accept", "application/json"));
        let s = to_go(&r);
        assert!(s.starts_with("package main\n"));
        assert!(s.contains("\"net/http\""));
        assert!(s.contains("http.NewRequest(\"GET\", \"https://x/y\", nil)"));
        assert!(s.contains("req.Header.Set(\"Accept\", \"application/json\")"));
        assert!(s.contains("http.DefaultClient.Do(req)"));
    }

    #[test]
    fn json_body_uses_bytes_buffer() {
        let mut r = req(HttpMethod::Post, "https://x");
        r.body = Some(HttpBody::Json {
            value: json!({"a": 1}),
        });
        let s = to_go(&r);
        assert!(s.contains("\"bytes\""));
        assert!(s.contains("bytes.NewBufferString(\"{\\\"a\\\":1}\")"));
        assert!(s.contains("Content-Type"));
        assert!(s.contains("application/json"));
    }

    #[test]
    fn form_body_uses_url_values() {
        let mut r = req(HttpMethod::Post, "https://x");
        r.body = Some(HttpBody::FormUrlEncoded {
            fields: vec![("user".into(), "alice".into())],
        });
        let s = to_go(&r);
        assert!(s.contains("\"net/url\""));
        assert!(s.contains("form := url.Values{}"));
        assert!(s.contains("form.Add(\"user\", \"alice\")"));
        assert!(s.contains("form.Encode()"));
    }

    #[test]
    fn text_body_uses_strings_reader() {
        let mut r = req(HttpMethod::Post, "https://x");
        r.body = Some(HttpBody::Text {
            content: "hi".into(),
            content_type: "text/plain".into(),
        });
        let s = to_go(&r);
        assert!(s.contains("\"strings\""));
        assert!(s.contains("strings.NewReader(\"hi\")"));
        assert!(s.contains("text/plain"));
    }
}
