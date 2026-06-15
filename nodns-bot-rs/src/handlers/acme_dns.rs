use std::sync::Arc;

use axum::extract::State as AxumState;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Json, Response};
use base64::Engine;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

use crate::AppState;

// ---------------------------------------------------------------------------
// acme-dns compatible handlers
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct AcmeDnsRegisterResponse {
    allowfrom: Vec<String>,
    fulldomain: String,
    password: String,
    subdomain: String,
    username: String,
}

pub async fn acmedns_register_handler(
    AxumState(state): AxumState<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    let npub = headers
        .get("X-Nostr-Npub")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "anonymous".to_string());

    let zone = match state.dns_zones.first() {
        Some(zc) => zc.zone.clone(),
        None => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "no DNS zones configured"})),
            )
                .into_response();
        }
    };

    let subdomain = uuid::Uuid::new_v4().to_string();
    let username = uuid::Uuid::new_v4().to_string();

    let password_bytes: [u8; 32] = rand_bytes();
    let password = base64::engine::general_purpose::STANDARD_NO_PAD.encode(password_bytes);

    let fulldomain = format!("{}.acme.{}", subdomain, zone);

    if let Err(e) = state.store.save_acme_dns_registration(
        &subdomain,
        &username,
        &password,
        &npub,
        &zone,
        &fulldomain,
    ) {
        error!(error = %e, "acme-dns register: failed to save registration");
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Internal error"})),
        )
            .into_response();
    }

    info!(
        subdomain = %subdomain,
        fulldomain = %fulldomain,
        npub = %npub,
        "acme-dns registration created"
    );

    (
        axum::http::StatusCode::CREATED,
        Json(AcmeDnsRegisterResponse {
            allowfrom: vec![],
            fulldomain,
            password,
            subdomain,
            username,
        }),
    )
        .into_response()
}

#[derive(Deserialize)]
pub struct AcmeDnsUpdateRequest {
    subdomain: String,
    txt: String,
}

#[derive(Serialize)]
pub struct AcmeDnsUpdateResponse {
    txt: String,
}

pub async fn acmedns_update_handler(
    AxumState(state): AxumState<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<AcmeDnsUpdateRequest>,
) -> Response {
    let api_user = match headers.get("X-Api-User").and_then(|v| v.to_str().ok()) {
        Some(u) => u.to_string(),
        None => {
            return (
                axum::http::StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "X-Api-User header required"})),
            )
                .into_response();
        }
    };

    let api_key = match headers.get("X-Api-Key").and_then(|v| v.to_str().ok()) {
        Some(k) => k.to_string(),
        None => {
            return (
                axum::http::StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "X-Api-Key header required"})),
            )
                .into_response();
        }
    };

    let registration = match state.store.get_acme_dns_registration_by_username(&api_user) {
        Ok(Some(r)) => r,
        Ok(None) => {
            return (
                axum::http::StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "invalid credentials"})),
            )
                .into_response();
        }
        Err(e) => {
            error!(error = %e, "acme-dns update: failed to look up registration");
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Internal error"})),
            )
                .into_response();
        }
    };

    if registration.password != api_key {
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "invalid credentials"})),
        )
            .into_response();
    }

    if registration.subdomain != body.subdomain {
        return (
            axum::http::StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "subdomain mismatch"})),
        )
            .into_response();
    }

    if let Err(e) = state.store.update_acme_dns_txt(&body.subdomain, &body.txt) {
        error!(error = %e, "acme-dns update: failed to update TXT in store");
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Internal error"})),
        )
            .into_response();
    }

    let Some(updater) = state.updaters.get(&registration.zone) else {
        error!(zone = %registration.zone, "acme-dns update: no updater for zone");
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Internal error"})),
        )
            .into_response();
    };

    let fqdn = format!("{}.", registration.fulldomain);
    let ttl = 120;

    if let Err(e) = updater.delete_record(&fqdn, 16).await {
        error!(error = %e, fqdn = %fqdn, "acme-dns update: failed to delete existing TXT RRset");
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "DDNS update failed"})),
        )
            .into_response();
    }

    if let Err(e) = updater.append_record(&fqdn, ttl, 16, &body.txt).await {
        error!(error = %e, fqdn = %fqdn, "acme-dns update: failed to append current TXT");
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "DDNS update failed"})),
        )
            .into_response();
    }

    let refreshed = state.store.get_acme_dns_registration_by_username(&api_user);
    if let Ok(Some(ref reg)) = refreshed {
        if let Some(ref prev) = reg.txt_value_prev {
            if !prev.is_empty() {
                if let Err(e) = updater.append_record(&fqdn, ttl, 16, prev).await {
                    warn!(error = %e, "acme-dns update: failed to append previous TXT (non-fatal)");
                }
            }
        }
    }

    info!(
        subdomain = %body.subdomain,
        txt = %body.txt,
        "acme-dns TXT updated"
    );

    Json(AcmeDnsUpdateResponse { txt: body.txt }).into_response()
}

