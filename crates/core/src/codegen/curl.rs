//! Generate a `curl` command from an [`HttpRequest`].
//!
//! Output is multi-line with `\` continuation, ready to paste into a shell or
//! a `.sh` file. Single quotes are used for arguments; embedded single quotes
//! are escaped via the standard `'\''` pattern so the result is safe under
//! `bash`, `zsh` and `dash`.

use std::fmt::Write as _;

use crate::http::{HttpBody, HttpHeader, HttpMethod, HttpRequest};

/// Render the request as a multi-line `curl` invocation.
///
/// Conventions:
/// - `GET` is omitted (curl's default).
/// - `--data-urlencode` is used for `FormUrlEncoded` bodies — preserves
///   exact key/value boundaries even with special chars.
/// - `--data-binary @-` with a heredoc is used for `Raw` bodies that are not
///   valid UTF-8; otherwise raw bytes are inlined as a quoted string.
/// - Query parameters are merged into the URL via the engine's URL builder
///   (matches what would actually be sent).
#[must_use]
pub fn to_curl(req: &HttpRequest) -> String {
    let mut out = String::with_capacity(128);
    out.push_str("curl");

    if req.method != HttpMethod::Get {
        write!(out, " \\\n  -X {}", req.method.as_str()).unwrap();
    }

    let url = merge_query(&req.url, &req.query);
    write!(out, " \\\n  {}", shell_quote(&url)).unwrap();

    for HttpHeader { name, value } in &req.headers {
        write!(
            out,
            " \\\n  -H {}",
            shell_quote(&format!("{name}: {value}"))
        )
        .unwrap();
    }

    if let Some(body) = &req.body {
        append_body(&mut out, body);
    }

    out
}

fn append_body(out: &mut String, body: &HttpBody) {
    match body {
        HttpBody::Text {
            content,
            content_type,
        } => {
            write!(
                out,
                " \\\n  -H {} \\\n  --data {}",
                shell_quote(&format!("Content-Type: {content_type}")),
                shell_quote(content)
            )
            .unwrap();
        }
        HttpBody::Json { value } => {
            let json = serde_json::to_string(value).unwrap_or_default();
            write!(
                out,
                " \\\n  -H 'Content-Type: application/json' \\\n  --data {}",
                shell_quote(&json)
            )
            .unwrap();
        }
        HttpBody::FormUrlEncoded { fields } => {
            for (k, v) in fields {
                write!(
                    out,
                    " \\\n  --data-urlencode {}",
                    shell_quote(&format!("{k}={v}"))
                )
                .unwrap();
            }
        }
        HttpBody::Raw {
            bytes,
            content_type,
        } => {
            write!(
                out,
                " \\\n  -H {}",
                shell_quote(&format!("Content-Type: {content_type}"))
            )
            .unwrap();
            // Inline as quoted string when valid UTF-8, otherwise base64 + decode.
            if let Ok(text) = std::str::from_utf8(bytes) {
                write!(out, " \\\n  --data-binary {}", shell_quote(text)).unwrap();
            } else {
                // Non-UTF8 bodies — render a hint comment and base64.
                use base64_fallback as b64;
                write!(
                    out,
                    " \\\n  --data-binary @<(echo {} | base64 -d)",
                    shell_quote(&b64::encode(bytes))
                )
                .unwrap();
            }
        }
    }
}

/// Single-quote a string for safe inclusion in a shell argument.
///
/// Embedded `'` characters are escaped using the canonical `'\''` pattern.
fn shell_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\'' {
            out.push_str(r"'\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

fn merge_query(base: &str, query: &[(String, String)]) -> String {
    if query.is_empty() {
        return base.to_string();
    }
    // We use the same logic the engine uses, so the generated curl matches what
    // would actually be sent over the wire.
    if let Ok(mut u) = url::Url::parse(base) {
        {
            let mut pairs = u.query_pairs_mut();
            for (k, v) in query {
                pairs.append_pair(k, v);
            }
        }
        u.to_string()
    } else {
        // Fallback when the URL is unparsed (e.g. user is mid-typing).
        let sep = if base.contains('?') { '&' } else { '?' };
        let parts: Vec<String> = query
            .iter()
            .map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v)))
            .collect();
        format!("{base}{sep}{}", parts.join("&"))
    }
}

fn url_encode(s: &str) -> String {
    // Minimal percent-encoding for the fallback path. Production code uses
    // `url::Url::query_pairs_mut`; this is only hit for user-typed gibberish.
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            other => write!(&mut out, "%{other:02X}").unwrap(),
        }
    }
    out
}

