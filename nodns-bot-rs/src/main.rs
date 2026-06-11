#![allow(clippy::too_many_arguments, clippy::result_large_err)]

mod acme;
mod auth;
mod config;
mod dns;
mod dnssec_derivation;
mod event_processor;
mod handlers;
mod nip05;
mod parser;
mod payment;
mod store;
mod subscriber;
pub mod types;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use tower_governor::{
    governor::GovernorConfigBuilder,
    key_extractor::SmartIpKeyExtractor,
    GovernorLayer,
};

use axum::http::{HeaderName, HeaderValue};
use clap::Parser;
use tokio::signal;
use tower_http::set_header::SetResponseHeaderLayer;
use tracing::{info, warn};

use config::{Config, ZoneConfig};
use dns::Updater;
use payment::Verifier;
use store::Store;
use subscriber::Subscriber;
use types::{DelegationState, Metrics};

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

pub struct AppState {
    pub store: Arc<Store>,
    pub nip05: Arc<nip05::Nip05State>,
    pub acme: Option<Arc<acme::AcmeService>>,
    pub acme_environment: String,
    pub metrics: Metrics,
    pub start_time: Instant,
    pub dns_zones: Vec<config::ZoneConfig>,
}

// ---------------------------------------------------------------------------
// Background lease expiry task
// ---------------------------------------------------------------------------

