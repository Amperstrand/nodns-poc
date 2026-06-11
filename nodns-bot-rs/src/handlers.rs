use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use axum::extract::State as AxumState;
use axum::extract::Path;
use axum::extract::Query;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Json, Response};
use base64::Engine;
use nostr_sdk::nips::nip19::ToBech32;
use nostr_sdk::Keys;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

use crate::dns::query_txt_records;
use crate::event_processor::resolve_fqdn;
use crate::types::AcmeOrderLog;

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

#[derive(Deserialize, Default)]
pub struct RecordsQuery {
    pubkey: Option<String>,
    domain: Option<String>,
}

#[derive(Deserialize)]
pub struct AcmeOrderRequest {
    domain: String,
    email: Option<String>,
    csr_der: Option<String>,
    #[allow(dead_code)]
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

#[derive(Serialize)]
pub struct ZonePricingResponse {
    zone: String,
    enabled: bool,
    create_price: u64,
    update_price: u64,
    delete_price: u64,
    npub_names_free: bool,
    mint_url: String,
    mint_filter: String,
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

pub async fn records_handler(
    AxumState(state): AxumState<Arc<AppState>>,
    Query(params): Query<RecordsQuery>,
) -> Json<RecordsResponse> {
    let records = if let Some(pubkey) = params.pubkey {
        state.store.get_records_by_pubkey(&pubkey).unwrap_or_default()
    } else if let Some(domain) = params.domain {
        state.store.get_records_by_domain(&domain).unwrap_or_default()
    } else {
        state.store.list_all_records().unwrap_or_default()
    };

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
                fqdn: resolve_fqdn(&r.npub, &r.name, &r.zone, &state.store),
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

// ---------------------------------------------------------------------------
// Zone pricing handler
// ---------------------------------------------------------------------------

pub async fn zone_pricing_handler(
    AxumState(state): AxumState<Arc<AppState>>,
    Path(zone): Path<String>,
) -> Response {
    match state.dns_zones.iter().find(|z| z.zone == zone) {
        Some(zc) => {
            let p = &zc.payment;
            Json(ZonePricingResponse {
                zone: zc.zone.clone(),
                enabled: p.enabled,
                create_price: p.create_price,
                update_price: p.update_price,
                delete_price: p.delete_price,
                npub_names_free: p.npub_names_free,
                mint_url: p.mint_url.clone(),
                mint_filter: p.mint_filter.clone(),
            })
            .into_response()
        }
        None => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "zone not found"})),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// Registration check handler
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct CheckResponse {
    name: String,
    zone: String,
    api: CheckSource,
    dns: CheckSource,
}

#[derive(Serialize)]
pub struct CheckSource {
    registered: bool,
    records: Vec<ApiRecord>,
}

#[derive(Deserialize)]
pub struct CheckQuery {
    name: String,
}

pub async fn check_handler(
    AxumState(state): AxumState<Arc<AppState>>,
    Query(params): Query<CheckQuery>,
) -> Json<CheckResponse> {
    let name = params.name.trim().to_lowercase();
    let zone = state.dns_zones.first()
        .map(|z| z.zone.clone())
        .unwrap_or_default();

    let api_records = state.store.get_records_by_domain(&format!("{name}.{zone}"))
        .unwrap_or_default();

    let api_source = CheckSource {
        registered: !api_records.is_empty(),
        records: api_records.into_iter().map(|r| ApiRecord {
            npub: r.npub.clone(),
            name: if r.name == "@" || r.name.is_empty() { String::new() } else { r.name.clone() },
            fqdn: resolve_fqdn(&r.npub, &r.name, &r.zone, &state.store),
            record_type: r.record_type,
            ttl: r.ttl,
            rdata: r.rdata,
            created_at: r.created_at,
        }).collect(),
    };

    let dns_source = {
        let knot_addr = state.dns_zones.first()
            .map(|z| z.knot_address.clone())
            .unwrap_or_default();
        let nameserver: std::net::SocketAddr = match knot_addr.parse() {
            Ok(a) => a,
            Err(_) => {
                return Json(CheckResponse { name, zone, api: api_source, dns: CheckSource { registered: false, records: vec![] } });
            }
        };
        let fqdn = format!("{name}.{zone}.");
        let result = query_txt_records(nameserver, &fqdn).await;
        CheckSource {
            registered: result.registered,
            records: result.records.into_iter().map(|r| ApiRecord {
                npub: String::new(),
                name: name.clone(),
                fqdn: format!("{name}.{zone}"),
                record_type: r.record_type,
                ttl: r.ttl,
                rdata: r.rdata,
                created_at: 0,
            }).collect(),
        }
    };

    Json(CheckResponse {
        name,
        zone,
        api: api_source,
        dns: dns_source,
    })
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
        let fqdn = resolve_fqdn(&r.npub, &r.name, &r.zone, &state.store);
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
        "letsencrypt-staging" | "staging" => "https://acme-staging-v02.api.letsencrypt.org/directory".to_string(),
        _ => "https://acme-staging-v02.api.letsencrypt.org/directory".to_string(),
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

// ---------------------------------------------------------------------------
// DynDNS v2 compatible update handler
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct DynDnsUpdateParams {
    hostname: Option<String>,
    myip: Option<String>,
}

/// Plain-text response helper for DynDNS protocol.
fn dyndns_response(status_code: axum::http::StatusCode, body: &str) -> Response {
    (
        status_code,
        [(axum::http::header::CONTENT_TYPE, "text/plain")],
        body.to_string(),
    )
        .into_response()
}

pub async fn dyndns_update_handler(
    AxumState(state): AxumState<Arc<AppState>>,
    headers: HeaderMap,
    connect_info: axum::extract::ConnectInfo<SocketAddr>,
    Query(params): Query<DynDnsUpdateParams>,
) -> Response {
    let auth_header = match headers.get("authorization").and_then(|v| v.to_str().ok()) {
        Some(h) => h,
        None => return dyndns_response(axum::http::StatusCode::UNAUTHORIZED, "badauth"),
    };

    let (username, password) = match parse_basic_auth(auth_header) {
        Some(pair) => pair,
        None => return dyndns_response(axum::http::StatusCode::UNAUTHORIZED, "badauth"),
    };

    if password.is_empty() {
        return dyndns_response(axum::http::StatusCode::UNAUTHORIZED, "badauth");
    }

    let keys = match Keys::parse(&password) {
        Ok(k) => k,
        Err(e) => {
            warn!(error = %e, "dyndns: failed to parse nsec");
            return dyndns_response(axum::http::StatusCode::UNAUTHORIZED, "badauth");
        }
    };

    let derived_npub = match keys.public_key().to_bech32() {
        Ok(n) => n,
        Err(e) => {
            warn!(error = %e, "dyndns: failed to encode derived npub");
            return dyndns_response(axum::http::StatusCode::UNAUTHORIZED, "badauth");
        }
    };
    let derived_pubkey_hex = keys.public_key().to_hex();

    if !username.is_empty() && username != derived_npub {
        return dyndns_response(axum::http::StatusCode::UNAUTHORIZED, "badauth");
    }

    let hostname = match params.hostname {
        Some(ref h) if !h.is_empty() => h.trim().to_lowercase(),
        _ => return dyndns_response(axum::http::StatusCode::BAD_REQUEST, "notfqdn"),
    };

    if !hostname.contains('.') {
        return dyndns_response(axum::http::StatusCode::BAD_REQUEST, "notfqdn");
    }

    let zone = match find_zone_for_hostname(&hostname, &state.dns_zones) {
        Some(z) => z.clone(),
        None => return dyndns_response(axum::http::StatusCode::BAD_REQUEST, "notfqdn"),
    };

    let zone_config = state.dns_zones.iter().find(|z| z.zone == zone);
    let default_ttl = zone_config.map(|z| z.default_ttl).unwrap_or(3600);

    let zone_suffix = format!(".{}", zone);
    let (name, is_npub_name) = if hostname.ends_with(&zone_suffix) {
        let prefix = &hostname[..hostname.len() - zone_suffix.len()];
        if prefix.is_empty() {
            return dyndns_response(axum::http::StatusCode::BAD_REQUEST, "notfqdn");
        }
        if prefix.starts_with("npub1") {
            if prefix != derived_npub {
                return dyndns_response(axum::http::StatusCode::FORBIDDEN, "nohost");
            }
            ("@".to_string(), true)
        } else {
            (prefix.to_string(), false)
        }
    } else {
        return dyndns_response(axum::http::StatusCode::BAD_REQUEST, "notfqdn");
    };

    if !is_npub_name {
        match state.store.get_active_delegation(&name, &zone) {
            Ok(Some(del)) => {
                if del.npub != derived_npub {
                    return dyndns_response(axum::http::StatusCode::FORBIDDEN, "nohost");
                }
            }
            Ok(None) => {
                return dyndns_response(axum::http::StatusCode::FORBIDDEN, "nohost");
            }
            Err(e) => {
                error!(error = %e, "dyndns: failed to check delegation");
                return dyndns_response(
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "911",
                );
            }
        }
    }

    let ip_str = match &params.myip {
        Some(ip) if !ip.is_empty() => ip.clone(),
        _ => connect_info.0.ip().to_string(),
    };

    let (record_type_str, record_type_u16) = if ip_str.contains(':') {
        ("AAAA", 28)
    } else {
        ("A", 1)
    };

    if record_type_u16 == 1 {
        if ip_str.parse::<std::net::Ipv4Addr>().is_err() {
            return dyndns_response(axum::http::StatusCode::BAD_REQUEST, "notfqdn");
        }
    } else if ip_str.parse::<std::net::Ipv6Addr>().is_err() {
        return dyndns_response(axum::http::StatusCode::BAD_REQUEST, "notfqdn");
    }

    let fqdn = if name == "@" {
        format!("{}.{}.", derived_npub, zone)
    } else {
        format!("{}.{}.", name, zone)
    };

    let existing_records = state
        .store
        .get_records_by_pubkey(&derived_pubkey_hex)
        .unwrap_or_default();

    let current_ip = existing_records.iter().find(|r| {
        r.record_type == record_type_str
            && r.name == name
            && r.zone == zone
            && !r.deleted
    }).map(|r| r.rdata.clone());

    if current_ip.as_deref() == Some(&ip_str) {
        return dyndns_response(axum::http::StatusCode::OK, &format!("nochg {ip_str}"));
    }

    let updater = match state.updaters.get(&zone) {
        Some(u) => u,
        None => {
            error!(zone = %zone, "dyndns: no updater for zone");
            return dyndns_response(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "911",
            );
        }
    };

    if let Err(e) = updater.update_record(&fqdn, default_ttl, record_type_u16, &ip_str).await {
        error!(error = %e, fqdn = %fqdn, "dyndns: DDNS update failed");
        return dyndns_response(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "911",
        );
    }

    let event_id = format!("dyndns-{}", uuid::Uuid::new_v4());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    if let Err(e) = state.store.save_event(
        &event_id,
        &derived_npub,
        &derived_pubkey_hex,
        &name,
        record_type_str,
        default_ttl,
        &ip_str,
        &zone,
        now,
    ) {
        error!(error = %e, "dyndns: failed to save event to store");
    }

    info!(
        hostname = %hostname,
        ip = %ip_str,
        record_type = record_type_str,
        npub = %derived_npub,
        "dyndns: update successful"
    );

    dyndns_response(axum::http::StatusCode::OK, &format!("good {ip_str}"))
}

// ---------------------------------------------------------------------------
// DynDNS helpers
// ---------------------------------------------------------------------------

/// Parse a Basic Authorization header into (username, password).
fn parse_basic_auth(header: &str) -> Option<(String, String)> {
    let encoded = header.strip_prefix("Basic ")?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let decoded_str = String::from_utf8(decoded).ok()?;
    let (user, pass) = decoded_str.split_once(':')?;
    Some((user.to_string(), pass.to_string()))
}

/// Find which configured zone a hostname belongs to.
/// Returns the zone name (e.g. "nodns.shop") if the hostname ends with it.
fn find_zone_for_hostname<'a>(
    hostname: &str,
    zones: &'a [crate::config::ZoneConfig],
) -> Option<&'a String> {
    for zc in zones {
        let suffix = format!(".{}", zc.zone);
        if hostname.ends_with(&suffix) || hostname == zc.zone {
            return Some(&zc.zone);
        }
    }
    None
}

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

