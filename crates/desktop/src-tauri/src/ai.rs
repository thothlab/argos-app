//! BYOK AI calls — provider adapters for log-extraction (and any
//! future AI-backed feature).
//!
//! Argos never proxies AI traffic — the user's key + the log payload
//! go from the desktop binary straight to the provider they picked.
//! See `apps/docs/importing.md` for the privacy story shown to users.
//!
//! The chunk loop lives in the **UI**, not here. Rust exposes three
//! commands:
//!
//! - [`ai_extract_split`] turns a log into a list of byte-bounded
//!   chunks on line boundaries (with a small overlap), so the UI can
//!   show "this will be N chunks" before committing.
//! - [`ai_extract_chunk`] sends a single chunk to the configured
//!   provider and parses out a `Vec<ExtractedRequest>`. The UI loops,
//!   dedups, and decides what to do on per-chunk errors (Retry / Skip
//!   / Stop). The in-flight HTTP call races a shared cancel flag, so a
//!   Cancel from the UI tears down the reqwest future immediately
//!   rather than waiting up to the per-chunk timeout.
//! - [`ai_extract_cancel`] flips that shared cancel flag.

use argos_core::{HttpBody, HttpHeader, HttpMethod, HttpRequest};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::State;

use crate::{http_client, AppState};

/// Hard ceiling on a single Extract job. The input goes through
/// chunking, so this is more about cost / wall-clock than provider
/// context limits.
pub const MAX_LOG_BYTES: usize = 256 * 1024;

/// Target bytes per chunk. Conservative so each chunk fits well inside
/// `OLLAMA_NUM_CTX` together with the system prompt and the JSON
/// response budget.
pub const TARGET_CHUNK_BYTES: usize = 8 * 1024;

/// Lines from the tail of chunk N copied into the head of chunk N+1 so
/// a request whose lines straddle a boundary isn't lost. Kept tiny —
/// the UI dedups the duplicates a larger overlap would create.
const CHUNK_OVERLAP_LINES: usize = 8;

/// Bound the KV-cache Ollama allocates. Without this, Ollama reserves
/// memory for the model's *default* context (32K-128K on modern
/// models) regardless of actual prompt size; on Apple Silicon that can
/// exhaust unified memory, stall Metal, and trigger a WindowServer
/// watchdog reboot.
const OLLAMA_NUM_CTX: u32 = 8192;

/// How often the cancellation watcher polls the shared flag. 50 ms is
/// well below human-perceived latency — the user feels Cancel as
/// instant even though the future actually wakes once per tick.
const CANCEL_POLL_MS: u64 = 50;

