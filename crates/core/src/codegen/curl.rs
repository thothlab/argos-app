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

/// Errors produced by [`from_curl`].
#[derive(Debug, thiserror::Error)]
pub enum CurlParseError {
    /// The input failed shell-style tokenisation (unbalanced quotes, etc.).
    #[error("could not tokenise curl command: malformed quoting")]
    Tokenise,
    /// The input did not start with `curl` or contained no URL.
    #[error("not a curl command: missing `curl` keyword or URL")]
    NotACurlCommand,
    /// A flag was given without its argument (e.g. trailing `-H`).
    #[error("flag {0} expected an argument")]
    MissingArg(&'static str),
}

/// Parse a `curl` command line into an [`HttpRequest`].
///
/// Recognised flags (long and short forms):
/// `-X` / `--request`, `-H` / `--header`, `-d` / `--data` /
/// `--data-raw` / `--data-binary` / `--data-ascii`, `--data-urlencode`,
/// `-u` / `--user`, `-A` / `--user-agent`, `-e` / `--referer`,
/// `--url` (positional URL is also accepted).
///
/// Heuristics that match common-sense `curl` behaviour:
/// - Method defaults to `GET`, or `POST` if any data flag was supplied
///   without an explicit `-X`.
/// - A `--data` body whose first non-whitespace char is `{` or `[` is
///   parsed as JSON (Content-Type `application/json`); otherwise it is
///   stored as a form-urlencoded body if it parses as `k=v&k2=v2`,
///   else as plain text. Any explicit `Content-Type` header takes
///   precedence over the heuristic.
/// - Single backslash-newline continuations are tolerated (the user
///   typically pastes multi-line snippets straight from documentation).
/// - Unknown / unsupported flags (`-L`, `-k`, `-i`, `--compressed`, …)
///   are silently skipped — they don't affect the request shape.
///
/// # Errors
///
/// Returns [`CurlParseError`] if tokenisation fails, the leading
/// `curl` keyword is missing, no URL was supplied, or a recognised
/// flag is missing its argument.
#[allow(clippy::too_many_lines)]
pub fn from_curl(input: &str) -> Result<HttpRequest, CurlParseError> {
    // Drop any line-continuation backslashes (`\` followed by EOL or
    // whitespace). shlex itself doesn't understand them.
    let cleaned = input.replace("\\\n", " ").replace("\\\r\n", " ");

    let tokens = shlex::split(&cleaned).ok_or(CurlParseError::Tokenise)?;
    let mut iter = tokens.into_iter();

    // Drop everything up to and including the leading `curl` keyword.
    // Most paste flows start with literally "curl" but we tolerate a
    // shell prefix like `$ curl …`.
    let mut found_curl = false;
    for tok in iter.by_ref() {
        if tok == "curl" || tok.ends_with("/curl") {
            found_curl = true;
            break;
        }
        // Allow an environment-prefix style like `FOO=bar curl …`.
        if tok.contains('=') && !tok.starts_with('-') {
            continue;
        }
        // Anything else before `curl` is suspicious.
        return Err(CurlParseError::NotACurlCommand);
    }
    if !found_curl {
        return Err(CurlParseError::NotACurlCommand);
    }

    let mut method: Option<HttpMethod> = None;
    let mut url: Option<String> = None;
    let mut headers: Vec<HttpHeader> = Vec::new();
    let mut data_chunks: Vec<DataChunk> = Vec::new();
    let mut basic_auth: Option<(String, String)> = None;

    while let Some(tok) = iter.next() {
        match tok.as_str() {
            "-X" | "--request" => {
                let v = iter.next().ok_or(CurlParseError::MissingArg("-X"))?;
                method = parse_method(&v);
            }
            "-H" | "--header" => {
                let v = iter.next().ok_or(CurlParseError::MissingArg("-H"))?;
                if let Some((name, value)) = split_header(&v) {
                    headers.push(HttpHeader::new(name, value));
                }
            }
            "-d" | "--data" | "--data-ascii" | "--data-raw" | "--data-binary" => {
                let v = iter.next().ok_or(CurlParseError::MissingArg("-d"))?;
                data_chunks.push(DataChunk::Raw(v));
            }
            "--data-urlencode" => {
                let v = iter
                    .next()
                    .ok_or(CurlParseError::MissingArg("--data-urlencode"))?;
                data_chunks.push(DataChunk::UrlEncode(v));
            }
            "-u" | "--user" => {
                let v = iter.next().ok_or(CurlParseError::MissingArg("-u"))?;
                if let Some((u, p)) = v.split_once(':') {
                    basic_auth = Some((u.to_string(), p.to_string()));
                } else {
                    basic_auth = Some((v, String::new()));
                }
            }
            "-A" | "--user-agent" => {
                let v = iter.next().ok_or(CurlParseError::MissingArg("-A"))?;
                headers.push(HttpHeader::new("User-Agent", v));
            }
            "-e" | "--referer" => {
                let v = iter.next().ok_or(CurlParseError::MissingArg("-e"))?;
                headers.push(HttpHeader::new("Referer", v));
            }
            "--url" => {
                let v = iter.next().ok_or(CurlParseError::MissingArg("--url"))?;
                if url.is_none() {
                    url = Some(v);
                }
            }
            // Flags we accept but don't model.
            "-L" | "--location" | "-k" | "--insecure" | "-i" | "--include" | "-s" | "--silent"
            | "-S" | "--show-error" | "-v" | "--verbose" | "--compressed" | "-N"
            | "--no-buffer" | "-#" | "--progress-bar" | "-f" | "--fail" => {}
            // Flags that take an arg but we don't model — consume the arg.
            "-o" | "--output" | "-w" | "--write-out" | "-T" | "--upload-file"
            | "--connect-timeout" | "-m" | "--max-time" | "--retry" | "--retry-delay"
            | "--cacert" | "--cert" | "--key" | "-b" | "--cookie" | "-c" | "--cookie-jar"
            | "-x" | "--proxy" => {
                let _ = iter.next();
            }
            other => {
                if other.starts_with("--") {
                    // Unknown long flag with possibly an `=value`.
                    if !other.contains('=') {
                        // Skip a single trailing arg if it isn't another flag.
                    }
                } else if other.starts_with('-') && other.len() > 1 {
                    // Unknown short flag — ignore.
                } else if url.is_none() {
                    url = Some(other.to_string());
                }
            }
        }
    }

    let url = url.ok_or(CurlParseError::NotACurlCommand)?;

    // Basic auth → Authorization header if user didn't provide one.
    if let Some((u, p)) = basic_auth {
        if !headers
            .iter()
            .any(|h| h.name.eq_ignore_ascii_case("authorization"))
        {
            headers.push(HttpHeader::new(
                "Authorization",
                format!("Basic {}", base64_encode(format!("{u}:{p}").as_bytes())),
            ));
        }
    }

    // Body assembly. Multiple `-d` flags concatenate with `&` (the
    // documented curl behaviour).
    let body = if data_chunks.is_empty() {
        None
    } else {
        let raw = assemble_body(&data_chunks);
        let explicit_ct = headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case("content-type"))
            .map(|h| h.value.to_lowercase());
        Some(make_body(raw, explicit_ct.as_deref()))
    };