    let updater = match state.updaters.get(&registration.zone) {
        Some(u) => u,
        None => {
            error!(zone = %registration.zone, "acme-dns update: no updater for zone");
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Internal error"})),
            )
                .into_response();
        }
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

    Json(AcmeDnsUpdateResponse {
        txt: body.txt,
    })
    .into_response()
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
    use super::*;
    use std::collections::HashMap;
    use std::time::Instant;

    use axum::Router;
    use axum::body;
    use axum::extract::ConnectInfo;
    use http::Request;
    use nostr_sdk::nips::nip19::ToBech32;
    use nostr_sdk::Keys;
    use tower::ServiceExt;

    use crate::config;
    use crate::dns;
    use crate::nip05;
    use crate::types::Metrics;

    // =======================================================================
    // Test helpers
    // =======================================================================

    fn make_zone_config() -> config::ZoneConfig {
        config::ZoneConfig {
            knot_address: "127.0.0.1:1".to_string(),
            zone: "nodns.shop".to_string(),
            tsig_key_name: "test-key.".to_string(),
            tsig_key_secret: "dGVzdA==".to_string(),
            tsig_algorithm: "hmac-sha256".to_string(),
            default_ttl: 3600,
            ..Default::default()
        }
    }

    fn create_test_state() -> Arc<AppState> {
        let store = Arc::new(crate::store::Store::new(":memory:", None).unwrap());
        store.init().unwrap();

        let nip05_state = Arc::new(nip05::Nip05State {
            store: store.clone(),
            registrar_pubkeys: HashMap::new(),
            relays: vec![],
            zones: vec!["nodns.shop".to_string()],
        });

        Arc::new(AppState {
            store,
            nip05: nip05_state,
            acme: None,
            acme_environment: "staging".to_string(),
            metrics: Metrics::default(),
            start_time: Instant::now(),
            dns_zones: vec![make_zone_config()],
            updaters: Arc::new(HashMap::new()),
        })
    }

