use std::sync::Arc;

use axum::extract::{Path, Query, State as AxumState};
use axum::http::header;
use axum::response::{IntoResponse, Json, Response};
use serde::{Deserialize, Serialize};

use crate::event_processor::resolve_fqdn;
use crate::types::EventRecord;
use crate::AppState;
use nodns_connectors::dns::query_txt_records;

// ---------------------------------------------------------------------------
// API response types
// ---------------------------------------------------------------------------

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
// Record handlers
// ---------------------------------------------------------------------------

pub async fn records_handler(
    AxumState(state): AxumState<Arc<AppState>>,
    Query(params): Query<RecordsQuery>,
) -> Json<RecordsResponse> {
    let records = if let Some(pubkey) = params.pubkey {
        state
            .store
            .get_records_by_pubkey(&pubkey)
            .unwrap_or_default()
    } else if let Some(domain) = params.domain {
        state
            .store
            .get_records_by_domain(&domain)
            .unwrap_or_default()
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
    Json(RecordsResponse {
        records: out,
        count,
    })
}

// ---------------------------------------------------------------------------
// Record lookup by npub / pubkey prefix
// ---------------------------------------------------------------------------

pub async fn records_by_npub_handler(
    AxumState(state): AxumState<Arc<AppState>>,
    Path(npub): Path<String>,
) -> Json<RecordsResponse> {
    let records = state
        .store
        .get_records_by_npub_exact(&npub)
        .unwrap_or_default();
    let out: Vec<ApiRecord> = records
        .into_iter()
        .map(|r| ApiRecord {
            npub: r.npub.clone(),
            name: if r.name == "@" || r.name.is_empty() {
                String::new()
            } else {
                r.name.clone()
            },
            fqdn: resolve_fqdn(&r.npub, &r.name, &r.zone, &state.store),
            record_type: r.record_type,
            ttl: r.ttl,
            rdata: r.rdata,
            created_at: r.created_at,
        })
        .collect();
    let count = out.len();
    Json(RecordsResponse {
        records: out,
        count,
    })
}

pub async fn records_by_prefix_handler(
    AxumState(state): AxumState<Arc<AppState>>,
    Path(prefix): Path<String>,
) -> Json<RecordsResponse> {
    let records = state
        .store
        .lookup_by_pubkey_prefix(&prefix)
        .unwrap_or_default();
    let out: Vec<ApiRecord> = records
        .into_iter()
        .map(|r| ApiRecord {
            npub: r.npub.clone(),
            name: if r.name == "@" || r.name.is_empty() {
                String::new()
            } else {
                r.name.clone()
            },
            fqdn: resolve_fqdn(&r.npub, &r.name, &r.zone, &state.store),
            record_type: r.record_type,
            ttl: r.ttl,
            rdata: r.rdata,
            created_at: r.created_at,
        })
        .collect();
    let count = out.len();
    Json(RecordsResponse {
        records: out,
        count,
    })
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
    let zone = state
        .dns_zones
        .first()
        .map(|z| z.zone.clone())
        .unwrap_or_default();

    let api_records = state
        .store
        .get_records_by_domain(&format!("{name}.{zone}"))
        .unwrap_or_default();

    let api_source = CheckSource {
        registered: !api_records.is_empty(),
        records: api_records
            .into_iter()
            .map(|r| ApiRecord {
                npub: r.npub.clone(),
                name: if r.name == "@" || r.name.is_empty() {
                    String::new()
                } else {
                    r.name.clone()
                },
                fqdn: resolve_fqdn(&r.npub, &r.name, &r.zone, &state.store),
                record_type: r.record_type,
                ttl: r.ttl,
                rdata: r.rdata,
                created_at: r.created_at,
            })
            .collect(),
    };

    let dns_source = {
        let knot_addr = state
            .dns_zones
            .first()
            .map(|z| z.knot_address.clone())
            .unwrap_or_default();
        let nameserver: std::net::SocketAddr = match knot_addr.parse() {
            Ok(a) => a,
            Err(_) => {
                return Json(CheckResponse {
                    name,
                    zone,
                    api: api_source,
                    dns: CheckSource {
                        registered: false,
                        records: vec![],
                    },
                });
            }
        };
        let fqdn = format!("{name}.{zone}.");
        let result = query_txt_records(nameserver, &fqdn).await;
        CheckSource {
            registered: result.registered,
            records: result
                .records
                .into_iter()
                .map(|r| ApiRecord {
                    npub: String::new(),
                    name: name.clone(),
                    fqdn: format!("{name}.{zone}"),
                    record_type: r.record_type,
                    ttl: r.ttl,
                    rdata: r.rdata,
                    created_at: 0,
                })
                .collect(),
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
// Zone export & zone-scoped records
// ---------------------------------------------------------------------------

pub async fn zone_export(
    AxumState(state): AxumState<Arc<AppState>>,
    Path(zone): Path<String>,
) -> Response {
    let records: Vec<EventRecord> = state
        .store
        .list_zone_records(&zone)
        .unwrap_or_default()
        .into_iter()
        .filter(|r| !r.deleted)
        .collect();

    let default_ttl = state
        .dns_zones
        .iter()
        .find(|z| z.zone == zone)
        .map(|z| z.default_ttl)
        .unwrap_or(3600);

    let body = format_zone_file(&records, &zone, default_ttl);

    (
        axum::http::StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        body,
    )
        .into_response()
}

pub async fn zone_records(
    AxumState(state): AxumState<Arc<AppState>>,
    Path(zone): Path<String>,
) -> Response {
    let records = state.store.list_zone_records(&zone).unwrap_or_default();

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
    Json(RecordsResponse {
        records: out,
        count,
    })
    .into_response()
}

fn record_type_sort_key(rt: &str) -> u8 {
    match rt {
        "A" => 0,
        "AAAA" => 1,
        "CNAME" => 2,
        "MX" => 3,
        "TXT" => 4,
        _ => 5,
    }
}

fn zone_file_owner_name(record: &EventRecord) -> String {
    if record.name == "@" || record.name.is_empty() {
        record.npub.clone()
    } else {
        format!("{}.{}", record.name, record.npub)
    }
}

fn format_zone_file(records: &[EventRecord], zone: &str, default_ttl: u32) -> String {
    let mut sorted: Vec<&EventRecord> = records.iter().collect();
    sorted.sort_by(|a, b| {
        let owner_a = zone_file_owner_name(a);
        let owner_b = zone_file_owner_name(b);
        let cmp = owner_a.to_lowercase().cmp(&owner_b.to_lowercase());
        if cmp != std::cmp::Ordering::Equal {
            return cmp;
        }
        record_type_sort_key(&a.record_type).cmp(&record_type_sort_key(&b.record_type))
    });

    let mut out = String::with_capacity(256 + sorted.len() * 64);
    out.push_str(&format!("$ORIGIN {zone}.\n"));
    out.push_str(&format!("$TTL {default_ttl}\n\n"));

    for r in &sorted {
        let owner = zone_file_owner_name(r);
        let ttl = if r.ttl > 0 { r.ttl } else { default_ttl };
        let rdata = if r.record_type == "TXT" {
            format!("\"{}\"", r.rdata.replace('\\', "\\\\").replace('"', "\\\""))
        } else {
            r.rdata.clone()
        };
        out.push_str(&format!(
            "{owner}\tIN\t{rtype}\t{ttl}\t{rdata}\n",
            rtype = r.record_type,
        ));
    }

    out
}