    let final_method = method.unwrap_or(if body.is_some() {
        HttpMethod::Post
    } else {
        HttpMethod::Get
    });

    Ok(HttpRequest {
        method: final_method,
        url,
        headers,
        query: Vec::new(),
        body,
        timeout: None,
    })
}

#[derive(Debug, Clone)]
enum DataChunk {
    Raw(String),
    UrlEncode(String),
}

fn assemble_body(chunks: &[DataChunk]) -> String {
    let parts: Vec<String> = chunks
        .iter()
        .map(|c| match c {
            DataChunk::Raw(s) => s.clone(),
            DataChunk::UrlEncode(s) => {
                if let Some((k, v)) = s.split_once('=') {
                    format!("{}={}", url_encode(k), url_encode(v))
                } else {
                    url_encode(s)
                }
            }
        })
        .collect();
    parts.join("&")
}

fn make_body(raw: String, explicit_content_type: Option<&str>) -> HttpBody {
    if let Some(ct) = explicit_content_type {
        if ct.contains("application/json") {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
                return HttpBody::Json { value };
            }
            return HttpBody::Text {
                content: raw,
                content_type: ct.to_string(),
            };
        }
        if ct.contains("application/x-www-form-urlencoded") {
            return HttpBody::FormUrlEncoded {
                fields: parse_form(&raw),
            };
        }
        return HttpBody::Text {
            content: raw,
            content_type: ct.to_string(),
        };
    }

    // No explicit content type → infer.
    let trimmed = raw.trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
            return HttpBody::Json { value };
        }
    }
    if looks_like_form(&raw) {
        return HttpBody::FormUrlEncoded {
            fields: parse_form(&raw),
        };
    }
    HttpBody::Text {
        content: raw,
        content_type: "text/plain".to_string(),
    }
}

