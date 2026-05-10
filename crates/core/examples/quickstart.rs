//! Quick smoke test for the HTTP engine + curl codegen.
//!
//! Run with:
//!   cargo run --example quickstart -p argos-core
//!
//! Uses httpbin.org as the live target. Falls back gracefully if offline.

use argos_core::codegen::curl;
use argos_core::{HttpBody, HttpClient, HttpHeader, HttpMethod, HttpRequest};
use serde_json::json;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    println!(
        "=== argos-core v{} — HTTP engine smoke test ===\n",
        argos_core::VERSION
    );

    // ---- 1. Live GET with query + header ----
    let client = HttpClient::new()?;

    let req = HttpRequest {
        method: HttpMethod::Get,
        url: "https://httpbin.org/get".into(),
        query: vec![
            ("page".into(), "2".into()),
            ("limit".into(), "50".into()),
            ("q".into(), "hello world".into()),
        ],
        headers: vec![
            HttpHeader::new("Accept", "application/json"),
            HttpHeader::new("X-Argos-Demo", "true"),
        ],
        ..Default::default()
    };

    println!("--- 1. Live GET httpbin.org/get -----------------\n");
    print_curl(&req);

    println!("\nSending live request...\n");
    match client.execute(&req).await {
        Ok(resp) => {
            println!("Status:       {} {}", resp.status, resp.status_text);
            println!("Final URL:    {}", resp.final_url);
            println!("Total:        {} ms", resp.timing.total_ms);
            println!("TTFB:         {} ms", resp.timing.ttfb_ms.unwrap_or(0));
            println!("Download:     {} ms", resp.timing.download_ms.unwrap_or(0));
            println!("Size:         {} bytes", resp.body.size_bytes);
            println!(
                "Content-Type: {}",
                resp.body.content_type.as_deref().unwrap_or("?")
            );
            println!("Headers:      {} entries", resp.headers.len());
            if resp.is_success() {
                println!("✓ Success (2xx)");
            }

            // Pretty-print the JSON response if applicable.
            if let Ok(json) = resp.body.as_json() {
                let pretty = serde_json::to_string_pretty(&json)?;
                let snippet = pretty.lines().take(20).collect::<Vec<_>>().join("\n");
                println!("\nResponse body (first 20 lines):\n{snippet}");
            } else if let Some(text) = resp.body.as_str() {
                println!("\nResponse body:\n{text}");
            }
        }
        Err(e) => {
            println!("✗ Network error: {e}");
            println!(
                "  (This is fine if you're offline — the unit tests use a local mock server.)"
            );
        }
    }

    // ---- 2. POST with JSON — curl-only (no live send) ----
    println!("\n\n--- 2. POST with JSON body — curl preview only -----\n");
    let post_req = HttpRequest {
        method: HttpMethod::Post,
        url: "https://api.example.com/users".into(),
        headers: vec![HttpHeader::new("Authorization", "Bearer it's-secret")],
        body: Some(HttpBody::Json {
            value: json!({
                "name": "Alice",
                "role": "admin",
                "tags": ["dev", "ops"]
            }),
        }),
        ..Default::default()
    };
    print_curl(&post_req);

    // ---- 3. POST with form body ----
    println!("\n\n--- 3. POST x-www-form-urlencoded — curl preview ----\n");
    let form_req = HttpRequest {
        method: HttpMethod::Post,
        url: "https://api.example.com/login".into(),
        body: Some(HttpBody::FormUrlEncoded {
            fields: vec![
                ("user".into(), "alice".into()),
                ("pass".into(), "p@ss w0rd!".into()),
            ],
        }),
        ..Default::default()
    };
    print_curl(&form_req);

    println!("\n=== done ===");
    Ok(())
}

fn print_curl(req: &HttpRequest) {
    println!("{}", curl::to_curl(req));
}