async fn lease_expiry_task(
    store: Arc<Store>,
    zone_configs: Vec<ZoneConfig>,
) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
    loop {
        interval.tick().await;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let delegations = match store.get_delegations_past_valid_until() {
            Ok(d) => d,
            Err(e) => {
                tracing::error!(error = %e, "lease expiry task: failed to query delegations");
                continue;
            }
        };

        for del in &delegations {
            let grace_config = zone_configs.iter().find(|z| z.zone == del.zone);
            let grace_period_secs = grace_config
                .map(|z| z.lease.grace_period_days as i64 * 86400)
                .unwrap_or(30 * 86400);

            let state = DelegationState::from_str(&del.status).unwrap_or(DelegationState::Active);
            let grace_deadline = del.valid_until + grace_period_secs;

            if now >= grace_deadline {
                if state != DelegationState::Expired {
                    if let Err(e) = store.mark_delegation_expired(&del.domain, &del.zone) {
                        tracing::error!(
                            domain = %del.domain, zone = %del.zone, error = %e,
                            "lease expiry task: failed to mark delegation expired"
                        );
                    } else {
                        tracing::info!(
                            domain = %del.domain, zone = %del.zone,
                            "lease expiry task: delegation expired, name available for re-registration"
                        );
                    }
                }
            } else if state == DelegationState::Active {
                if let Err(e) = store.mark_delegation_grace(&del.domain, &del.zone) {
                    tracing::error!(
                        domain = %del.domain, zone = %del.zone, error = %e,
                        "lease expiry task: failed to mark delegation grace"
                    );
                } else {
                    tracing::info!(
                        domain = %del.domain, zone = %del.zone,
                        "lease expiry task: delegation entered grace period"
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Install rustls crypto provider BEFORE tokio runtime creation.
    // If the first TLS handshake races ahead in a worker thread before
    // install_default() completes, rustls panics. Installing before the
    // runtime prevents this.
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install rustls crypto provider");

    tracing_subscriber::fmt().json().with_target(false).init();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async_main())
}

async fn async_main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let cfg = Config::load(&cli.config)?;

    info!(
        zone = %cfg.nostr.zone,
        relays = ?cfg.nostr.relays,
        dns_zones = cfg.dns.zones.len(),
        "nodns-bot starting"
    );

     // ── Registrar identity + DNSSEC derivation + Nostr attestation ──
    if !cfg.registrar.nsec_hex.is_empty() && cfg.dnssec_derivation.enabled {
        let nsec_bytes = hex::decode(&cfg.registrar.nsec_hex)
            .expect("registrar.nsec_hex must be valid hex");
        assert_eq!(nsec_bytes.len(), 32, "registrar.nsec_hex must be 32 bytes");

        match dnssec_derivation::derive_dnssec_key(&nsec_bytes) {
            Ok(dnssec_key) => {
                let pem = dnssec_key.to_pkcs8_pem().expect("PKCS#8 export");

                let secret = p256::SecretKey::from_bytes((&dnssec_key.private_key_bytes().clone()).into())
                    .expect("valid P-256 key");
                let pub_key = secret.public_key();
                let pub_hex = hex::encode(pub_key.to_sec1_bytes());

                info!("registrar DNSSEC key derived via SLIP-10 → P-256");
                info!("P-256 public key (uncompressed): {}", pub_hex);
                info!("PKCS#8 PEM:\n{}", pem.trim());

                let registrar_keys = nostr_sdk::Keys::parse(&cfg.registrar.nsec_hex)
                    .expect("registrar.nsec_hex must be a valid nostr secret key");
                info!("registrar npub: {}", registrar_keys.public_key().to_hex());

                let zone = cfg.nostr.zone.clone();
                let dnskey_b64 = base64_dnskey(&pub_hex);
                let key_tag = compute_key_tag_13(&dnskey_b64);

                let mut tags: Vec<nostr_sdk::Tag> = vec![
                    nostr_sdk::Tag::custom(nostr_sdk::TagKind::custom("dnskey"), [
                        zone.clone(),
                        key_tag.to_string(),
                        "13".to_string(),
                        dnskey_b64,
                    ]),
                    nostr_sdk::Tag::custom(nostr_sdk::TagKind::custom("dnskey-derivation"), [
                        "slip10".to_string(),
                        "Nist256p1 seed".to_string(),
                    ]),
                ];
                for relay in &cfg.nostr.relays {
                    tags.push(nostr_sdk::Tag::custom(nostr_sdk::TagKind::custom("relay"), [relay.clone()]));
                }

                let builder = nostr_sdk::EventBuilder::new(
                    nostr_sdk::Kind::from(11111u16),
                    "",
                ).tags(tags);

                let client = nostr_sdk::Client::new(registrar_keys.clone());
                for relay in &cfg.nostr.relays {
                    if let Err(e) = client.add_relay(relay).await {
                        warn!("could not add relay {} for attestation: {}", relay, e);
                    }
                }
                client.connect().await;

                match client.send_event_builder(builder).await {
                    Ok(output) => {
                        info!("DNSKEY attestation event: {}", output.id().to_hex());
                        info!("sent to {} relay(s), failed: {:?}", output.success.len(), output.failed);
                    }
                    Err(e) => warn!("failed to publish DNSKEY attestation: {}", e),
                }
                client.disconnect().await;
            }
            Err(e) => {
                tracing::error!("DNSSEC key derivation failed: {e}");
            }
        }
    } else if cfg.dnssec_derivation.enabled && cfg.registrar.nsec_hex.is_empty() {
        tracing::warn!("dnssec_derivation.enabled but registrar.nsec_hex is empty — skipping");
    }

    // ── Store ──
    let acme_enc_key: Option<String> = cfg.acme.encryption_key.clone().or_else(|| {
        tracing::warn!("acme.encryption_key not set — generating ephemeral key; encrypted private keys will be unreadable after restart");
        Some(uuid::Uuid::new_v4().to_string())
    });
    let store = Arc::new(Store::new(&cfg.store.path, acme_enc_key.as_deref())?);
    store.init()?;

    // ── Authority ──
    let authority = auth::AuthorityChecker::new(store.clone(), cfg.registrar_keys.clone());

    // ── Payment ──
    let mut zone_verifiers: HashMap<String, Verifier> = HashMap::new();
    for zc in &cfg.dns.zones {
        if zc.payment.enabled {
            zone_verifiers.insert(zc.zone.clone(), Verifier::from_zone_config(&zc.payment));
        }
    }

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

    // ── ACME ──
    let acme_service: Option<Arc<acme::AcmeService>> = if cfg.acme.enabled {
        let zones = cfg.dns.zones.iter().map(|z| z.zone.clone()).collect();
        let svc = Arc::new(acme::AcmeService::new(
            cfg.acme.clone(),
            updaters.clone(),
            store.clone(),
            zones,
        ));
        info!("ACME enabled, directory_url = {}", cfg.acme.directory_url);
        Some(svc)
    } else {
        None
    };

    // ── HTTP health server ──
    let nip05_state = Arc::new(nip05::Nip05State {
        store: store.clone(),
        registrar_pubkeys: cfg.registrar_keys.clone(),
        relays: cfg.nostr.relays.clone(),
        zones: cfg.dns.zones.iter().map(|z| z.zone.clone()).collect(),
    });
    let app_state = Arc::new(AppState {
        store: store.clone(),
        nip05: nip05_state,
        acme: acme_service,
        acme_environment: cfg.acme.environment.clone(),
        metrics: Metrics::default(),
        start_time: Instant::now(),
        dns_zones: cfg.dns.zones.clone(),
    });
    let bind = cfg.server.bind.clone();
    let http_state = app_state.clone();
    let http_handle = tokio::spawn(async move {
        let api_limit = Arc::new(
            GovernorConfigBuilder::default()
                .key_extractor(SmartIpKeyExtractor)
                .per_second(1)
                .burst_size(30)
                .finish()
                .unwrap(),
        );

        let acme_limit = Arc::new(
            GovernorConfigBuilder::default()
                .key_extractor(SmartIpKeyExtractor)
                .per_second(1)
                .burst_size(3)
                .finish()
                .unwrap(),
        );

        let acme_routes = axum::Router::new()
            .route("/api/acme/order", axum::routing::post(handlers::acme_order_handler))
            .route("/api/acme/order/{id}", axum::routing::get(handlers::acme_cert_handler))
            .layer(GovernorLayer::new(acme_limit))
            .with_state(http_state.clone());

        let api_routes = axum::Router::new()
            .route("/.well-known/nostr.json", axum::routing::get(nip05::nip05_handler))
            .route("/health", axum::routing::get(handlers::health_handler))
.route("/api/records", axum::routing::get(handlers::records_handler))
.route("/api/check", axum::routing::get(handlers::check_handler))
.route("/api/zones/{zone}/pricing", axum::routing::get(handlers::zone_pricing_handler))
            .layer(GovernorLayer::new(api_limit))
            .with_state(http_state);

        let app = axum::Router::new()
            .merge(acme_routes)
            .merge(api_routes)
            .layer(SetResponseHeaderLayer::overriding(
                HeaderName::from_static("x-content-type-options"),
                HeaderValue::from_static("nosniff"),
            ))
            .layer(SetResponseHeaderLayer::overriding(
                HeaderName::from_static("x-frame-options"),
                HeaderValue::from_static("DENY"),
            ))
            .layer(SetResponseHeaderLayer::overriding(
                HeaderName::from_static("referrer-policy"),
                HeaderValue::from_static("strict-origin-when-cross-origin"),
            ))
            .layer(SetResponseHeaderLayer::overriding(
                HeaderName::from_static("permissions-policy"),
                HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
            ));

        let listener = tokio::net::TcpListener::bind(&bind).await.unwrap();
        info!(bind = %bind, "health server listening");
        axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await.unwrap();
    });

    // ── Nostr subscriber ──
    let subscriber = Subscriber::new(&cfg.nostr, store.clone());
    let mut event_rx = subscriber.subscribe()?;

    // ── Lease expiry task ──
    {
        let lease_store = store.clone();
        let lease_zones = cfg.dns.zones.clone();
        tokio::spawn(async move {
            lease_expiry_task(lease_store, lease_zones).await;
        });
    }

    // ── Event loop ──
    let ev_store = store.clone();
    let ev_auth = authority;
    let ev_updaters = updaters;
    let ev_zone_verifiers = zone_verifiers;
    let ev_cfg = cfg;
    let ev_metrics = app_state.clone();

    tokio::spawn(async move {
        loop {
            match event_rx.recv().await {
                Some(evt) => {
                    event_processor::process_nostr_event(
                        &evt, &ev_cfg, &ev_updaters, &ev_store,
                        &ev_auth, &ev_zone_verifiers, &ev_metrics.metrics,
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

fn base64_dnskey(pub_hex: &str) -> String {
    use base64::Engine;
    let pub_bytes = hex::decode(pub_hex).expect("valid hex pubkey");
    let mut rdata = vec![1, 1, 3, 13]; // flags: 257 (Zone Key + SEP = KSK), protocol: 3 (DNSSEC), algorithm: ECDSAP256SHA256
    rdata.extend_from_slice(&pub_bytes);
    base64::engine::general_purpose::STANDARD.encode(&rdata)
}

fn compute_key_tag_13(dnskey_b64: &str) -> u16 {
    use base64::Engine;
    let dnskey = base64::engine::general_purpose::STANDARD.decode(dnskey_b64).expect("valid base64");
    let sum: u32 = dnskey.chunks(2).map(|chunk| {
        if chunk.len() == 2 {
            u32::from(u16::from_be_bytes([chunk[0], chunk[1]]))
        } else {
            u32::from(chunk[0])
        }
    }).sum();
    ((sum + (sum >> 16)) & 0xFFFF) as u16
}
