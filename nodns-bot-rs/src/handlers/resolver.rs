use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::extract::State as AxumState;
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use base64::Engine;
use serde::Serialize;
use serde_json::json;
use tracing::{info, warn};

use crate::payment::Verifier;
use crate::store::ResolverStats;
use crate::AppState;

#[derive(Serialize)]
struct Nut18PaymentRequest {
    #[serde(rename = "a", skip_serializing_if = "Option::is_none")]
    amount: Option<i64>,
    #[serde(rename = "u", skip_serializing_if = "Option::is_none")]
    unit: Option<String>,
    #[serde(rename = "m", skip_serializing_if = "Option::is_none")]
    mints: Option<Vec<String>>,
    #[serde(rename = "d", skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

fn encode_creq_a(req: &Nut18PaymentRequest) -> String {
    let mut cbor_bytes = Vec::new();
    if ciborium::ser::into_writer(req, &mut cbor_bytes).is_err() {
        return String::new();
    }
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&cbor_bytes);
    format!("creqA{encoded}")
}

fn payment_required_response(mint_url: &str, amount: i64, description: &str) -> Response {
    let req = Nut18PaymentRequest {
        amount: Some(amount),
        unit: Some("sat".to_string()),
        mints: Some(vec![mint_url.to_string()]),
        description: Some(description.to_string()),
    };
    let creq_a = encode_creq_a(&req);

    let body = json!({
        "error": "payment required",
        "accepts": {
            "cashu": {
                "mint": mint_url,
                "amount": amount,
                "unit": "sat"
            }
        },
        "instructions": "Retry with X-Cashu header containing a valid Cashu token from the listed mint"
    });

    let mut response = (StatusCode::PAYMENT_REQUIRED, Json(body)).into_response();

    if !creq_a.is_empty() {
        if let Ok(val) = HeaderValue::from_str(&creq_a) {
            response.headers_mut().insert("x-cashu", val);
        }
    }

    response
}

pub async fn resolver_subscribe_handler(
    AxumState(state): AxumState<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    let Some(ref cfg) = state.resolver_config else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "resolver service disabled"})),
        )
            .into_response();
    };

    if !cfg.enabled {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "resolver service disabled"})),
        )
            .into_response();
    }

    let npub = headers
        .get("x-nostr-npub")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let cashu_token = headers
        .get("x-cashu")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if cashu_token.is_empty() {
        return payment_required_response(
            &cfg.mint_url,
            cfg.price_sats,
            "nodns resolver subscription",
        );
    }

    let verifier = Verifier::new(&cfg.mint_url, &cfg.mint_filter, cfg.price_sats);
    let verified_amount = match verifier.verify_payment(cashu_token, cfg.price_sats).await {
        Ok(amount) => amount,
        Err(e) => {
            warn!(error = %e, "resolver subscription payment verification failed");
            state
                .metrics
                .resolver_subscribe_failures
                .fetch_add(1, Ordering::Relaxed);
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "invalid payment", "reason": e.to_string()})),
            )
                .into_response();
        }
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let expires_at = now + (cfg.duration_days as i64) * 86400;

    let npub_ref = npub.as_deref();
    match state.store.create_resolver_subscription(
        npub_ref,
        expires_at,
        cfg.daily_query_limit,
        verified_amount,
    ) {
        Ok(token) => {
            info!(
                npub_provided = npub.is_some(),
                expires_at,
                price_sats = cfg.price_sats,
                "resolver subscription created"
            );
            state
                .metrics
                .resolver_subscribes
                .fetch_add(1, Ordering::Relaxed);
            Json(json!({
                "token": token,
                "expires_at": expires_at,
                "daily_query_limit": cfg.daily_query_limit,
                "doh_endpoint": "https://dns.nodns.shop/dns-query"
            }))
            .into_response()
        }
        Err(e) => {
            warn!(error = %e, "failed to create resolver subscription");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal error"})),
            )
                .into_response()
        }
    }
}

pub async fn resolver_auth_handler(
    AxumState(state): AxumState<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    let token = headers
        .get("x-subscription")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    state
        .metrics
        .resolver_auth_checks
        .fetch_add(1, Ordering::Relaxed);

    if token.is_empty() {
        state
            .metrics
            .resolver_auth_rejected
            .fetch_add(1, Ordering::Relaxed);
        return StatusCode::PAYMENT_REQUIRED.into_response();
    }

    match state.store.validate_resolver_subscription(token) {
        Ok(true) => {
            let mut response = StatusCode::OK.into_response();
            response.headers_mut().insert(
                HeaderName::from_static("x-authenticated-by"),
                HeaderValue::from_static("resolver"),
            );
            response
        }
        Ok(false) => {
            state
                .metrics
                .resolver_auth_rejected
                .fetch_add(1, Ordering::Relaxed);
            StatusCode::PAYMENT_REQUIRED.into_response()
        }
        Err(e) => {
            warn!(error = %e, "resolver auth check failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn resolver_status_handler(
    AxumState(state): AxumState<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    let token = headers
        .get("x-subscription")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if token.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "X-Subscription header required"})),
        )
            .into_response();
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    match state.store.get_resolver_subscription(token) {
        Ok(Some(sub)) => Json(json!({
            "active": sub.expires_at > now,
            "expires_at": sub.expires_at,
            "queries_today": sub.queries_today,
            "daily_query_limit": sub.daily_query_limit,
            "doh_endpoint": "https://dns.nodns.shop/dns-query"
        }))
        .into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "subscription not found"})),
        )
            .into_response(),
        Err(e) => {
            warn!(error = %e, "resolver status query failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal error"})),
            )
                .into_response()
        }
    }
}

pub async fn resolver_stats_handler(AxumState(state): AxumState<Arc<AppState>>) -> Response {
    let stats = state.store.resolver_stats().unwrap_or(ResolverStats {
        active_subscriptions: 0,
        total_subscriptions: 0,
        queries_today: 0,
    });

    Json(json!({
        "active_subscriptions": stats.active_subscriptions,
        "total_subscriptions": stats.total_subscriptions,
        "queries_today": stats.queries_today,
        "subscribes_total": state.metrics.resolver_subscribes.load(Ordering::Relaxed),
        "subscribe_failures_total": state.metrics.resolver_subscribe_failures.load(Ordering::Relaxed),
        "auth_checks_total": state.metrics.resolver_auth_checks.load(Ordering::Relaxed),
        "auth_rejected_total": state.metrics.resolver_auth_rejected.load(Ordering::Relaxed),
        "resolver_enabled": state.resolver_config.is_some(),
    }))
    .into_response()
}
