use std::sync::Arc;
use std::sync::atomic::Ordering;

use axum::extract::State as AxumState;
use axum::extract::Path;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Json, Response};
use base64::Engine;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

use crate::types::{AcmeOrderLog, build_fqdn};

// ---------------------------------------------------------------------------
// API response types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct HealthResponse {
    status: &'static str,
    uptime_seconds: u64,
    events_processed: i64,
    events_rejected: i64,
    ddns_successes: i64,
    ddns_failures: i64,
}

#[derive(Serialize)]
pub struct ApiRecord {
    npub: String,
    name: String,
    fqdn: String,
    #[serde(rename = "type")]
    record_type: String,
    ttl: u32,
    rdata: String,
    created_at: i64,
}

#[derive(Serialize)]
pub struct RecordsResponse {
    records: Vec<ApiRecord>,
    count: usize,
}

#[derive(Deserialize)]
pub struct AcmeOrderRequest {
    domain: String,
    email: Option<String>,
    csr_der: Option<String>,
    environment: Option<String>,
    ca: Option<String>,
}

#[derive(Serialize)]
pub struct AcmeOrderResponse {
    order_id: String,
    status: String,
}

#[derive(Serialize)]
pub struct AcmeCertResponse {
    order_id: String,
    status: String,
    domain: String,
    certificate_pem: Option<String>,
    private_key_pem: Option<String>,
    error: Option<String>,
    acme_environment: String,
    logs: Vec<AcmeOrderLog>,
}

// ---------------------------------------------------------------------------
// AppState (re-exported from crate root)
// ---------------------------------------------------------------------------

/// Re-export AppState so handlers can reference it without coupling to main.
/// AppState is defined in main.rs as `pub struct AppState`.
/// We use `crate::AppState` to reference it.
use crate::AppState;

// ---------------------------------------------------------------------------
// Axum handlers
// ---------------------------------------------------------------------------

pub async fn health_handler(AxumState(state): AxumState<Arc<AppState>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        uptime_seconds: state.start_time.elapsed().as_secs(),
        events_processed: state.metrics.events_processed.load(Ordering::Relaxed),
        events_rejected: state.metrics.events_rejected.load(Ordering::Relaxed),
        ddns_successes: state.metrics.ddns_successes.load(Ordering::Relaxed),
        ddns_failures: state.metrics.ddns_failures.load(Ordering::Relaxed),
    })
}

pub async fn records_handler(AxumState(state): AxumState<Arc<AppState>>) -> Json<RecordsResponse> {
    match state.store.list_all_records() {
        Ok(records) => {
            let out: Vec<ApiRecord> = records
                .into_iter()
                .map(|r| {
                    let name = if r.name == "@" || r.name.is_empty() {
                        String::new()
                    } else {
                        r.name.clone()
                    };
                    ApiRecord {
                        npub: r.npub.clone(),
                        name,
                        fqdn: build_fqdn(&r.npub, &r.name, &r.zone),
                        record_type: r.record_type,
                        ttl: r.ttl,
                        rdata: r.rdata,
                        created_at: r.created_at,
                    }
                })
                .collect();
            let count = out.len();
            Json(RecordsResponse { records: out, count })
        }
        Err(e) => {
            error!(error = %e, "failed to list records");
            Json(RecordsResponse { records: vec![], count: 0 })
        }
    }
}

// ---------------------------------------------------------------------------
// ACME handlers
// ---------------------------------------------------------------------------

