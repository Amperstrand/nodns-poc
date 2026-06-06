mod auth;
mod config;
mod dns;
mod parser;
mod payment;
mod store;
mod subscriber;
pub mod types;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Instant;

use axum::extract::State as AxumState;
use axum::response::Json;
use clap::Parser;
use nostr_sdk::nips::nip19::ToBech32;
use nostr_sdk::Event;
use serde::Serialize;
use tokio::signal;
use tracing::{error, info, warn};

use config::Config;
use dns::Updater;
use payment::Verifier;
use store::Store;
use subscriber::Subscriber;
use types::*;

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(name = "nodns-bot", about = "NoDNS bot — resolves DNS records from Nostr events")]
struct Cli {
    #[arg(short, long, default_value = "config.toml")]
    config: std::path::PathBuf,
}

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

struct AppState {
    store: Arc<Store>,
    metrics: Metrics,
    start_time: Instant,
}

struct Metrics {
    events_processed: AtomicI64,
    events_rejected: AtomicI64,
    ddns_successes: AtomicI64,
    ddns_failures: AtomicI64,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            events_processed: AtomicI64::new(0),
            events_rejected: AtomicI64::new(0),
            ddns_successes: AtomicI64::new(0),
            ddns_failures: AtomicI64::new(0),
        }
    }
}

// ---------------------------------------------------------------------------
// API response types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    uptime_seconds: u64,
    events_processed: i64,
    events_rejected: i64,
    ddns_successes: i64,
    ddns_failures: i64,
}

#[derive(Serialize)]
struct ApiRecord {
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
struct RecordsResponse {
    records: Vec<ApiRecord>,
    count: usize,
}

// ---------------------------------------------------------------------------
// Axum handlers
// ---------------------------------------------------------------------------

async fn health_handler(AxumState(state): AxumState<Arc<AppState>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        uptime_seconds: state.start_time.elapsed().as_secs(),
        events_processed: state.metrics.events_processed.load(Ordering::Relaxed),
        events_rejected: state.metrics.events_rejected.load(Ordering::Relaxed),
        ddns_successes: state.metrics.ddns_successes.load(Ordering::Relaxed),
        ddns_failures: state.metrics.ddns_failures.load(Ordering::Relaxed),
    })
}

