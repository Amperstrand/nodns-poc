//! llms.txt and llms-full.txt endpoints (https://llmstxt.org).
//!
//! Provides machine-readable API documentation for AI agents discovering the
//! nodns HTTP API.

use axum::http::header;
use axum::response::{IntoResponse, Response};

/// `GET /llms.txt` — concise summary following the llms.txt convention.
pub async fn llms_txt_handler() -> Response {
    let body = include_str!("../llms/llms.txt");
    (
        axum::http::StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        body,
    )
        .into_response()
}

/// `GET /llms-full.txt` — full API reference with examples.
pub async fn llms_full_txt_handler() -> Response {
    let body = include_str!("../llms/llms-full.txt");
    (
        axum::http::StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        body,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llms_txt_is_non_empty_and_has_title() {
        let body = include_str!("../llms/llms.txt");
        assert!(!body.trim().is_empty());
        assert!(body.starts_with("# "), "llms.txt should start with a title");
        assert!(body.contains("nodns"));
    }

    #[test]
    fn llms_full_txt_documents_core_endpoints() {
        let body = include_str!("../llms/llms-full.txt");
        assert!(!body.trim().is_empty());
        for endpoint in ["/api/health", "/api/check", "/api/records", "/api/zones"] {
            assert!(
                body.contains(endpoint),
                "llms-full.txt should document {endpoint}"
            );
        }
        // Should describe the Nostr protocol (kind 11111).
        assert!(body.contains("11111"));
        // Should describe the Cashu payment flow.
        assert!(
            body.to_lowercase().contains("cashu"),
            "llms-full.txt should document Cashu payments"
        );
    }

    #[tokio::test]
    async fn llms_txt_handler_returns_text_plain() {
        let resp = llms_txt_handler().await;
        assert_eq!(resp.status(), axum::http::StatusCode::OK);
        let ct = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        assert!(
            ct.starts_with("text/plain"),
            "unexpected content-type: {ct}"
        );
    }

    #[tokio::test]
    async fn llms_full_txt_handler_returns_text_plain() {
        let resp = llms_full_txt_handler().await;
        assert_eq!(resp.status(), axum::http::StatusCode::OK);
        let ct = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        assert!(
            ct.starts_with("text/plain"),
            "unexpected content-type: {ct}"
        );
    }
}
