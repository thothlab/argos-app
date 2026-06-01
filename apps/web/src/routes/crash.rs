//! `POST /api/crash` — accept and persist a crash report.
//!
//! Validation:
//!   - JSON body, must include `schema: "argos.crash.v1"` and the
//!     `panic.location` + `panic.message` fields.
//!   - Total body ≤ 64 KB.
//!
//! Sanitisation (T9.2.3):
//!   - Every panic field runs through [`crate::sanitize::redact`]
//!     before hashing or hitting disk. Home paths, Bearer / Basic
//!     auth headers, common secret query params, and JWTs are
//!     masked. The on-disk file is the sanitised JSON — the raw
//!     body never lands on disk.
//!
//! Storage:
//!   - `data_dir/crashes/YYYY-MM-DD/<sha256(location+'\n'+message)>.json`
//!     where `location` and `message` are the *sanitised* values, so
//!     two users hitting the same panic dedupe even if their pre-
//!     redaction paths differed by username.
//!   - First write stores the sanitised report. Subsequent writes for
//!     the same hash bump a sidecar `<hash>.count` file (atomic-ish
//!     increment via read → +1 → write).
//!
//! Rate limit: 10/min/IP (peer address from `ConnectInfo`).

use std::net::SocketAddr;

use axum::{
    body::Bytes,
    extract::{ConnectInfo, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::AppState;

const MAX_BODY: usize = 64 * 1024;

// Schema is `serde(deny_unknown_fields = false)` by default — we
// accept clients that ship extra fields (e.g. future schema versions)
// and only validate the ones we read. The struct is round-tripped:
// parse → sanitise panic fields → serialise back to disk. Anything
// not in the struct (forward-compat future fields) is dropped on
// write, which is the right behaviour for a closed-alpha collector.
#[derive(Debug, Deserialize, Serialize)]
pub struct CrashReport {
    pub schema: String,
    pub app_version: String,
    pub os: String,
    pub ts: String,
    pub panic: PanicInfo,
    /// Anonymous session id — generated client-side, persisted in
    /// `~/.argos/session_id`. Optional; older clients may omit it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PanicInfo {
    pub message: String,
    pub location: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backtrace: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AcceptResponse {
    pub deduped: bool,
    pub stored_as: String,
}

pub async fn submit(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<AppState>,
    body: Bytes,
) -> Response {
    if !state.rate_limiter.check(addr.ip()) {
        return (StatusCode::TOO_MANY_REQUESTS, "rate limit").into_response();
    }
    if body.len() > MAX_BODY {
        return (StatusCode::PAYLOAD_TOO_LARGE, "max 64 KB").into_response();
    }
    let mut report: CrashReport = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, format!("invalid JSON: {e}"))
                .into_response();
        }
    };
    if report.schema != "argos.crash.v1" {
        return (
            StatusCode::BAD_REQUEST,
            format!(
                "unsupported schema `{}` — expected `argos.crash.v1`",
                report.schema
            ),
        )
            .into_response();
    }
    if report.panic.message.is_empty() || report.panic.location.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            "panic.message and panic.location must be non-empty",
        )
            .into_response();
    }

    // Scrub PII *before* hashing so two users hitting the same panic
    // from different home directories dedupe to one file.
    report.panic.message = crate::sanitize::redact(&report.panic.message);
    report.panic.location = crate::sanitize::redact(&report.panic.location);
    if let Some(bt) = report.panic.backtrace.as_ref() {
        report.panic.backtrace = Some(crate::sanitize::redact(bt));
    }

    let hash = hash_panic(&report.panic.location, &report.panic.message);
    let day = day_bucket(&report.ts);
    let day_dir = state.data_dir.join("crashes").join(&day);
    if let Err(e) = tokio::fs::create_dir_all(&day_dir).await {
        tracing::error!(error = %e, "create_dir_all crashes/<day> failed");
        return (StatusCode::INTERNAL_SERVER_ERROR, "storage unavailable")
            .into_response();
    }

    let report_path = day_dir.join(format!("{hash}.json"));
    let count_path = day_dir.join(format!("{hash}.count"));

    let already_existed = tokio::fs::try_exists(&report_path).await.unwrap_or(false);

    if !already_existed {
        let sanitised = match serde_json::to_vec_pretty(&report) {
            Ok(b) => b,
            Err(e) => {
                tracing::error!(error = %e, "reserialise sanitised report failed");
                return (StatusCode::INTERNAL_SERVER_ERROR, "storage unavailable")
                    .into_response();
            }
        };
        if let Err(e) = tokio::fs::write(&report_path, &sanitised).await {
            tracing::error!(error = %e, "write crash report failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, "storage unavailable")
                .into_response();
        }
    }
    // Increment the count (atomic-ish for a single-process service; we
    // don't shard).
    let prev: u64 = tokio::fs::read_to_string(&count_path)
        .await
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    let _ = tokio::fs::write(&count_path, (prev + 1).to_string()).await;

    (
        StatusCode::ACCEPTED,
        Json(AcceptResponse {
            deduped: already_existed,
            stored_as: hash,
        }),
    )
        .into_response()
}