async fn records_handler(AxumState(state): AxumState<Arc<AppState>>) -> Json<RecordsResponse> {
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
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().json().with_target(false).init();

    let cli = Cli::parse();
    let cfg = Config::load(&cli.config)?;

    info!(
        zone = %cfg.nostr.zone,
        relays = ?cfg.nostr.relays,
        dns_zones = cfg.dns.zones.len(),
        "nodns-bot starting"
    );

    // ── Store ──
    let store = Arc::new(Store::new(&cfg.store.path)?);
    store.init()?;

    // ── Authority ──
    let authority = auth::AuthorityChecker::new(store.clone(), cfg.registrar_keys.clone());

    // ── Payment ──
    let verifier: Option<Arc<Verifier>> = if cfg.payment.enabled {
        Some(Arc::new(Verifier::new(
            &cfg.payment.cashu_mint_url,
            cfg.payment.required_sats,
            cfg.payment.update_free,
        )))
    } else {
        None
    };

    // ── DNS updaters ──
    let mut updaters: HashMap<String, Updater> = HashMap::new();
    for zc in &cfg.dns.zones {
        let u = Updater::new(zc)?;
        if let Err(e) = u.test_connection().await {
            warn!(zone = %zc.zone, error = %e, "Knot DNS connection test failed (will retry on updates)");
        } else {
            info!(zone = %zc.zone, "Knot DNS connection test passed");
        }
        updaters.insert(zc.zone.clone(), u);
    }
    let updaters = Arc::new(updaters);

    // ── HTTP health server ──
    let app_state = Arc::new(AppState {
        store: store.clone(),
        metrics: Metrics::default(),
        start_time: Instant::now(),
    });
    let bind = cfg.server.bind.clone();
    let http_state = app_state.clone();
    let http_handle = tokio::spawn(async move {
        let app = axum::Router::new()
            .route("/health", axum::routing::get(health_handler))
            .route("/api/records", axum::routing::get(records_handler))
            .with_state(http_state);
        let listener = tokio::net::TcpListener::bind(&bind).await.unwrap();
        info!(bind = %bind, "health server listening");
        axum::serve(listener, app).await.unwrap();
    });

    // ── Nostr subscriber ──
    let subscriber = Subscriber::new(&cfg.nostr, store.clone());
    let mut event_rx = subscriber.subscribe()?;

    // ── Event loop ──
    let ev_store = store.clone();
    let ev_auth = authority;
    let ev_updaters = updaters;
    let ev_verifier = verifier.clone();
    let ev_cfg = cfg;
    let ev_metrics = app_state.clone();

    tokio::spawn(async move {
        loop {
            match event_rx.recv().await {
                Some(evt) => {
                    process_nostr_event(
                        &evt, &ev_cfg, &ev_updaters, &ev_store,
                        &ev_auth, ev_verifier.as_deref(), &ev_metrics.metrics,
                    ).await;
                }
                None => {
                    info!("event channel closed");
                    break;
                }
            }
        }
    });

    // ── Shutdown ──
    signal::ctrl_c().await?;
    info!("received SIGINT, shutting down");

    subscriber.stop();
    http_handle.abort();

    info!(
        uptime_seconds = app_state.start_time.elapsed().as_secs(),
        events_processed = app_state.metrics.events_processed.load(Ordering::Relaxed),
        "nodns-bot stopped"
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Event processing
// ---------------------------------------------------------------------------

async fn process_nostr_event(
    evt: &Event,
    cfg: &Config,
    updaters: &Arc<HashMap<String, Updater>>,
    store: &Arc<Store>,
    authority: &auth::AuthorityChecker,
    verifier: Option<&Verifier>,
    metrics: &Metrics,
) {
    let event_id = evt.id.to_hex();
    let pubkey_hex = evt.pubkey.to_hex();
    let created_at = evt.created_at.as_u64() as i64;

    let npub = match evt.pubkey.to_bech32() {
        Ok(n) => n,
        Err(e) => {
            error!(event_id = %event_id, error = %e, "failed to encode pubkey to npub");
            metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
            return;
        }
    };

    let parsed = match parser::classify_event(
        evt,
        &cfg.policy.allowed_types,
        cfg.policy.block_private_ip,
        cfg.policy.max_txt_length,
    ) {
        Ok(p) => p,
        Err(e) => {
            warn!(event_id = %event_id, error = %e, "event parse failed");
            metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
            return;
        }
    };

    if let Some(ref delegation) = parsed.delegation {
        process_delegation(delegation, &event_id, &pubkey_hex, &npub, created_at, cfg, store, authority, metrics);
    } else if let Some(ref registrar) = parsed.registrar {
        process_registrar(registrar, &event_id, &pubkey_hex, &npub, created_at, store, authority, metrics);
    } else if !parsed.records.is_empty() {
        process_dns_update(
            &parsed, &event_id, &pubkey_hex, &npub, created_at,
            cfg, updaters, store, authority, verifier, metrics,
        ).await;
    }

    if let Err(e) = store.set_last_seen(created_at) {
        error!(event_id = %event_id, error = %e, "failed to update last_seen");
    }
}

// ---------------------------------------------------------------------------
// process_delegation
// ---------------------------------------------------------------------------

fn process_delegation(
    delegation: &Delegation,
    event_id: &str,
    pubkey_hex: &str,
    _npub: &str,
    _created_at: i64,
    cfg: &Config,
    store: &Arc<Store>,
    authority: &auth::AuthorityChecker,
    metrics: &Metrics,
) {
    let zones: Vec<&str> = cfg.dns.zones.iter().map(|z| z.zone.as_str()).collect();
    let domain = delegation.domain.trim_end_matches('.');

    let matched_zone = zones.iter().find(|z| {
        domain.ends_with(&format!(".{z}")) || domain == **z
    });

    let Some(zone) = matched_zone else {
        warn!(domain = %delegation.domain, "delegation domain does not match any configured zone");
        metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
        return;
    };

    if let Err(e) = authority.validate_delegation(delegation, zone, pubkey_hex) {
        warn!(domain = %delegation.domain, error = %e, "delegation validation failed");
        metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
        return;
    }

    let domain_name = domain.trim_end_matches(&format!(".{zone}"));

    if let Err(e) = store.save_delegation(
        event_id, domain_name, zone,
        &delegation.npub, pubkey_hex,
        delegation.valid_from, delegation.valid_until, delegation.renew_by,
        pubkey_hex,
    ) {
        error!(event_id = %event_id, error = %e, "failed to save delegation");
        metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
        return;
    }

    info!(event_id = %event_id, domain = %delegation.domain, "delegation processed");
    metrics.events_processed.fetch_add(1, Ordering::Relaxed);
}

// ---------------------------------------------------------------------------
// process_registrar
// ---------------------------------------------------------------------------

fn process_registrar(
    registrar: &RegistrarKey,
    event_id: &str,
    pubkey_hex: &str,
    npub: &str,
    _created_at: i64,
    store: &Arc<Store>,
    authority: &auth::AuthorityChecker,
    metrics: &Metrics,
) {
    let is_registrar = match authority.is_registrar(&registrar.zone, pubkey_hex) {
        Ok(b) => b,
        Err(e) => {
            error!(event_id = %event_id, error = %e, "failed to check registrar status");
            metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
            return;
        }
    };
    if !is_registrar {
        warn!(event_id = %event_id, signer = %pubkey_hex, "unauthorized registrar key publication");
        metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
        return;
    }

    if let Err(e) = store.save_registrar_key(&registrar.zone, &registrar.pubkey_hex, npub, "nostr", event_id) {
        error!(event_id = %event_id, error = %e, "failed to save registrar key");
        metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
        return;
    }

    info!(event_id = %event_id, zone = %registrar.zone, "registrar key processed");
    metrics.events_processed.fetch_add(1, Ordering::Relaxed);
}

// ---------------------------------------------------------------------------
// process_dns_update
// ---------------------------------------------------------------------------

async fn process_dns_update(
    parsed: &ParsedEvent,
    event_id: &str,
    pubkey_hex: &str,
    npub: &str,
    created_at: i64,
    cfg: &Config,
    updaters: &Arc<HashMap<String, Updater>>,
    store: &Arc<Store>,
    authority: &auth::AuthorityChecker,
    verifier: Option<&Verifier>,
    metrics: &Metrics,
) {
    if parsed.records.is_empty() {
        return;
    }

    // Payment
    if let Some(v) = verifier {
        for zone_name in updaters.keys() {
            if let Err(e) = payment::check_event_payment(
                &parsed.payments, npub, &parsed.records, zone_name, store, Some(v),
            ).await {
                warn!(event_id = %event_id, error = %e, "payment verification failed");
                metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
                return;
            }
        }
    }

    // Record count
    match store.record_count_by_pubkey(pubkey_hex) {
        Ok(count) if count + parsed.records.len() > cfg.policy.max_records => {
            warn!(event_id = %event_id, current = count, new = parsed.records.len(), max = cfg.policy.max_records, "exceeds max records");
            metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
            return;
        }
        Err(e) => {
            error!(event_id = %event_id, error = %e, "failed to count records");
            metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
            return;
        }
        _ => {}
    }

    // Rate limit
    match store.events_in_last_minute(pubkey_hex) {
        Ok(recent) if recent >= cfg.policy.rate_limit => {
            warn!(event_id = %event_id, recent, limit = cfg.policy.rate_limit, "rate limited");
            metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
            return;
        }
        Err(e) => {
            error!(event_id = %event_id, error = %e, "failed to check rate limit");
            metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
            return;
        }
        _ => {}
    }

    // Update each record in each zone
    let mut all_ok = true;
    for (zone_name, updater) in updaters.iter() {
        for rec in &parsed.records {
            let fqdn = build_fqdn(npub, &rec.name, zone_name);

            if let Err(e) = authority.check_authority(&fqdn, zone_name, pubkey_hex) {
                warn!(event_id = %event_id, fqdn = %fqdn, error = %e, "authority check failed");
                all_ok = false;
                continue;
            }

            let rt = record_type_to_u16(&rec.record_type);

            if let Err(e) = updater.update_record(&fqdn, rec.ttl, rt, &rec.rdata).await {
                error!(event_id = %event_id, fqdn = %fqdn, r#type = %rec.record_type, error = %e, "DDNS update failed");
                metrics.ddns_failures.fetch_add(1, Ordering::Relaxed);
                all_ok = false;
                continue;
            }

            metrics.ddns_successes.fetch_add(1, Ordering::Relaxed);

            if let Err(e) = store.save_event(
                event_id, npub, pubkey_hex, &rec.name, &rec.record_type,
                rec.ttl, &rec.rdata, zone_name, created_at,
            ) {
                error!(event_id = %event_id, error = %e, "failed to save event");
            }
        }
    }

    if all_ok {
        metrics.events_processed.fetch_add(1, Ordering::Relaxed);
    }
}