// Vendored 12-line base64 encoder so we don't pull in a `base64` crate just
// for the rare non-UTF8 raw-body case.
mod base64_fallback {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    pub fn encode(input: &[u8]) -> String {
        let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
        for chunk in input.chunks(3) {
            let b0 = chunk[0];
            let b1 = chunk.get(1).copied().unwrap_or(0);
            let b2 = chunk.get(2).copied().unwrap_or(0);
            out.push(TABLE[(b0 >> 2) as usize] as char);
            out.push(TABLE[(((b0 & 0b11) << 4) | (b1 >> 4)) as usize] as char);
            if chunk.len() > 1 {
                out.push(TABLE[(((b1 & 0b1111) << 2) | (b2 >> 6)) as usize] as char);
            } else {
                out.push('=');
            }
            if chunk.len() > 2 {
                out.push(TABLE[(b2 & 0b11_1111) as usize] as char);
            } else {
                out.push('=');
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn simple_get_omits_method() {
        let req = HttpRequest {
            url: "https://api.example.com/users".into(),
            ..Default::default()
        };
        let cmd = to_curl(&req);
        assert!(cmd.starts_with("curl"));
        assert!(!cmd.contains("-X "));
        assert!(cmd.contains("'https://api.example.com/users'"));
    }

    #[test]
    fn explicit_method_is_emitted() {
        let req = HttpRequest {
            method: HttpMethod::Delete,
            url: "https://api.example.com/users/42".into(),
            ..Default::default()
        };
        let cmd = to_curl(&req);
        assert!(cmd.contains("-X DELETE"));
    }

    #[test]
    fn query_params_merge_into_url() {
        let req = HttpRequest {
            url: "https://api.example.com/users".into(),
            query: vec![("page".into(), "2".into()), ("limit".into(), "50".into())],
            ..Default::default()
        };
        let cmd = to_curl(&req);
        assert!(cmd.contains("page=2"));
        assert!(cmd.contains("limit=50"));
    }

    #[test]
    fn headers_are_each_h_flag() {
        let req = HttpRequest {
            url: "https://api.example.com/me".into(),
            headers: vec![
                HttpHeader::new("Authorization", "Bearer abc"),
                HttpHeader::new("X-Trace-Id", "deadbeef"),
            ],
            ..Default::default()
        };
        let cmd = to_curl(&req);
        assert!(cmd.contains("-H 'Authorization: Bearer abc'"));
        assert!(cmd.contains("-H 'X-Trace-Id: deadbeef'"));
    }

    #[test]
    fn json_body_uses_data_flag() {
        let req = HttpRequest {
            method: HttpMethod::Post,
            url: "https://api.example.com/users".into(),
            body: Some(HttpBody::Json {
                value: json!({ "name": "Alice", "role": "admin" }),
            }),
            ..Default::default()
        };
        let cmd = to_curl(&req);
        assert!(cmd.contains("-X POST"));
        assert!(cmd.contains("-H 'Content-Type: application/json'"));
        assert!(cmd.contains(r#"--data '{"name":"Alice","role":"admin"}'"#));
    }

    #[test]
    fn form_body_uses_data_urlencode_per_field() {
        let req = HttpRequest {
            method: HttpMethod::Post,
            url: "https://api.example.com/login".into(),
            body: Some(HttpBody::FormUrlEncoded {
                fields: vec![
                    ("user".into(), "alice".into()),
                    ("pass".into(), "p@ss w0rd".into()),
                ],
            }),
            ..Default::default()
        };
        let cmd = to_curl(&req);
        assert!(cmd.contains("--data-urlencode 'user=alice'"));
        assert!(cmd.contains("--data-urlencode 'pass=p@ss w0rd'"));
    }

    #[test]
    fn single_quotes_in_values_are_escaped_for_shell() {
        let req = HttpRequest {
            method: HttpMethod::Post,
            url: "https://api.example.com/echo".into(),
            body: Some(HttpBody::Text {
                content: "it's working".into(),
                content_type: "text/plain".into(),
            }),
            ..Default::default()
        };
        let cmd = to_curl(&req);
        assert!(cmd.contains(r"'it'\''s working'"));
    }

    #[test]
    fn raw_utf8_body_is_inlined_as_quoted_string() {
        let req = HttpRequest {
            method: HttpMethod::Put,
            url: "https://api.example.com/blob".into(),
            body: Some(HttpBody::Raw {
                bytes: b"hello world".to_vec(),
                content_type: "text/plain".into(),
            }),
            ..Default::default()
        };
        let cmd = to_curl(&req);
        assert!(cmd.contains("--data-binary 'hello world'"));
    }

    #[test]
    fn raw_non_utf8_body_uses_base64_decode_substitution() {
        let req = HttpRequest {
            method: HttpMethod::Put,
            url: "https://api.example.com/blob".into(),
            body: Some(HttpBody::Raw {
                bytes: vec![0xff, 0xfe, 0x00, 0x01],
                content_type: "application/octet-stream".into(),
            }),
            ..Default::default()
        };
        let cmd = to_curl(&req);
        assert!(cmd.contains("base64 -d"));
        assert!(cmd.contains("//4AAQ==")); // base64 of 0xff 0xfe 0x00 0x01
    }

    #[test]
    fn output_is_line_continued() {
        let req = HttpRequest {
            method: HttpMethod::Post,
            url: "https://api.example.com/x".into(),
            headers: vec![HttpHeader::new("X-One", "1")],
            ..Default::default()
        };
        let cmd = to_curl(&req);
        // First line = `curl`, every subsequent flag is on its own line.
        let lines: Vec<&str> = cmd.lines().collect();
        assert_eq!(lines.first().copied(), Some("curl \\"));
        assert!(lines.len() >= 3);
        for line in &lines[..lines.len() - 1] {
            assert!(
                line.ends_with('\\'),
                "non-final line must end with continuation: {line:?}"
            );
        }
    }
}
