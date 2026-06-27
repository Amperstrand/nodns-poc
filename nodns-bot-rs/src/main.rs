#![allow(clippy::too_many_arguments, clippy::result_large_err)]

mod acme;
mod auth;
mod classify;
mod config;
mod dns_cache;
mod dns_update_server;
mod dnssec_derivation;
#[allow(dead_code)]
mod epp;
mod event_processor;
mod handlers;
mod nip05;
mod parser;
mod payment;
#[cfg(test)]
mod security_tests;
mod store;
mod subscriber;
pub mod types;

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use tower_governor::{
    governor::GovernorConfigBuilder, key_extractor::SmartIpKeyExtractor, GovernorLayer,
};

use axum::http::{HeaderName, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;
use clap::Parser;
use tokio::signal;
use tower_http::set_header::SetResponseHeaderLayer;
use tracing::{info, warn};

const CORRELATION_ID_HEADER: &str = "x-correlation-id";

async fn correlation_id_middleware(request: axum::extract::Request, next: Next) -> Response {
    let correlation_id = request
        .headers()
        .get(CORRELATION_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let span = tracing::info_span!("request", correlation_id = %correlation_id);
    let response = span.in_scope(|| next.run(request)).await;

    let mut response = response;
    let headers = response.headers_mut();
    headers.insert(
        HeaderName::from_static(CORRELATION_ID_HEADER),
        HeaderValue::from_str(&correlation_id)
            .unwrap_or_else(|_| HeaderValue::from_static("invalid")),
    );
    response
}

use config::{Config, ZoneConfig};
use nodns_connectors::cloudflare_backend::CloudflareBackend;
use nodns_connectors::connector::DnsConnector;
use nodns_connectors::dns::DdnsConfig;
use nodns_connectors::dns::Updater;
use nodns_connectors::failover::FailoverConnector;
use payment::Verifier;
use store::Store;
use subscriber::Subscriber;
use types::{DelegationState, Metrics};

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(
    name = "nodns-bot",
    about = "NoDNS bot — resolves DNS records from Nostr events"
)]
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
    pub updaters: Arc<HashMap<String, Arc<dyn DnsConnector>>>,
    pub nostr_client: nostr_sdk::Client,
    pub relay_urls: Vec<String>,
    pub db_path: std::path::PathBuf,
}

// ---------------------------------------------------------------------------
// Background lease expiry task
// ---------------------------------------------------------------------------

