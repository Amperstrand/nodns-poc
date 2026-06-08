use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use nostr_sdk::nips::nip19::ToBech32;
use nostr_sdk::Event;
use tracing::{error, info, warn};

use crate::auth;
use crate::config::Config;
use crate::dns::Updater;
use crate::payment;
use crate::payment::Verifier;
use crate::parser;
use crate::store::Store;
use crate::types::{Metrics, ParsedEvent, Delegation, RegistrarKey, build_fqdn, record_type_to_u16};

// ---------------------------------------------------------------------------
// Event processing
// ---------------------------------------------------------------------------

pub async fn process_nostr_event(
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
    }
    if let Some(ref registrar) = parsed.registrar {
        process_registrar(registrar, &event_id, &pubkey_hex, &npub, created_at, store, authority, metrics);
    }
    if !parsed.records.is_empty() {
        process_dns_update(
            &parsed, &event_id, &pubkey_hex, &npub, created_at,
            cfg, updaters, store, authority, verifier, metrics,
        ).await;
    }

    if !parsed.deletes.is_empty() {
        process_dns_deletes(
            &parsed, &event_id, &pubkey_hex, &npub,
            updaters, store, authority, metrics,
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

// ---------------------------------------------------------------------------
// process_dns_deletes
// ---------------------------------------------------------------------------

async fn process_dns_deletes(
    parsed: &ParsedEvent,
    event_id: &str,
    pubkey_hex: &str,
    npub: &str,
    updaters: &Arc<HashMap<String, Updater>>,
    store: &Arc<Store>,
    authority: &auth::AuthorityChecker,
    metrics: &Metrics,
) {
    for (zone_name, updater) in updaters.iter() {
        for del in &parsed.deletes {
            let fqdn = build_fqdn(npub, &del.name, zone_name);

            if let Err(e) = authority.check_authority(&fqdn, zone_name, pubkey_hex) {
                warn!(event_id = %event_id, fqdn = %fqdn, error = %e, "delete authority check failed");
                continue;
            }

            let rt = record_type_to_u16(&del.record_type);

            if let Err(e) = updater.delete_record(&fqdn, rt).await {
                error!(event_id = %event_id, fqdn = %fqdn, r#type = %del.record_type, error = %e, "DDNS delete failed");
                metrics.ddns_failures.fetch_add(1, Ordering::Relaxed);
                continue;
            }

            info!(event_id = %event_id, fqdn = %fqdn, r#type = %del.record_type, "DDNS delete applied");
            metrics.ddns_successes.fetch_add(1, Ordering::Relaxed);

            if let Err(e) = store.delete_records_by_key(npub, &del.record_type, &del.name, zone_name) {
                error!(event_id = %event_id, error = %e, "failed to mark records deleted");
            }
        }
    }
    metrics.events_processed.fetch_add(1, Ordering::Relaxed);
}