/// Shared cancel flag — polled by the per-chunk `tokio::select!` arm
/// and flipped by the `ai_extract_cancel` command. Cleared at the top
/// of every chunk call so a previous run's signal can't poison the
/// next one.
pub type AiExtractCancel = Arc<AtomicBool>;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiExtractInput {
    pub provider: String,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExtractedRequest {
    /// Display name shown in the import preview and used as filename slug.
    pub name: String,
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<NameValue>,
    #[serde(default)]
    pub query: Vec<NameValue>,
    #[serde(default)]
    pub body: Option<ExtractedBody>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NameValue {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExtractedBody {
    Json { value: serde_json::Value },
    Text { content: String },
    Form { fields: Vec<NameValue> },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiChunkResponse {
    /// Requests extracted from this chunk. May be empty.
    pub requests: Vec<ExtractedRequest>,
    /// Raw provider output before JSON parsing. The UI keeps this
    /// around for the per-chunk transcript shown when extraction
    /// quality is bad.
    pub raw: String,
}

#[derive(Debug, Deserialize)]
struct ExtractEnvelope {
    requests: Vec<ExtractedRequest>,
}

/// Schema embedded in the system prompt so the model has the exact
/// shape it should emit. Keep in sync with [`ExtractedRequest`].
const SCHEMA_DESCRIPTION: &str = r#"{
  "requests": [
    {
      "name": "POST createOrder",
      "method": "POST",
      "url": "https://api.example.com/v1/orders",
      "headers": [
        { "name": "Authorization", "value": "Bearer eyJ…" },
        { "name": "Content-Type", "value": "application/json" }
      ],
      "query": [
        { "name": "draft", "value": "true" }
      ],
      "body": { "kind": "json", "value": { "amount": 100, "currency": "USD" } }
    }
  ]
}"#;

const SYSTEM_PROMPT_TEMPLATE: &str = r#"You are an HTTP request extractor for the Argos API client.
Read the log below and emit ONLY JSON in exactly this shape:

{{SCHEMA}}

Rules:
- Emit every distinct HTTP request you find — there may be many in one log.
- `method` must be UPPERCASE: GET / POST / PUT / PATCH / DELETE / HEAD / OPTIONS.
- `url` includes scheme + host + path; query string goes into `query`, NOT into `url`.
- `headers` mirrors the log verbatim. Keep auth headers + tokens (the user pastes their own logs and needs them for replay).
- `body.kind` is "json" if the body parses as JSON, "form" if it's `application/x-www-form-urlencoded`, otherwise "text".
- For JSON bodies, `value` is the parsed JSON value (object / array / scalar). Do not stringify it.
- Pick a short human-readable `name` like "METHOD lastSegment" (e.g. "POST createOrder", "GET userById").
- Ignore stack traces, GC noise, lifecycle / framework chatter — only emit actual HTTP requests.
- If the log contains zero requests, return `{"requests": []}`.
- DO NOT wrap the output in markdown fences. DO NOT explain. JSON only."#;

const USER_PROMPT_PREFIX: &str = "Log to extract requests from:\n\n";

fn build_system_prompt() -> String {
    SYSTEM_PROMPT_TEMPLATE.replace("{{SCHEMA}}", SCHEMA_DESCRIPTION)
}

/// Split a log into byte-bounded chunks on line boundaries with a
/// small overlap. The cap is soft — a single line longer than
/// `TARGET_CHUNK_BYTES` is emitted as its own chunk rather than cut
/// mid-line.
pub fn split_into_chunks(log: &str) -> Vec<String> {
    if log.len() <= TARGET_CHUNK_BYTES {
        return if log.is_empty() {
            vec![]
        } else {
            vec![log.to_string()]
        };
    }
    let lines: Vec<&str> = log.split_inclusive('\n').collect();
    let mut chunks = Vec::new();
    let mut start = 0usize;
    while start < lines.len() {
        let mut size = 0usize;
        let mut end = start;
        while end < lines.len() && size + lines[end].len() <= TARGET_CHUNK_BYTES {
            size += lines[end].len();
            end += 1;
        }
        if end == start {
            // Single line larger than the target — emit it alone.
            end = start + 1;
        }
        chunks.push(lines[start..end].concat());
        if end >= lines.len() {
            break;
        }
        let span = end - start;
        let advance = span.saturating_sub(CHUNK_OVERLAP_LINES).max(1);
        start += advance;
    }
    chunks
}

/// Tauri command — the UI calls this once at the start of an Extract
/// job to get the chunk list (so it can show "log will be N chunks"
/// and drive a Retry/Skip/Stop loop with per-chunk control). Resets
/// the shared cancel flag so a previous run's signal doesn't bleed
/// into this one.
#[tauri::command]
pub fn ai_extract_split(
    cancel: State<'_, AiExtractCancel>,
    log_text: String,
) -> Result<Vec<String>, String> {
    if log_text.is_empty() {
        return Err("log text is empty".into());
    }
    if log_text.len() > MAX_LOG_BYTES {
        return Err(format!(
            "log is {} bytes; max is {} bytes — trim before extracting",
            log_text.len(),
            MAX_LOG_BYTES
        ));
    }
    cancel.store(false, Ordering::SeqCst);
    Ok(split_into_chunks(&log_text))
}

/// Tauri command — send one chunk to the configured provider and
/// parse the extracted requests. Returns an empty list (with the raw
/// response captured) if the provider responded with JSON that
/// doesn't match the schema; returns `Err` only for network /
/// authentication / cancellation failures.
///
/// The HTTP future races [`wait_for_cancel`]. If the UI flips the
/// shared cancel flag mid-call, `tokio::select!` drops the request
/// future, the reqwest connection is closed, and we return the
/// dedicated `"cancelled"` error so the UI can branch on it.
#[tauri::command]
pub async fn ai_extract_chunk(
    state: State<'_, AppState>,
    cancel: State<'_, AiExtractCancel>,
    input: AiExtractInput,
    chunk_text: String,
) -> Result<AiChunkResponse, String> {
    if chunk_text.is_empty() {
        return Err("chunk text is empty".into());
    }
    if input.model.trim().is_empty() {
        return Err("AI settings: model is empty".into());
    }
    if input.base_url.trim().is_empty() {
        return Err("AI settings: base URL is empty".into());
    }

    let http = http_client(&state).await?;
    let cancel_inner = cancel.inner().clone();

    let raw = tokio::select! {
        res = async {
            match input.provider.as_str() {
                "anthropic" => call_anthropic(http, &input, &chunk_text).await,
                "openai-compatible" => call_openai_compatible(http, &input, &chunk_text, &[]).await,
                "openrouter" => {
                    // OpenRouter speaks OpenAI dialect; the two extra headers are
                    // optional but power their public-leaderboard attribution.
                    let extra = [
                        ("HTTP-Referer", "https://argos.thothlab.tech"),
                        ("X-Title", "Argos"),
                    ];
                    call_openai_compatible(http, &input, &chunk_text, &extra).await
                }
                "ollama" => call_ollama(http, &input, &chunk_text).await,
                other => Err(format!("unknown AI provider `{other}`")),
            }
        } => res?,
        _ = wait_for_cancel(&cancel_inner) => {
            return Err("cancelled".into());
        }
    };

    // Bad JSON in one chunk shouldn't poison the whole run: return an
    // empty `requests` list and let the UI surface the raw output.
    let requests = serde_json::from_str::<ExtractEnvelope>(&raw)
        .map(|env| env.requests)
        .unwrap_or_default();

    Ok(AiChunkResponse { requests, raw })
}

/// Flip the shared cancel flag. The in-flight chunk's `tokio::select!`
/// loses the race within `CANCEL_POLL_MS` and returns the `"cancelled"`
/// error; the UI's loop sees that, stops scheduling new chunks, and
/// transitions to the review phase with whatever it has so far.
#[tauri::command]
pub fn ai_extract_cancel(cancel: State<'_, AiExtractCancel>) {
    cancel.store(true, Ordering::SeqCst);
}

/// Poll the shared cancel flag at [`CANCEL_POLL_MS`] resolution. Used
/// inside `tokio::select!` to race the provider HTTP future. A tighter
/// signal (`Notify`) would be more elegant but the poll latency is
/// invisible to humans, so we keep the dependency surface small.
async fn wait_for_cancel(cancel: &AiExtractCancel) {
    loop {
        if cancel.load(Ordering::SeqCst) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(CANCEL_POLL_MS)).await;
    }
}

// ---- provider adapters --------------------------------------------------

async fn call_anthropic(
    http: &argos_core::HttpClient,
    input: &AiExtractInput,
    chunk: &str,
) -> Result<String, String> {
    if input.api_key.trim().is_empty() {
        return Err("Anthropic: API key is empty".into());
    }
    // Prefill the assistant turn with `{` so the model is forced to
    // start its output with JSON instead of any "Here you go:" prose.
    // We re-prepend the `{` to the response before parsing.
    let body = serde_json::json!({
        "model": input.model,
        "max_tokens": 4096,
        "system": build_system_prompt(),
        "messages": [
            { "role": "user",      "content": format!("{USER_PROMPT_PREFIX}{}", chunk) },
            { "role": "assistant", "content": "{" }
        ]
    });

    let req = HttpRequest {
        method: HttpMethod::Post,
        url: format!("{}/v1/messages", input.base_url.trim_end_matches('/')),
        headers: vec![
            HttpHeader::new("x-api-key", &input.api_key),
            HttpHeader::new("anthropic-version", "2023-06-01"),
            HttpHeader::new("content-type", "application/json"),
        ],
        query: vec![],
        body: Some(HttpBody::Json { value: body }),
        timeout: Some(Duration::from_secs(60)),
    };

    let resp = http.execute(&req).await.map_err(|e| e.to_string())?;
    let text = resp
        .body
        .as_str()
        .ok_or_else(|| "Anthropic returned non-UTF8 body".to_string())?
        .to_string();
    if !(200..300).contains(&resp.status) {
        return Err(format!("Anthropic HTTP {}: {}", resp.status, truncate(&text, 400)));
    }
    let val: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("Anthropic returned non-JSON ({e}): {}", truncate(&text, 400)))?;
    let mut out = String::from("{");
    if let Some(arr) = val.get("content").and_then(|v| v.as_array()) {
        for block in arr {
            if block.get("type").and_then(|v| v.as_str()) == Some("text") {
                if let Some(s) = block.get("text").and_then(|v| v.as_str()) {
                    out.push_str(s);
                }
            }
        }
    } else {
        return Err(format!(
            "Anthropic response missing `content` array: {}",
            truncate(&text, 400)
        ));
    }
    Ok(out)
}

async fn call_openai_compatible(
    http: &argos_core::HttpClient,
    input: &AiExtractInput,
    chunk: &str,
    extra_headers: &[(&str, &str)],
) -> Result<String, String> {
    if input.api_key.trim().is_empty() {
        return Err("OpenAI-compatible: API key is empty".into());
    }
    let body = serde_json::json!({
        "model": input.model,
        "messages": [
            { "role": "system", "content": build_system_prompt() },
            { "role": "user",   "content": format!("{USER_PROMPT_PREFIX}{}", chunk) }
        ],
        "response_format": { "type": "json_object" },
        "temperature": 0.0
    });
    let mut headers = vec![
        HttpHeader::new("authorization", &format!("Bearer {}", input.api_key)),
        HttpHeader::new("content-type", "application/json"),
    ];
    for (k, v) in extra_headers {
        headers.push(HttpHeader::new(*k, *v));
    }
    let req = HttpRequest {
        method: HttpMethod::Post,
        url: format!("{}/chat/completions", input.base_url.trim_end_matches('/')),
        headers,
        query: vec![],
        body: Some(HttpBody::Json { value: body }),
        timeout: Some(Duration::from_secs(60)),
    };
    let resp = http.execute(&req).await.map_err(|e| e.to_string())?;
    let text = resp
        .body
        .as_str()
        .ok_or_else(|| "OpenAI: non-UTF8 body".to_string())?
        .to_string();
    if !(200..300).contains(&resp.status) {
        return Err(format!("OpenAI HTTP {}: {}", resp.status, truncate(&text, 400)));
    }
    let val: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("OpenAI returned non-JSON ({e}): {}", truncate(&text, 400)))?;
    let content = val
        .get("choices")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .ok_or_else(|| {
            format!(
                "OpenAI response missing choices[0].message.content: {}",
                truncate(&text, 400)
            )
        })?;
    Ok(content.to_string())
}