async fn lease_expiry_task(
    store: Arc<Store>,
    zone_configs: Vec<ZoneConfig>,
    updaters: Arc<HashMap<String, Arc<dyn DnsConnector>>>,
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
            let grace_period_secs =
                grace_config.map_or(30 * 86400, |z| i64::from(z.lease.grace_period_days) * 86400);

            let state = DelegationState::from_str(&del.status).unwrap_or(DelegationState::Active);
            let grace_deadline = del.valid_until + grace_period_secs;

            if now >= grace_deadline {
                if state != DelegationState::Expired {
                    if let Some(updater) = updaters.get(&del.zone) {
                        let records = store
                            .get_records_by_npub_exact(&del.npub)
                            .unwrap_or_default()
                            .into_iter()
                            .filter(|r| r.zone == del.zone)
                            .collect::<Vec<_>>();

                        let mut sent_fqdns: std::collections::HashSet<(String, u16)> =
                            HashSet::default();
                        for rec in &records {
                            let fqdn = if rec.name == "@" || rec.name.is_empty() {
                                format!("{}.{}.", del.domain, del.zone)
                            } else {
                                format!("{}.{}.{}.", rec.name, del.domain, del.zone)
                            };
                            let rt = crate::types::record_type_to_u16(&rec.record_type);
                            if sent_fqdns.insert((fqdn.clone(), rt)) {
                                if let Err(e) = updater.delete_record(&fqdn, rt).await {
                                    tracing::warn!(
                                        fqdn = %fqdn, error = %e,
                                        "lease expiry task: DDNS delete failed (continuing)"
                                    );
                                }
                            }
                        }
                    }

                    if let Err(e) = store.soft_delete_records_by_npub_zone(&del.npub, &del.zone) {
                        tracing::error!(
                            domain = %del.domain, zone = %del.zone, error = %e,
                            "lease expiry task: failed to soft-delete DNS records"
                        );
                    }

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
// Background test record cleanup task
// ---------------------------------------------------------------------------

async fn test_record_cleanup_task(
    store: Arc<Store>,
    updaters: Arc<HashMap<String, Arc<dyn DnsConnector>>>,
    epp_pool: Option<Arc<epp::EppPool>>,
) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
    loop {
        interval.tick().await;

        let delegations = match store.get_test_delegations_expired() {
            Ok(d) => d,
            Err(e) => {
                tracing::error!(error = %e, "test cleanup: query failed");
                continue;
            }
        };

        if delegations.is_empty() {
            continue;
        }

        tracing::info!(
            count = delegations.len(),
            "test cleanup: sweeping expired test records"
        );

        for del in &delegations {
            let full_domain = format!("{}.{}", del.domain, del.zone);
            if let Some(pool) = &epp_pool {
                if pool.is_simulated() {
                    tracing::info!(
                        domain = %full_domain,
                        "test cleanup: SIMULATED EPP domain:delete"
                    );
                } else {
                    match pool.domain_delete(&full_domain).await {
                        Ok(_) => {
                            tracing::info!(domain = %full_domain, "test cleanup: EPP domain:delete succeeded")
                        }
                        Err(e) => tracing::warn!(
                            domain = %full_domain, error = %e,
                            "test cleanup: EPP domain:delete failed (continuing with local cleanup)"
                        ),
                    }
                }
            } else {
                tracing::info!(
                    domain = %full_domain,
                    "test cleanup: no EPP pool — local cleanup only"
                );
            }

            if let Some(updater) = updaters.get(&del.zone) {
                let records = store
                    .get_records_by_npub_exact(&del.npub)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|r| r.zone == del.zone)
                    .collect::<Vec<_>>();

                let mut sent_fqdns: std::collections::HashSet<(String, u16)> =
                    std::collections::HashSet::new();
                for rec in &records {
                    let fqdn = if rec.name == "@" || rec.name.is_empty() {
                        format!("{}.{}.", del.domain, del.zone)
                    } else {
                        format!("{}.{}.{}.", rec.name, del.domain, del.zone)
                    };
                    let rt = crate::types::record_type_to_u16(&rec.record_type);
                    if sent_fqdns.insert((fqdn.clone(), rt)) {
                        if let Err(e) = updater.delete_record(&fqdn, rt).await {
                            tracing::warn!(
                                fqdn = %fqdn, error = %e,
                                "test cleanup: DDNS delete failed (continuing)"
                            );
                        }
                    }
                }
            }

            if let Err(e) = store.soft_delete_records_by_npub_zone(&del.npub, &del.zone) {
                tracing::error!(
                    domain = %del.domain, zone = %del.zone, error = %e,
                    "test cleanup: soft-delete DNS records failed"
                );
                continue;
            }

            if let Err(e) = store.mark_delegation_expired(&del.domain, &del.zone) {
                tracing::error!(
                    domain = %del.domain, zone = %del.zone, error = %e,
                    "test cleanup: mark delegation expired failed"
                );
            } else {
                tracing::info!(
                    domain = %del.domain, zone = %del.zone,
                    "test cleanup: test delegation expired and cleaned up"
                );
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

    if cfg.epp.pool_size > 0 {
        warn!(
            host = %cfg.epp.host,
            port = %cfg.epp.port,
            "EPP module loaded — operations blocked pending registry IP allowlist"
        );
    }

    let epp_pool: Option<Arc<epp::EppPool>> = if cfg.epp.pool_size > 0 {
        let pool = Arc::new(epp::EppPool::new(cfg.epp.clone()).await?);
        info!(host = %cfg.epp.host, simulated = cfg.epp.simulate, "EPP pool constructed");
        Some(pool)
    } else {
        None
    };

    // ── Registrar identity + DNSSEC derivation + Nostr attestation ──
    if !cfg.registrar.nsec_hex.is_empty() && cfg.dnssec_derivation.enabled {
        let nsec_bytes =
            hex::decode(&cfg.registrar.nsec_hex).expect("registrar.nsec_hex must be valid hex");
        assert_eq!(nsec_bytes.len(), 32, "registrar.nsec_hex must be 32 bytes");

        match dnssec_derivation::derive_dnssec_key(&nsec_bytes) {
            Ok(dnssec_key) => {
                let pem = dnssec_key.to_pkcs8_pem().expect("PKCS#8 export");

                let secret =
                    p256::SecretKey::from_bytes((&dnssec_key.private_key_bytes().clone()).into())
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
                let derived_dnskey_b64 = base64_dnskey(&pub_hex);
                let derived_key_tag = compute_key_tag_13(&derived_dnskey_b64);

                let (dnskey_b64, key_tag, attestation_source) = if let Some(first_zone) =
                    cfg.dns.zones.first()
                {
                    let nameserver: SocketAddr = first_zone.knot_address.parse().unwrap_or_else(|_| {
                        warn!(zone = %first_zone.zone, address = %first_zone.knot_address, "invalid knot_address, using derived key");
                        "127.0.0.1:5353".parse().unwrap()
                    });
                    let live_keys =
                        nodns_connectors::dns::query_dnskey_base64(nameserver, &first_zone.zone)
                            .await;
                    let ksk = live_keys.iter().find(|k| {
                        use base64::Engine;
                        if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(k) {
                            !bytes.is_empty() && (bytes[0] & 0x01) != 0
                        } else {
                            false
                        }
                    });

                    if let Some(live_ksk) = ksk {
                        let live_tag = compute_key_tag_13(live_ksk);
                        if live_ksk == &derived_dnskey_b64 {
                            (
                                derived_dnskey_b64.clone(),
                                derived_key_tag,
                                "derived (matches live)",
                            )
                        } else {
                            warn!(
                                zone = %first_zone.zone,
                                derived_key_tag = derived_key_tag,
                                live_key_tag = live_tag,
                                "DNSSEC KSK mismatch: live DNSKEY differs from derived key — attesting live key"
                            );
                            (live_ksk.clone(), live_tag, "live (rollover detected)")
                        }
                    } else {
                        warn!(zone = %first_zone.zone, "no KSK found in live DNS — attesting derived key");
                        (
                            derived_dnskey_b64.clone(),
                            derived_key_tag,
                            "derived (no live KSK found)",
                        )
                    }
                } else {
                    (
                        derived_dnskey_b64.clone(),
                        derived_key_tag,
                        "derived (no zones configured)",
                    )
                };

                info!(zone = %zone, source = attestation_source, key_tag = key_tag, "DNSKEY attestation source");

                let mut tags: Vec<nostr_sdk::Tag> = vec![
                    nostr_sdk::Tag::custom(
                        nostr_sdk::TagKind::custom("dnskey"),
                        [
                            zone.clone(),
                            key_tag.to_string(),
                            "13".to_string(),
                            dnskey_b64,
                        ],
                    ),
                    nostr_sdk::Tag::custom(
                        nostr_sdk::TagKind::custom("dnskey-derivation"),
                        ["slip10".to_string(), "Nist256p1 seed".to_string()],
                    ),
                ];
                for relay in &cfg.nostr.relays {
                    tags.push(nostr_sdk::Tag::custom(
                        nostr_sdk::TagKind::custom("relay"),
                        [relay.clone()],
                    ));
                }

                let builder =
                    nostr_sdk::EventBuilder::new(nostr_sdk::Kind::from(11111u16), "").tags(tags);

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
                        info!(
                            "sent to {} relay(s), failed: {:?}",
                            output.success.len(),
                            output.failed
                        );
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
    let acme_enc_key: Option<String> = if cfg.acme.enabled {
        cfg.acme.encryption_key.clone().or_else(|| {
            warn!("acme.encryption_key not set — generating ephemeral key; encrypted private keys will be unreadable after restart");
            Some(uuid::Uuid::new_v4().to_string())
        })
    } else {
        cfg.acme.encryption_key.clone()
    };
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

    // ── DNS backends ──
    let mut updaters: HashMap<String, Arc<dyn DnsConnector>> = HashMap::new();
    for zc in &cfg.dns.zones {
        let backend: Arc<dyn DnsConnector> = if zc.backend == "cloudflare" {
            let token = zc.cloudflare_api_token.clone().unwrap_or_default();
            let zone_id = zc.cloudflare_zone_id.clone().unwrap_or_default();
            Arc::new(CloudflareBackend::new(token, zone_id))
        } else {
            let primary: Arc<dyn DnsConnector> = {
                let ddns_config = DdnsConfig {
                    knot_address: zc.knot_address.clone(),
                    zone: zc.zone.clone(),
                    tsig_key_name: zc.tsig_key_name.clone(),
                    tsig_key_secret: zc.tsig_key_secret.clone(),
                    tsig_algorithm: zc.tsig_algorithm.clone(),
                };
                Arc::new(Updater::new(&ddns_config)?)
            };
            // Opt-in failover: if Cloudflare credentials are supplied alongside
            // a DDNS backend, wrap the primary so Knot DNS failures fall back
            // to the Cloudflare API automatically (Issue #68).
            let has_cf_creds = zc
                .cloudflare_api_token
                .as_deref()
                .is_some_and(|t| !t.is_empty())
                && zc
                    .cloudflare_zone_id
                    .as_deref()
                    .is_some_and(|z| !z.is_empty());
            if has_cf_creds {
                let token = zc.cloudflare_api_token.clone().unwrap_or_default();
                let zone_id = zc.cloudflare_zone_id.clone().unwrap_or_default();
                let fallback: Arc<dyn DnsConnector> =
                    Arc::new(CloudflareBackend::new(token, zone_id));
                info!(
                    zone = %zc.zone,
                    "DNS failover enabled: Knot DDNS primary → Cloudflare fallback"
                );
                Arc::new(FailoverConnector::new(primary, fallback))
            } else {
                primary
            }
        };
        if let Err(e) = backend.test_connection().await {
            warn!(zone = %zc.zone, backend = %zc.backend, error = %e, "DNS backend connection test failed (will retry on updates)");
        } else {
            info!(zone = %zc.zone, backend = %zc.backend, "DNS backend connection test passed");
        }
        updaters.insert(zc.zone.clone(), backend);
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

    if let Some(ref acme) = acme_service {
        acme.recover_pending_orders().await;
    }

    // ── RFC 2136 DNS UPDATE server ──
    if cfg.dns_update.enabled {
        let listen_addr: SocketAddr =
            cfg.dns_update
                .listen
                .parse()
                .map_err(|e: std::net::AddrParseError| {
                    format!(
                        "invalid dns_update.listen address '{}': {}",
                        cfg.dns_update.listen, e
                    )
                })?;
        let zones = cfg.dns.zones.iter().map(|z| z.zone.clone()).collect();
        match dns_update_server::DnsUpdateServer::new(
            listen_addr,
            store.clone(),
            updaters.clone(),
            zones,
            &cfg.dns_update.tsig_key_name,
            &cfg.dns_update.tsig_key_secret,
        ) {
            Ok(server) => {
                let server = Arc::new(server);
                tokio::spawn(async move {
                    server.run().await;
                });
                info!(listen = %cfg.dns_update.listen, "RFC 2136 DNS UPDATE server started");
            }
            Err(e) => {
                warn!(error = %e, "failed to create DNS UPDATE server — skipping");
            }
        }
    }

    // ── HTTP health server ──
    let nostr_client = nostr_sdk::Client::default();
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
        updaters: updaters.clone(),
        nostr_client: nostr_client.clone(),
        relay_urls: cfg.nostr.relays.clone(),
        db_path: std::path::PathBuf::from(&cfg.store.path),
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
                .expect("governor: invalid API rate limit config"),
        );

        let acme_limit = Arc::new(
            GovernorConfigBuilder::default()
                .key_extractor(SmartIpKeyExtractor)
                .per_second(1)
                .burst_size(3)
                .finish()
                .expect("governor: invalid ACME rate limit config"),
        );

        let acme_routes = axum::Router::new()
            .route(
                "/api/acme/order",
                axum::routing::post(handlers::acme_order_handler),
            )
            .route(
                "/api/acme/order/{id}",
                axum::routing::get(handlers::acme_cert_handler),
            )
            .layer(GovernorLayer::new(acme_limit))
            .with_state(http_state.clone());

        let api_routes = axum::Router::new()
            .route(
                "/.well-known/nostr.json",
                axum::routing::get(nip05::nip05_handler),
            )
            .route("/health", axum::routing::get(handlers::health_handler))
            .route("/llms.txt", axum::routing::get(handlers::llms_txt_handler))
            .route(
                "/llms-full.txt",
                axum::routing::get(handlers::llms_full_txt_handler),
            )
            .route(
                "/api/records",
                axum::routing::get(handlers::records_handler),
            )
            .route(
                "/api/records/by-npub/{npub}",
                axum::routing::get(handlers::records_by_npub_handler),
            )
            .route(
                "/api/records/by-prefix/{prefix}",
                axum::routing::get(handlers::records_by_prefix_handler),
            )
            .route("/api/check", axum::routing::get(handlers::check_handler))
            .route(
                "/api/tls-check",
                axum::routing::get(handlers::tls_check_handler),
            )
            .route(
                "/api/zones/{zone}/pricing",
                axum::routing::get(handlers::zone_pricing_handler),
            )
            .route(
                "/api/zone/{zone}/export",
                axum::routing::get(handlers::zone_export),
            )
            .route(
                "/api/zone/{zone}/records",
                axum::routing::get(handlers::zone_records),
            )
            .route(
                "/nic/update",
                axum::routing::get(handlers::dyndns_update_handler),
            )
            .route(
                "/nic/update",
                axum::routing::post(handlers::dyndns_update_handler),
            )
            .route(
                "/api/client-log",
                axum::routing::post(handlers::client_log_handler),
            )
            .route(
                "/register",
                axum::routing::post(handlers::acmedns_register_handler),
            )
            .route(
                "/update",
                axum::routing::post(handlers::acmedns_update_handler),
            )
            .layer(GovernorLayer::new(api_limit))
            .with_state(http_state);

        let app = axum::Router::new()
            .merge(acme_routes)
            .merge(api_routes)
            .layer(axum::middleware::from_fn(correlation_id_middleware))
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

        let listener = match tokio::net::TcpListener::bind(&bind).await {
            Ok(l) => l,
            Err(e) => {
                tracing::error!(bind = %bind, error = %e, "failed to bind health server");
                return;
            }
        };
        info!(bind = %bind, "health server listening");
        if let Err(e) = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        {
            tracing::error!(error = %e, "health server stopped with error");
        }
    });

    // ── Nostr subscriber ──
    let subscriber = Subscriber::with_client(nostr_client, &cfg.nostr, store.clone());
    let mut event_rx = subscriber.subscribe()?;

    // ── Lease expiry task ──
    {
        let lease_store = store.clone();
        let lease_zones = cfg.dns.zones.clone();
        let lease_updaters = updaters.clone();
        tokio::spawn(async move {
            lease_expiry_task(lease_store, lease_zones, lease_updaters).await;
        });
    }

    // ── Test record cleanup task ──
    {
        let tc_store = store.clone();
        let tc_updaters = updaters.clone();
        let tc_epp_pool = epp_pool.clone();
        tokio::spawn(async move {
            test_record_cleanup_task(tc_store, tc_updaters, tc_epp_pool).await;
        });
    }

    // ── Event loop ──
    let ev_store = store.clone();
    let ev_auth = authority;
    let ev_updaters = updaters;
    let ev_zone_verifiers = zone_verifiers;
    let ev_cfg = cfg;
    let ev_metrics = app_state.clone();
    let ev_epp_pool = epp_pool.clone();

    tokio::spawn(async move {
        loop {
            if let Some(evt) = event_rx.recv().await {
                event_processor::process_nostr_event(
                    &evt,
                    &ev_cfg,
                    &ev_updaters,
                    &ev_store,
                    &ev_auth,
                    &ev_zone_verifiers,
                    &ev_metrics.metrics,
                    ev_epp_pool.as_deref(),
                )
                .await;
            } else {
                info!("event channel closed");
                break;
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
    let dnskey = base64::engine::general_purpose::STANDARD
        .decode(dnskey_b64)
        .expect("valid base64");
    let sum: u32 = dnskey
        .chunks(2)
        .map(|chunk| {
            if chunk.len() == 2 {
                u32::from(u16::from_be_bytes([chunk[0], chunk[1]]))
            } else {
                u32::from(chunk[0])
            }
        })
        .sum();
    ((sum + (sum >> 16)) & 0xFFFF) as u16
}