fn rand_bytes() -> [u8; 32] {
    use aes_gcm::aead::OsRng;
    use nostr_sdk::secp256k1::rand::RngCore;
    let mut buf = [0u8; 32];
    OsRng.fill_bytes(&mut buf);
    buf
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::handlers::test_helpers::*;

    use axum::body;
    use axum::Router;
    use http::Request;
    use tower::ServiceExt;

    // =======================================================================
    // acme-dns register integration tests
    // =======================================================================

    #[tokio::test]
    async fn acmedns_register_returns_201_with_fields() {
        let state = create_test_state();
        let app = build_router(state);

        let request = Request::builder()
            .method("POST")
            .uri("/register")
            .body(body::Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 201);

        let json = response_json(response).await;
        assert!(json["fulldomain"].is_string());
        assert!(json["username"].is_string());
        assert!(json["password"].is_string());
        assert!(json["subdomain"].is_string());
        assert!(json["allowfrom"].is_array());
    }

    #[tokio::test]
    async fn acmedns_register_fulldomain_format() {
        let state = create_test_state();
        let app = build_router(state);

        let request = Request::builder()
            .method("POST")
            .uri("/register")
            .body(body::Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        let json = response_json(response).await;

        let fulldomain = json["fulldomain"].as_str().unwrap();
        assert!(
            fulldomain.ends_with(".acme.nodns.shop"),
            "fulldomain should end with .acme.nodns.shop, got: {fulldomain}"
        );
    }

    #[tokio::test]
    async fn acmedns_register_subdomain_is_uuid() {
        let state = create_test_state();
        let app = build_router(state);

        let request = Request::builder()
            .method("POST")
            .uri("/register")
            .body(body::Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        let json = response_json(response).await;

        let subdomain = json["subdomain"].as_str().unwrap();
        assert!(
            uuid::Uuid::parse_str(subdomain).is_ok(),
            "subdomain should be a valid UUID, got: {subdomain}"
        );
    }

    #[tokio::test]
    async fn acmedns_register_with_npub_header_stores_npub() {
        let state = create_test_state();
        let app = build_router(state.clone());

        let request = Request::builder()
            .method("POST")
            .uri("/register")
            .header("X-Nostr-Npub", "npub1test")
            .body(body::Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 201);

        let json = response_json(response).await;
        let subdomain = json["subdomain"].as_str().unwrap();

        let reg = state
            .store
            .get_acme_dns_registration_by_username(json["username"].as_str().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(reg.npub, "npub1test");
        assert_eq!(reg.subdomain, subdomain);
    }

    #[tokio::test]
    async fn acmedns_register_twice_produces_different_subdomains() {
        let state = create_test_state();
        let app = build_router(state);

        let req1 = Request::builder()
            .method("POST")
            .uri("/register")
            .body(body::Body::empty())
            .unwrap();
        let resp1 = app.clone().oneshot(req1).await.unwrap();
        let json1 = response_json(resp1).await;

        let req2 = Request::builder()
            .method("POST")
            .uri("/register")
            .body(body::Body::empty())
            .unwrap();
        let resp2 = app.oneshot(req2).await.unwrap();
        let json2 = response_json(resp2).await;

        assert_ne!(
            json1["subdomain"].as_str().unwrap(),
            json2["subdomain"].as_str().unwrap(),
            "two registrations must produce different subdomains"
        );
    }

    // =======================================================================
    // acme-dns update integration tests
    // =======================================================================

    #[tokio::test]
    async fn acmedns_update_correct_credentials_dns_fails() {
        // Register, then update with correct credentials.
        // DNS push fails (updater points at 127.0.0.1:1) → 500.
        let state = create_test_state_with_updater();
        let app: Router = build_router(state);

        let (subdomain, username, password) = register_acmedns(&app).await;

        let body = serde_json::json!({
            "subdomain": subdomain,
            "txt": "test-challenge-token"
        });

        let request = Request::builder()
            .method("POST")
            .uri("/update")
            .header("X-Api-User", &username)
            .header("X-Api-Key", &password)
            .header("content-type", "application/json")
            .body(body::Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        // DNS delete fails → 500
        assert_eq!(response.status(), 500);
    }

    #[tokio::test]
    async fn acmedns_update_wrong_api_key_returns_401() {
        let state = create_test_state();
        let app = build_router(state);

        let (subdomain, username, _password) = register_acmedns(&app).await;

        let body = serde_json::json!({
            "subdomain": subdomain,
            "txt": "test-challenge"
        });

        let request = Request::builder()
            .method("POST")
            .uri("/update")
            .header("X-Api-User", &username)
            .header("X-Api-Key", "wrong-key")
            .header("content-type", "application/json")
            .body(body::Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 401);
    }

    #[tokio::test]
    async fn acmedns_update_missing_api_user_returns_401() {
        let state = create_test_state();
        let app = build_router(state);

        let body = serde_json::json!({
            "subdomain": "irrelevant",
            "txt": "test"
        });

        let request = Request::builder()
            .method("POST")
            .uri("/update")
            .header("X-Api-Key", "some-key")
            .header("content-type", "application/json")
            .body(body::Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 401);
    }

    #[tokio::test]
    async fn acmedns_update_missing_api_key_returns_401() {
        let state = create_test_state();
        let app = build_router(state);

        let body = serde_json::json!({
            "subdomain": "irrelevant",
            "txt": "test"
        });

        let request = Request::builder()
            .method("POST")
            .uri("/update")
            .header("X-Api-User", "some-user")
            .header("content-type", "application/json")
            .body(body::Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 401);
    }

    #[tokio::test]
    async fn acmedns_update_nonexistent_username_returns_401() {
        let state = create_test_state();
        let app = build_router(state);

        let body = serde_json::json!({
            "subdomain": "irrelevant",
            "txt": "test"
        });

        let request = Request::builder()
            .method("POST")
            .uri("/update")
            .header("X-Api-User", "nonexistent-user-id")
            .header("X-Api-Key", "some-key")
            .header("content-type", "application/json")
            .body(body::Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 401);
    }

    #[tokio::test]
    async fn acmedns_update_wrong_subdomain_returns_403() {
        let state = create_test_state();
        let app = build_router(state);

        let (_subdomain, username, password) = register_acmedns(&app).await;

        let body = serde_json::json!({
            "subdomain": "wrong-subdomain-uuid",
            "txt": "test-challenge"
        });

        let request = Request::builder()
            .method("POST")
            .uri("/update")
            .header("X-Api-User", &username)
            .header("X-Api-Key", &password)
            .header("content-type", "application/json")
            .body(body::Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 403);
    }
}