/// Build the JSON body sent to Ollama. Extracted so the test below
/// can assert `num_ctx` is present without spinning up a fake HTTP
/// server. **Do not inline this back into `call_ollama`** — the test
/// is the only thing standing between a refactor and the Mac mini
/// rebooting again.
fn build_ollama_body(model: &str, chunk: &str) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "messages": [
            { "role": "system", "content": build_system_prompt() },
            { "role": "user",   "content": format!("{USER_PROMPT_PREFIX}{}", chunk) }
        ],
        "format": "json",
        "stream": false,
        "options": {
            "temperature": 0.0,
            "num_ctx": OLLAMA_NUM_CTX,
        }
    })
}

async fn call_ollama(
    http: &argos_core::HttpClient,
    input: &AiExtractInput,
    chunk: &str,
) -> Result<String, String> {
    // `num_ctx` is non-optional here — see [[build_ollama_body]] doc.
    let body = build_ollama_body(&input.model, chunk);
    let mut headers = vec![HttpHeader::new("content-type", "application/json")];
    // Ollama doesn't need an API key by default; only forward one if
    // the user happens to be behind a gateway that wants it.
    if !input.api_key.trim().is_empty() {
        headers.push(HttpHeader::new(
            "authorization",
            &format!("Bearer {}", input.api_key),
        ));
    }
    let req = HttpRequest {
        method: HttpMethod::Post,
        url: format!("{}/api/chat", input.base_url.trim_end_matches('/')),
        headers,
        query: vec![],
        body: Some(HttpBody::Json { value: body }),
        // Per-chunk timeout. The user can also Cancel mid-flight and
        // we'll abort the future immediately via `tokio::select!`.
        timeout: Some(Duration::from_secs(120)),
    };
    let resp = http.execute(&req).await.map_err(|e| e.to_string())?;
    let text = resp
        .body
        .as_str()
        .ok_or_else(|| "Ollama: non-UTF8 body".to_string())?
        .to_string();
    if !(200..300).contains(&resp.status) {
        return Err(format!("Ollama HTTP {}: {}", resp.status, truncate(&text, 400)));
    }
    let val: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("Ollama returned non-JSON ({e}): {}", truncate(&text, 400)))?;
    let content = val
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .ok_or_else(|| {
            format!(
                "Ollama response missing message.content: {}",
                truncate(&text, 400)
            )
        })?;
    Ok(content.to_string())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…[+{} bytes]", &s[..max], s.len() - max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_log_yields_one_chunk() {
        let log = "line 1\nline 2\n";
        let chunks = split_into_chunks(log);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], log);
    }

    #[test]
    fn empty_log_yields_no_chunks() {
        assert!(split_into_chunks("").is_empty());
    }

    #[test]
    fn large_log_splits_and_advances() {
        let line = "x".repeat(100) + "\n";
        let log: String = line.repeat(300); // ~30 KB
        let chunks = split_into_chunks(&log);
        assert!(chunks.len() >= 3, "expected multi-chunk, got {}", chunks.len());
        for c in &chunks {
            assert!(
                c.len() <= TARGET_CHUNK_BYTES + line.len(),
                "chunk too big: {}",
                c.len()
            );
        }
    }

    #[test]
    fn oversized_single_line_emitted_alone() {
        let huge = "y".repeat(TARGET_CHUNK_BYTES * 2);
        let chunks = split_into_chunks(&huge);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), huge.len());
    }

    #[test]
    fn ollama_body_carries_num_ctx_and_zero_temp() {
        // The whole crash-fix for the "Ollama freezes the Mac" bug
        // depends on these two options reaching the Ollama daemon.
        // A typo or accidental removal here re-introduces the
        // WindowServer watchdog reboot — this test is the lock.
        let body = build_ollama_body("llama3.1", "GET https://x.com/a");
        assert_eq!(body["options"]["num_ctx"], 8192);
        assert_eq!(body["options"]["temperature"], 0.0);
        assert_eq!(body["format"], "json");
        assert_eq!(body["stream"], false);
    }
}