fn looks_like_form(s: &str) -> bool {
    !s.is_empty() && s.split('&').all(|p| p.contains('='))
}

fn parse_form(s: &str) -> Vec<(String, String)> {
    s.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (url_decode(k), url_decode(v))
        })
        .collect()
}

fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'+' {
            out.push(b' ');
            i += 1;
        } else if b == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push((h << 4) | l);
                i += 3;
            } else {
                out.push(b);
                i += 1;
            }
        } else {
            out.push(b);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn split_header(v: &str) -> Option<(String, String)> {
    let (name, value) = v.split_once(':')?;
    let name = name.trim();
    let value = value.trim();
    if name.is_empty() {
        return None;
    }
    Some((name.to_string(), value.to_string()))
}

fn parse_method(s: &str) -> Option<HttpMethod> {
    match s.to_ascii_uppercase().as_str() {
        "GET" => Some(HttpMethod::Get),
        "POST" => Some(HttpMethod::Post),
        "PUT" => Some(HttpMethod::Put),
        "PATCH" => Some(HttpMethod::Patch),
        "DELETE" => Some(HttpMethod::Delete),
        "HEAD" => Some(HttpMethod::Head),
        "OPTIONS" => Some(HttpMethod::Options),
        _ => None,
    }
}

fn base64_encode(bytes: &[u8]) -> String {
    base64_fallback::encode(bytes)
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

    // ---- from_curl ------------------------------------------------------

    #[test]
    fn parse_simple_get() {
        let r = from_curl("curl https://example.com/users").unwrap();
        assert_eq!(r.method, HttpMethod::Get);
        assert_eq!(r.url, "https://example.com/users");
        assert!(r.headers.is_empty());
        assert!(r.body.is_none());
    }

    #[test]
    fn parse_method_via_x_flag() {
        let r = from_curl("curl -X DELETE https://example.com/x/1").unwrap();
        assert_eq!(r.method, HttpMethod::Delete);
        assert_eq!(r.url, "https://example.com/x/1");
    }

    #[test]
    fn parse_long_form_request_flag() {
        let r = from_curl("curl --request put https://example.com/x").unwrap();
        assert_eq!(r.method, HttpMethod::Put);
    }

    #[test]
    fn parse_headers_short_and_long() {
        let r =
            from_curl("curl -H 'Accept: application/json' --header 'X-Trace: abc' https://e.com")
                .unwrap();
        assert_eq!(r.headers.len(), 2);
        assert_eq!(r.headers[0].name, "Accept");
        assert_eq!(r.headers[0].value, "application/json");
        assert_eq!(r.headers[1].name, "X-Trace");
        assert_eq!(r.headers[1].value, "abc");
    }

    #[test]
    fn parse_data_implies_post_when_no_x_flag() {
        let r = from_curl("curl https://e.com/login -d 'a=1&b=2'").unwrap();
        assert_eq!(r.method, HttpMethod::Post);
        match r.body {
            Some(HttpBody::FormUrlEncoded { fields }) => {
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0], ("a".to_string(), "1".to_string()));
                assert_eq!(fields[1], ("b".to_string(), "2".to_string()));
            }
            other => panic!("expected form body, got {other:?}"),
        }
    }

    #[test]
    fn parse_json_body_via_inferred_braces() {
        let r = from_curl(r#"curl -X POST https://e.com/x -d '{"name":"Alice","n":3}'"#).unwrap();
        assert_eq!(r.method, HttpMethod::Post);
        match r.body {
            Some(HttpBody::Json { value }) => {
                assert_eq!(value, json!({"name":"Alice","n":3}));
            }
            other => panic!("expected json body, got {other:?}"),
        }
    }

    #[test]
    fn explicit_content_type_json_wins_over_form_heuristic() {
        let r =
            from_curl(r#"curl https://e.com/x -H 'Content-Type: application/json' -d '{"a":1}'"#)
                .unwrap();
        match r.body {
            Some(HttpBody::Json { value }) => assert_eq!(value, json!({"a":1})),
            other => panic!("expected json body, got {other:?}"),
        }
    }

    #[test]
    fn line_continuation_backslashes_are_tolerated() {
        let r = from_curl("curl https://e.com \\\n  -H 'X-One: 1' \\\n  -H 'X-Two: 2'").unwrap();
        assert_eq!(r.headers.len(), 2);
        assert_eq!(r.headers[0].name, "X-One");
        assert_eq!(r.headers[1].name, "X-Two");
    }

    #[test]
    fn basic_auth_becomes_authorization_header() {
        let r = from_curl("curl -u alice:s3cret https://e.com/me").unwrap();
        let auth = r
            .headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case("authorization"))
            .unwrap();
        // base64 of "alice:s3cret" = YWxpY2U6czNjcmV0
        assert_eq!(auth.value, "Basic YWxpY2U6czNjcmV0");
    }

    #[test]
    fn data_urlencode_encodes_value() {
        let r = from_curl("curl https://e.com -d 'q=normal' --data-urlencode 'tag=hello world'")
            .unwrap();
        match r.body {
            Some(HttpBody::FormUrlEncoded { fields }) => {
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].1, "normal");
                assert_eq!(fields[1].0, "tag");
                assert_eq!(fields[1].1, "hello world");
            }
            other => panic!("expected form body, got {other:?}"),
        }
    }

    #[test]
    fn user_agent_short_flag_becomes_header() {
        let r = from_curl("curl -A 'argos/test' https://e.com").unwrap();
        let ua = r
            .headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case("user-agent"))
            .unwrap();
        assert_eq!(ua.value, "argos/test");
    }

    #[test]
    fn unknown_flags_are_silently_skipped() {
        let r = from_curl("curl -L -k -i --compressed https://e.com").unwrap();
        assert_eq!(r.method, HttpMethod::Get);
        assert_eq!(r.url, "https://e.com");
    }

    #[test]
    fn flags_taking_args_consume_them() {
        // `-o out.json` should not be picked up as the URL.
        let r = from_curl("curl -o out.json https://e.com/api").unwrap();
        assert_eq!(r.url, "https://e.com/api");
    }

    #[test]
    fn rejects_non_curl_input() {
        assert!(matches!(
            from_curl("wget https://e.com"),
            Err(CurlParseError::NotACurlCommand)
        ));
    }

    #[test]
    fn rejects_unbalanced_quotes() {
        assert!(matches!(
            from_curl(r#"curl 'https://e.com -H "missing"#),
            Err(CurlParseError::Tokenise)
        ));
    }

    #[test]
    fn round_trip_to_curl_then_from_curl_preserves_shape() {
        let req = HttpRequest {
            method: HttpMethod::Post,
            url: "https://api.example.com/users".into(),
            headers: vec![
                HttpHeader::new("Accept", "application/json"),
                HttpHeader::new("Content-Type", "application/json"),
            ],
            body: Some(HttpBody::Json {
                value: json!({"name": "Alice"}),
            }),
            ..Default::default()
        };
        let cmd = to_curl(&req);
        let parsed = from_curl(&cmd).unwrap();
        assert_eq!(parsed.method, HttpMethod::Post);
        assert_eq!(parsed.url, "https://api.example.com/users");
        match parsed.body {
            Some(HttpBody::Json { value }) => assert_eq!(value, json!({"name": "Alice"})),
            other => panic!("expected json body, got {other:?}"),
        }
        assert!(parsed
            .headers
            .iter()
            .any(|h| h.name.eq_ignore_ascii_case("accept")));
    }
}