pub async fn acme_order_handler(
    AxumState(state): AxumState<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<AcmeOrderRequest>,
) -> Response {
    let Some(ref acme_service) = state.acme else {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "ACME is disabled"})),
        )
            .into_response();
    };

    let npub = headers
        .get("X-Nostr-Npub")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "api".to_string());

    let domain = body.domain.trim().to_lowercase();
    let _email = body.email.unwrap_or_else(|| format!("cert@{}", domain));

    let records = match state.store.list_all_records() {
        Ok(r) => r,
        Err(e) => {
            error!(error = %e, "failed to list records for ACME validation");
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Internal error"})),
            )
                .into_response();
        }
    };

    let domain_matches = records.iter().any(|r| {
        let fqdn = build_fqdn(&r.npub, &r.name, &r.zone);
        fqdn.trim_end_matches('.').eq_ignore_ascii_case(&domain)
    });

    if !domain_matches {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "domain not found in records"})),
        )
            .into_response();
    }

    let order_id = uuid::Uuid::new_v4().to_string();

    let csr_der_raw = body.csr_der;

    // Resolve per-request CA/environment to directory URL
    let effective_ca = body.ca.unwrap_or_else(|| "zerossl".to_string());
    let directory_url = match effective_ca.as_str() {
        "zerossl" => "https://acme.zerossl.com/v2/DV90".to_string(),
        "letsencrypt-production" | "production" => "https://acme-v02.api.letsencrypt.org/directory".to_string(),
        "letsencrypt-staging" | "staging" | _ => "https://acme-staging-v02.api.letsencrypt.org/directory".to_string(),
    };
    let effective_env = effective_ca.clone();

    if let Err(e) = state.store.save_acme_order(&order_id, &domain, &npub, "pending", csr_der_raw.as_deref(), Some(&effective_env)) {
        error!(error = %e, "failed to save ACME order");
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Internal error"})),
        )
            .into_response();
    }

    let acme = acme_service.clone();
    let domain_clone = domain.clone();
    let oid = order_id.clone();
    let csr_der = match csr_der_raw {
        Some(b) => match base64::engine::general_purpose::STANDARD.decode(&b) {
            Ok(der) => Some(der),
            Err(e) => {
                warn!(error = %e, "invalid CSR base64");
                return (
                    axum::http::StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": "invalid csr_base64: not valid base64"})),
                )
                    .into_response();
            }
        },
        None => None,
    };

    tokio::spawn(async move {
        if let Err(e) = acme.request_certificate(&oid, &domain_clone, "api", csr_der, Some(&directory_url)).await {
            error!(order_id = %oid, error = %e, "ACME order failed");
        }
    });

    info!(order_id = %order_id, domain = %domain, "ACME order accepted (background)");

    Json(AcmeOrderResponse {
        order_id,
        status: "pending".to_string(),
    })
    .into_response()
}

pub async fn acme_cert_handler(
    AxumState(state): AxumState<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let request_npub = headers
        .get("X-Nostr-Npub")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    match state.store.get_acme_order(&id) {
        Ok(Some(order)) => {
            if order.npub != "api" {
                match request_npub {
                    Some(ref npub) if npub == &order.npub => {}
                    _ => {
                        return (
                            axum::http::StatusCode::FORBIDDEN,
                            Json(serde_json::json!({"error": "npub mismatch"})),
                        )
                            .into_response();
                    }
                }
            }
            if order.private_key_pem.is_some() {
                if let Err(e) = state.store.clear_acme_private_key(&id) {
                    error!(order_id = %id, error = %e, "failed to clear private key");
                }
            }
            let logs = state.store.get_acme_order_logs(&id).unwrap_or_default();
            let acme_env = order.environment.as_deref().unwrap_or(&state.acme_environment).to_string();
            Json(AcmeCertResponse {
                order_id: order.id,
                status: order.status,
                domain: order.domain,
                certificate_pem: order.certificate_pem,
                private_key_pem: order.private_key_pem,
                error: order.error.as_ref().map(|_| "Certificate issuance failed. Check your domain and try again.".to_string()),
                acme_environment: acme_env,
                logs: logs.into_iter().map(|mut log| {
                    log.details = None;
                    if log.stage == "error" {
                        log.message = "Order failed".to_string();
                    }
                    log
                }).collect(),
            })
            .into_response()
        }
        Ok(None) => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "order not found"})),
        )
            .into_response(),
        Err(e) => {
            error!(error = %e, "failed to get ACME order");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Internal error"})),
            )
                .into_response()
        }
    }
}