fn hash_panic(location: &str, message: &str) -> String {
    let mut h = Sha256::new();
    h.update(location.as_bytes());
    h.update(b"\n");
    h.update(message.as_bytes());
    hex::encode(h.finalize())
}

/// Pull `YYYY-MM-DD` out of an RFC 3339 timestamp. Best-effort — if
/// the string doesn't start with a date we fall back to "unknown" so
/// the file at least gets stored.
fn day_bucket(ts: &str) -> String {
    if ts.len() >= 10 && ts.as_bytes()[4] == b'-' && ts.as_bytes()[7] == b'-' {
        ts[..10].to_string()
    } else {
        "unknown".to_string()
    }
}

#[cfg(test)]
mod tests {
    use crate::test_harness;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use std::net::SocketAddr;
    use tower::ServiceExt as _;

    fn sample_body(message: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "schema": "argos.crash.v1",
            "app_version": "0.1.0",
            "os": "macos 14.6.1 arm64",
            "ts": "2026-05-11T10:30:00Z",
            "panic": {
                "message": message,
                "location": "crates/core/src/format/request.rs:412"
            }
        }))
        .unwrap()
    }

    async fn post(h: &test_harness::Harness, body: Vec<u8>) -> axum::http::Response<Body> {
        let req = Request::builder()
            .method("POST")
            .uri("/api/crash")
            .header("content-type", "application/json")
            .extension(axum::extract::ConnectInfo(
                "127.0.0.1:1234".parse::<SocketAddr>().unwrap(),
            ))
            .body(Body::from(body))
            .unwrap();
        h.router.clone().oneshot(req).await.unwrap()
    }

    #[tokio::test]
    async fn first_submit_is_accepted_and_dedup_flag_is_false() {
        let h = test_harness::make().await;
        let res = post(&h, sample_body("boom")).await;
        assert_eq!(res.status(), StatusCode::ACCEPTED);
        let body = axum::body::to_bytes(res.into_body(), 4096).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["deduped"], false);
        assert!(v["stored_as"].as_str().unwrap().len() == 64);
    }

    #[tokio::test]
    async fn second_identical_submit_dedupes() {
        let h = test_harness::make().await;
        post(&h, sample_body("boom")).await;
        let res = post(&h, sample_body("boom")).await;
        assert_eq!(res.status(), StatusCode::ACCEPTED);
        let body = axum::body::to_bytes(res.into_body(), 4096).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["deduped"], true);
        // count file shows 2.
        let count_path = h
            .state
            .data_dir
            .join("crashes")
            .join("2026-05-11")
            .join(format!("{}.count", v["stored_as"].as_str().unwrap()));
        let count = tokio::fs::read_to_string(&count_path).await.unwrap();
        assert_eq!(count.trim(), "2");
    }

    #[tokio::test]
    async fn different_panic_message_gets_different_hash() {
        let h = test_harness::make().await;
        let a = post(&h, sample_body("boom")).await;
        let b = post(&h, sample_body("kaboom")).await;
        let ba = axum::body::to_bytes(a.into_body(), 4096).await.unwrap();
        let bb = axum::body::to_bytes(b.into_body(), 4096).await.unwrap();
        let va: serde_json::Value = serde_json::from_slice(&ba).unwrap();
        let vb: serde_json::Value = serde_json::from_slice(&bb).unwrap();
        assert_ne!(va["stored_as"], vb["stored_as"]);
    }

    #[tokio::test]
    async fn bad_schema_is_rejected() {
        let h = test_harness::make().await;
        let mut body: serde_json::Value =
            serde_json::from_slice(&sample_body("boom")).unwrap();
        body["schema"] = serde_json::json!("not-a-known-schema");
        let res = post(&h, serde_json::to_vec(&body).unwrap()).await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn invalid_json_is_rejected() {
        let h = test_harness::make().await;
        let res = post(&h, b"not json".to_vec()).await;
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn pii_in_panic_message_is_redacted_on_disk() {
        let h = test_harness::make().await;
        let body = serde_json::to_vec(&serde_json::json!({
            "schema": "argos.crash.v1",
            "app_version": "0.1.4",
            "os": "macos aarch64",
            "ts": "2026-06-01T08:00:00Z",
            "panic": {
                "message": "request failed at /Users/alice/proj/x.rs with Authorization: Bearer ABCDEF123 and api_key=topSecret",
                "location": "/Users/alice/.cargo/registry/src/foo-1.0/src/lib.rs:99",
            }
        }))
        .unwrap();
        let res = post(&h, body).await;
        assert_eq!(res.status(), StatusCode::ACCEPTED);
        let body_bytes = axum::body::to_bytes(res.into_body(), 4096).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let hash = v["stored_as"].as_str().unwrap();
        let stored = tokio::fs::read_to_string(
            h.state
                .data_dir
                .join("crashes")
                .join("2026-06-01")
                .join(format!("{hash}.json")),
        )
        .await
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&stored).unwrap();
        let msg = parsed["panic"]["message"].as_str().unwrap();
        let loc = parsed["panic"]["location"].as_str().unwrap();
        assert!(!msg.contains("alice"));
        assert!(!msg.contains("Bearer ABCDEF123"));
        assert!(!msg.contains("topSecret"));
        assert!(msg.contains("Bearer <redacted>"));
        assert!(msg.contains("api_key=<redacted>"));
        assert!(!loc.contains("alice"));
        assert!(loc.starts_with("/Users/<user>/.cargo"));
    }

    #[tokio::test]
    async fn dedup_works_across_different_usernames() {
        let h = test_harness::make().await;
        let make_body = |user: &str| {
            serde_json::to_vec(&serde_json::json!({
                "schema": "argos.crash.v1",
                "app_version": "0.1.4",
                "os": "macos aarch64",
                "ts": "2026-06-01T08:00:00Z",
                "panic": {
                    "message": format!("io error at /Users/{user}/cfg.yaml"),
                    "location": "crates/core/src/format/mod.rs:120",
                }
            }))
            .unwrap()
        };
        let a = post(&h, make_body("alice")).await;
        let b = post(&h, make_body("bob")).await;
        let va: serde_json::Value =
            serde_json::from_slice(&axum::body::to_bytes(a.into_body(), 4096).await.unwrap())
                .unwrap();
        let vb: serde_json::Value =
            serde_json::from_slice(&axum::body::to_bytes(b.into_body(), 4096).await.unwrap())
                .unwrap();
        // Same hash → same file → second submit deduped.
        assert_eq!(va["stored_as"], vb["stored_as"]);
        assert_eq!(va["deduped"], false);
        assert_eq!(vb["deduped"], true);
    }

    #[tokio::test]
    async fn oversize_body_is_rejected() {
        let h = test_harness::make().await;
        let huge = "x".repeat(65 * 1024);
        let body = serde_json::to_vec(&serde_json::json!({
            "schema": "argos.crash.v1",
            "app_version": "0.1.0",
            "os": "x",
            "ts": "2026-05-11T10:30:00Z",
            "panic": { "message": huge, "location": "x" }
        }))
        .unwrap();
        let res = post(&h, body).await;
        assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
