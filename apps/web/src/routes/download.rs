//! `GET /download/{target}` → 302 to the configured download base.
//!
//! `target` ∈ `macos-aarch64 | macos-x64 | linux-x64 | windows-x64`.
//! Anything else returns 400.

use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};

use crate::AppState;

pub async fn redirect(
    State(state): State<AppState>,
    Path(target): Path<String>,
) -> Response {
    let Some(asset) = asset_for_target(&target) else {
        return (
            StatusCode::BAD_REQUEST,
            format!(
                "unknown target `{target}`. Expected: macos-aarch64, macos-x64, linux-x64, windows-x64."
            ),
        )
            .into_response();
    };
    let url = format!("{}/{}", state.downloads_base.trim_end_matches('/'), asset);
    (StatusCode::FOUND, [(header::LOCATION, url)]).into_response()
}

/// Map a target to the canonical filename in the releases bucket.
/// Centralised so the landing HTML and Tauri updater agree on naming.
pub fn asset_for_target(target: &str) -> Option<&'static str> {
    match target {
        "macos-aarch64" | "darwin-aarch64" => Some("argos-macos-aarch64.dmg"),
        "macos-x64" | "darwin-x86_64" => Some("argos-macos-x64.dmg"),
        "linux-x64" | "linux-x86_64" => Some("argos-linux-x64.AppImage"),
        "windows-x64" | "windows-x86_64" => Some("argos-windows-x64-setup.exe"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::test_harness;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt as _;

    #[tokio::test]
    async fn redirects_to_configured_base() {
        let h = test_harness::make().await;
        let res = h
            .router
            .oneshot(
                Request::builder()
                    .uri("/download/macos-aarch64")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::FOUND);
        let loc = res.headers().get("location").unwrap().to_str().unwrap();
        assert_eq!(
            loc,
            "https://example.test/releases/latest/download/argos-macos-aarch64.dmg"
        );
    }

    #[tokio::test]
    async fn unknown_target_returns_400() {
        let h = test_harness::make().await;
        let res = h
            .router
            .oneshot(
                Request::builder()
                    .uri("/download/bsd")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }
}