    fn create_test_state_with_updater() -> Arc<AppState> {
        let store = Arc::new(crate::store::Store::new(":memory:", None).unwrap());
        store.init().unwrap();

        let zc = make_zone_config();
        let updater = dns::Updater::new(&zc).unwrap();
        let mut updaters = HashMap::new();
        updaters.insert("nodns.shop".to_string(), updater);

        let nip05_state = Arc::new(nip05::Nip05State {
            store: store.clone(),
            registrar_pubkeys: HashMap::new(),
            relays: vec![],
            zones: vec!["nodns.shop".to_string()],
        });

        Arc::new(AppState {
            store,
            nip05: nip05_state,
            acme: None,
            acme_environment: "staging".to_string(),
            metrics: Metrics::default(),
            start_time: Instant::now(),
            dns_zones: vec![zc],
            updaters: Arc::new(updaters),
        })
    }

    fn build_router(state: Arc<AppState>) -> Router {
        Router::new()
            .route(
                "/nic/update",
                axum::routing::get(dyndns_update_handler).post(dyndns_update_handler),
            )
            .route("/register", axum::routing::post(acmedns_register_handler))
            .route("/update", axum::routing::post(acmedns_update_handler))
            .with_state(state)
    }

    fn make_auth_header(npub: &str, nsec: &str) -> String {
        let credentials = format!("{npub}:{nsec}");
        let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
        format!("Basic {encoded}")
    }

