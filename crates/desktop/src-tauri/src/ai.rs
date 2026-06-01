//! BYOK AI calls — provider adapters for log-extraction (and any
//! future AI-backed feature). The Tauri command lives here; the UI
//! calls it via [`ai_extract_log`].
//!
//! Argos never proxies AI traffic — the user's key + the log payload
//! go from the desktop binary straight to the provider they picked.
//! See `apps/docs/importing.md` for the privacy story shown to users.

use argos_core::{HttpBody, HttpHeader, HttpMethod, HttpRequest};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tauri::State;

use crate::{http_client, AppState};

/// Cap on the log text we'll send to a provider. Anything larger
/// would (a) blow past most providers' context window, (b) cost the
/// user real money in tokens, and (c) take long enough that the user
/// will think the app froze. The UI shows live byte count next to
/// the textarea so users can trim before hitting Extract.
pub const MAX_LOG_BYTES: usize = 50 * 1024;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiExtractInput {
    pub provider: String,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub log_text: String,
}

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct NameValue {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExtractedBody {
    Json { value: serde_json::Value },
    Text { content: String },
    Form { fields: Vec<NameValue> },
}

#[derive(Debug, Serialize)]
pub struct AiExtractResponse {
    /// Requests the model emitted. Empty array is a valid response —
    /// the log just didn't contain anything that looked like a
    /// request.
    pub requests: Vec<ExtractedRequest>,
    /// Raw model output before parsing. Useful when extraction
    /// quality is bad — UI can show it on demand.
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

#[tauri::command]
pub async fn ai_extract_log(
    state: State<'_, AppState>,
    request: AiExtractInput,
) -> Result<AiExtractResponse, String> {
    if request.log_text.is_empty() {
        return Err("log text is empty".into());
    }
    if request.log_text.len() > MAX_LOG_BYTES {
        return Err(format!(
            "log is {} bytes; max is {} bytes — trim before extracting",
            request.log_text.len(),
            MAX_LOG_BYTES
        ));
    }
    if request.model.trim().is_empty() {
        return Err("AI settings: model is empty".into());
    }
    if request.base_url.trim().is_empty() {
        return Err("AI settings: base URL is empty".into());
    }

    let http = http_client(&state).await?;
    let raw = match request.provider.as_str() {
        "anthropic" => call_anthropic(http, &request).await?,
        "openai-compatible" => call_openai_compatible(http, &request, &[]).await?,
        "openrouter" => {
            // OpenRouter speaks OpenAI dialect; the two extra headers are
            // optional but power their public-leaderboard attribution. Free
            // marketing for Argos when users opt in via OpenRouter.
            let extra = [
                ("HTTP-Referer", "https://argos.thothlab.tech"),
                ("X-Title", "Argos"),
            ];
            call_openai_compatible(http, &request, &extra).await?
        }
        "ollama" => call_ollama(http, &request).await?,
        other => return Err(format!("unknown AI provider `{other}`")),
    };

    let parsed: ExtractEnvelope = serde_json::from_str(&raw).map_err(|e| {
        format!(
            "AI returned text that wasn't the expected JSON shape ({e}).\n\nRaw output:\n{raw}"
        )
    })?;

    Ok(AiExtractResponse {
        requests: parsed.requests,
        raw,
    })
}

// ---- provider adapters --------------------------------------------------

async fn call_anthropic(
    http: &argos_core::HttpClient,
    input: &AiExtractInput,
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
            { "role": "user",      "content": format!("{USER_PROMPT_PREFIX}{}", input.log_text) },
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
    // Concat all text blocks (typically one).
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
    extra_headers: &[(&str, &str)],
) -> Result<String, String> {
    if input.api_key.trim().is_empty() {
        return Err("OpenAI-compatible: API key is empty".into());
    }
    let body = serde_json::json!({
        "model": input.model,
        "messages": [
            { "role": "system", "content": build_system_prompt() },
            { "role": "user",   "content": format!("{USER_PROMPT_PREFIX}{}", input.log_text) }
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

async fn call_ollama(
    http: &argos_core::HttpClient,
    input: &AiExtractInput,
) -> Result<String, String> {
    // Ollama doesn't need an API key by default — leave the header
    // off entirely so it works against a fresh local install.
    let body = serde_json::json!({
        "model": input.model,
        "messages": [
            { "role": "system", "content": build_system_prompt() },
            { "role": "user",   "content": format!("{USER_PROMPT_PREFIX}{}", input.log_text) }
        ],
        "format": "json",
        "stream": false,
        "options": { "temperature": 0.0 }
    });
    let mut headers = vec![HttpHeader::new("content-type", "application/json")];
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
