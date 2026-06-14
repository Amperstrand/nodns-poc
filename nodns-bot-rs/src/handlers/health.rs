use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::extract::State as AxumState;
use axum::response::Json;
use serde::Serialize;

use crate::AppState;

#[derive(Serialize)]
pub struct RelayStatusInfo {
    url: String,
    status: String,
}

#[derive(Serialize)]
pub struct HealthResponse {
    status: String,
    uptime_seconds: u64,
    events_processed: i64,
    events_rejected: i64,
    ddns_successes: i64,
    ddns_failures: i64,
    relay_urls: Vec<String>,
    relay_statuses: Vec<RelayStatusInfo>,
    zone_names: Vec<String>,
    zone_records: i64,
    last_event_at: Option<i64>,
    db_size_bytes: u64,
}

pub async fn health_handler(AxumState(state): AxumState<Arc<AppState>>) -> Json<HealthResponse> {
    // Query live relay connection status from the shared nostr-sdk Client.
    let relay_statuses: Vec<RelayStatusInfo> = state
        .nostr_client
        .relays()
        .await
        .into_iter()
        .map(|(url, relay)| RelayStatusInfo {
            url: url.to_string(),
            status: format!("{}", relay.status()),
        })
        .collect();

    let connected_count = relay_statuses
        .iter()
        .filter(|r| r.status == "Connected")
        .count();

    let zone_records = state.store.total_record_count().unwrap_or(-1);

    let last_event_at = {
        let ts = state.metrics.last_event_at.load(Ordering::Relaxed);
        if ts > 0 {
            Some(ts)
        } else {
            None
        }
    };

    let db_size_bytes = std::fs::metadata(&state.db_path)
        .map(|m| m.len())
        .unwrap_or(0);

    let ddns_failures = state.metrics.ddns_failures.load(Ordering::Relaxed);
    let status = if connected_count == 0 || ddns_failures > 10 {
        "degraded".to_string()
    } else {
        "ok".to_string()
    };

    Json(HealthResponse {
        status,
        uptime_seconds: state.start_time.elapsed().as_secs(),
        events_processed: state.metrics.events_processed.load(Ordering::Relaxed),
        events_rejected: state.metrics.events_rejected.load(Ordering::Relaxed),
        ddns_successes: state.metrics.ddns_successes.load(Ordering::Relaxed),
        ddns_failures,
        relay_urls: state.relay_urls.clone(),
        relay_statuses,
        zone_names: state.dns_zones.iter().map(|z| z.zone.clone()).collect(),
        zone_records,
        last_event_at,
        db_size_bytes,
    })
}