    fn make_connect_info() -> ConnectInfo<std::net::SocketAddr> {
        ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 12345)))
    }

    async fn response_body(response: axum::response::Response) -> String {
        let bytes = body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    async fn response_json(response: axum::response::Response) -> serde_json::Value {
        let bytes = body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    // =======================================================================
    // parse_basic_auth
    // =======================================================================

    #[test]
    fn parse_basic_auth_valid_credentials() {
        let encoded = base64::engine::general_purpose::STANDARD.encode("user:pass");
        let header = format!("Basic {encoded}");
        assert_eq!(
            parse_basic_auth(&header),
            Some(("user".to_string(), "pass".to_string()))
        );
    }

    #[test]
    fn parse_basic_auth_missing_prefix() {
        let encoded = base64::engine::general_purpose::STANDARD.encode("user:pass");
        assert_eq!(parse_basic_auth(&encoded), None);
    }

    #[test]
    fn parse_basic_auth_invalid_base64() {
        assert_eq!(parse_basic_auth("Basic !!!invalid!!!"), None);
    }

    #[test]
    fn parse_basic_auth_no_colon_separator() {
        let encoded = base64::engine::general_purpose::STANDARD.encode("userpass");
        let header = format!("Basic {encoded}");
        assert_eq!(parse_basic_auth(&header), None);
    }

    #[test]
    fn parse_basic_auth_empty_password() {
        let encoded = base64::engine::general_purpose::STANDARD.encode("user:");
        let header = format!("Basic {encoded}");
        assert_eq!(
            parse_basic_auth(&header),
            Some(("user".to_string(), String::new()))
        );
    }

    #[test]
    fn parse_basic_auth_colon_in_password() {
        // split_once(':') splits on the FIRST colon only
        let encoded =
            base64::engine::general_purpose::STANDARD.encode("user:pass:word");
        let header = format!("Basic {encoded}");
        assert_eq!(
            parse_basic_auth(&header),
            Some(("user".to_string(), "pass:word".to_string()))
        );
    }

    // =======================================================================
    // find_zone_for_hostname
    // =======================================================================

    #[test]
    fn find_zone_hostname_subdomain() {
        let zones = vec![make_zone_config()];
        assert_eq!(
            find_zone_for_hostname("test.nodns.shop", &zones),
            Some(&"nodns.shop".to_string())
        );
    }

    #[test]
    fn find_zone_hostname_exact_zone() {
        let zones = vec![make_zone_config()];
        assert_eq!(
            find_zone_for_hostname("nodns.shop", &zones),
            Some(&"nodns.shop".to_string())
        );
    }

    #[test]
    fn find_zone_hostname_different_zone_returns_none() {
        let zones = vec![make_zone_config()];
        assert_eq!(
            find_zone_for_hostname("test.example.com", &zones),
            None
        );
    }

    #[test]
    fn find_zone_hostname_multiple_zones() {
        let zones = vec![
            make_zone_config(),
            config::ZoneConfig {
                zone: "example.com".to_string(),
                knot_address: "127.0.0.1:2".to_string(),
                tsig_key_name: "k2.".to_string(),
                tsig_key_secret: "dGVzdA==".to_string(),
                ..Default::default()
            },
        ];
        assert_eq!(
            find_zone_for_hostname("foo.example.com", &zones),
            Some(&"example.com".to_string())
        );
        assert_eq!(
            find_zone_for_hostname("bar.nodns.shop", &zones),
            Some(&"nodns.shop".to_string())
        );
    }

    #[test]
    fn find_zone_hostname_case_sensitive() {
        // find_zone_for_hostname does NOT lowercase — the handler does that
        // before calling. Documenting current behavior: case-sensitive match.
        let zones = vec![make_zone_config()];
        assert_eq!(
            find_zone_for_hostname("TEST.NODNS.SHOP", &zones),
            None,
            "hostname is case-sensitive; handler lowercases before calling"
        );
        assert_eq!(
            find_zone_for_hostname("test.nodns.shop", &zones),
            Some(&"nodns.shop".to_string())
        );
    }

    // =======================================================================
    // DynDNS v2 integration tests
    // =======================================================================

    #[tokio::test]
    async fn dyndns_valid_auth_no_updater_returns_911() {
        // With valid npub/nsec pair but no DNS updater for the zone,
        // auth passes but the handler returns 500 "911" (no updater).
        let state = create_test_state();
        let app = build_router(state);

        let keys = Keys::generate();
        let npub = keys.public_key().to_bech32().unwrap();
        let nsec = keys.secret_key().to_bech32().unwrap();
        let hostname = format!("{npub}.nodns.shop");

        let uri = format!("/nic/update?hostname={hostname}&myip=1.2.3.4");

        let mut request = Request::builder()
            .method("GET")
            .uri(&uri)
            .header("authorization", make_auth_header(&npub, &nsec))
            .body(body::Body::empty())
            .unwrap();
        request.extensions_mut().insert(make_connect_info());

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 500);
        let text = response_body(response).await;
        assert_eq!(text, "911");
    }

    #[tokio::test]
    async fn dyndns_wrong_nsec_returns_badauth() {
        let state = create_test_state();
        let app = build_router(state);

        let keys_correct = Keys::generate();
        let keys_wrong = Keys::generate();
        let npub_correct = keys_correct.public_key().to_bech32().unwrap();
        let nsec_wrong = keys_wrong.secret_key().to_bech32().unwrap();

        let uri = format!(
            "/nic/update?hostname={npub_correct}.nodns.shop&myip=1.2.3.4"
        );

        let mut request = Request::builder()
            .method("GET")
            .uri(&uri)
            .header(
                "authorization",
                make_auth_header(&npub_correct, &nsec_wrong),
            )
            .body(body::Body::empty())
            .unwrap();
        request.extensions_mut().insert(make_connect_info());

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 401);
        assert_eq!(response_body(response).await, "badauth");
    }

    #[tokio::test]
    async fn dyndns_empty_password_returns_badauth() {
        let state = create_test_state();
        let app = build_router(state);

        let keys = Keys::generate();
        let npub = keys.public_key().to_bech32().unwrap();

        let auth = base64::engine::general_purpose::STANDARD.encode(format!("{npub}:"));
        let uri = format!("/nic/update?hostname={npub}.nodns.shop&myip=1.2.3.4");

        let mut request = Request::builder()
            .method("GET")
            .uri(&uri)
            .header("authorization", format!("Basic {auth}"))
            .body(body::Body::empty())
            .unwrap();
        request.extensions_mut().insert(make_connect_info());

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 401);
        assert_eq!(response_body(response).await, "badauth");
    }

    #[tokio::test]
    async fn dyndns_missing_auth_returns_badauth() {
        let state = create_test_state();
        let app = build_router(state);

        let mut request = Request::builder()
            .method("GET")
            .uri("/nic/update?hostname=test.nodns.shop&myip=1.2.3.4")
            .body(body::Body::empty())
            .unwrap();
        request.extensions_mut().insert(make_connect_info());

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 401);
        assert_eq!(response_body(response).await, "badauth");
    }

    #[tokio::test]
    async fn dyndns_hostname_without_dot_returns_notfqdn() {
        let state = create_test_state();
        let app = build_router(state);

        let keys = Keys::generate();
        let npub = keys.public_key().to_bech32().unwrap();
        let nsec = keys.secret_key().to_bech32().unwrap();

        let mut request = Request::builder()
            .method("GET")
            .uri("/nic/update?hostname=test&myip=1.2.3.4")
            .header("authorization", make_auth_header(&npub, &nsec))
            .body(body::Body::empty())
            .unwrap();
        request.extensions_mut().insert(make_connect_info());

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 400);
        assert_eq!(response_body(response).await, "notfqdn");
    }

    #[tokio::test]
    async fn dyndns_hostname_not_in_managed_zone_returns_notfqdn() {
        let state = create_test_state();
        let app = build_router(state);

        let keys = Keys::generate();
        let npub = keys.public_key().to_bech32().unwrap();
        let nsec = keys.secret_key().to_bech32().unwrap();

        let mut request = Request::builder()
            .method("GET")
            .uri("/nic/update?hostname=test.example.com&myip=1.2.3.4")
            .header("authorization", make_auth_header(&npub, &nsec))
            .body(body::Body::empty())
            .unwrap();
        request.extensions_mut().insert(make_connect_info());

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 400);
        assert_eq!(response_body(response).await, "notfqdn");
    }

    #[tokio::test]
    async fn dyndns_custom_name_no_delegation_returns_nohost() {
        let state = create_test_state();
        let app = build_router(state);

        let keys = Keys::generate();
        let npub = keys.public_key().to_bech32().unwrap();
        let nsec = keys.secret_key().to_bech32().unwrap();

        let mut request = Request::builder()
            .method("GET")
            .uri("/nic/update?hostname=alice.nodns.shop&myip=1.2.3.4")
            .header("authorization", make_auth_header(&npub, &nsec))
            .body(body::Body::empty())
            .unwrap();
        request.extensions_mut().insert(make_connect_info());

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 403);
        assert_eq!(response_body(response).await, "nohost");
    }

    #[tokio::test]
    async fn dyndns_ipv6_address_accepted_through_auth() {
        // IPv6 address passes IP validation; fails later at updater (no updater)
        let state = create_test_state();
        let app = build_router(state);

        let keys = Keys::generate();
        let npub = keys.public_key().to_bech32().unwrap();
        let nsec = keys.secret_key().to_bech32().unwrap();
        let hostname = format!("{npub}.nodns.shop");

        let uri = format!("/nic/update?hostname={hostname}&myip=2001:db8::1");

        let mut request = Request::builder()
            .method("GET")
            .uri(&uri)
            .header("authorization", make_auth_header(&npub, &nsec))
            .body(body::Body::empty())
            .unwrap();
        request.extensions_mut().insert(make_connect_info());

        let response = app.oneshot(request).await.unwrap();
        // Auth + IP validation pass; fails at "no updater" → 911
        assert_eq!(response.status(), 500);
        assert_eq!(response_body(response).await, "911");
    }

    #[tokio::test]
    async fn dyndns_missing_myip_uses_connect_info_ip() {
        // When myip is absent, handler falls back to ConnectInfo IP (127.0.0.1).
        // Still reaches "no updater" → 911, proving auth passed.
        let state = create_test_state();
        let app = build_router(state);

        let keys = Keys::generate();
        let npub = keys.public_key().to_bech32().unwrap();
        let nsec = keys.secret_key().to_bech32().unwrap();
        let hostname = format!("{npub}.nodns.shop");

        let uri = format!("/nic/update?hostname={hostname}");

        let mut request = Request::builder()
            .method("GET")
            .uri(&uri)
            .header("authorization", make_auth_header(&npub, &nsec))
            .body(body::Body::empty())
            .unwrap();
        request.extensions_mut().insert(make_connect_info());

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 500);
        assert_eq!(response_body(response).await, "911");
    }

    #[tokio::test]
    async fn dyndns_with_updater_dns_push_fails_returns_911() {
        // Updater exists but points at 127.0.0.1:1 (nothing listening).
        // DNS push fails → 500 "911". This proves the full path up to DNS push.
        let state = create_test_state_with_updater();
        let app = build_router(state);

        let keys = Keys::generate();
        let npub = keys.public_key().to_bech32().unwrap();
        let nsec = keys.secret_key().to_bech32().unwrap();
        let hostname = format!("{npub}.nodns.shop");

        let uri = format!("/nic/update?hostname={hostname}&myip=1.2.3.4");

        let mut request = Request::builder()
            .method("GET")
            .uri(&uri)
            .header("authorization", make_auth_header(&npub, &nsec))
            .body(body::Body::empty())
            .unwrap();
        request.extensions_mut().insert(make_connect_info());

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 500);
        assert_eq!(response_body(response).await, "911");
    }

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

    async fn register_acmedns(
        app: &Router,
    ) -> (String, String, String) {
        let request = Request::builder()
            .method("POST")
            .uri("/register")
            .body(body::Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        let json = response_json(response).await;
        let subdomain = json["subdomain"].as_str().unwrap().to_string();
        let username = json["username"].as_str().unwrap().to_string();
        let password = json["password"].as_str().unwrap().to_string();
        (subdomain, username, password)
    }

    #[tokio::test]
    async fn acmedns_update_correct_credentials_dns_fails() {
        // Register, then update with correct credentials.
        // DNS push fails (updater points at 127.0.0.1:1) → 500.
        let state = create_test_state_with_updater();
        let app = build_router(state);

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
