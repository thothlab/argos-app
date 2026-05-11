//! Landing page + healthcheck.

use axum::{
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};

/// Inline HTML — no JS, no external assets, < 4 KB. The CSS is in a
/// `<style>` block so the page loads even when offline mirrors archive
/// it. The four download buttons point at `/download/{target}` which
/// then 302s to the actual GitHub Releases asset (configurable via
/// `ARGOS_DOWNLOADS_BASE`).
const LANDING_HTML: &str = include_str!("../../static/landing.html");

pub async fn index() -> Response {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        LANDING_HTML,
    )
        .into_response()
}

pub async fn healthz() -> (StatusCode, &'static str) {
    (StatusCode::OK, "ok")
}

#[cfg(test)]
mod tests {
    use crate::test_harness;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt as _;

    #[tokio::test]
    async fn index_returns_html() {
        let h = test_harness::make().await;
        let res = h
            .router
            .clone()
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let ct = res
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(ct.starts_with("text/html"));
        let body = axum::body::to_bytes(res.into_body(), 64 * 1024)
            .await
            .unwrap();
        let body = std::str::from_utf8(&body).unwrap();
        assert!(body.contains("Argos"));
        assert!(body.contains("/download/"));
    }

    #[tokio::test]
    async fn healthz_returns_ok() {
        let h = test_harness::make().await;
        let res = h
            .router
            .oneshot(Request::builder().uri("/healthz").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }
}
