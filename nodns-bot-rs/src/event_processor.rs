use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use nostr_sdk::nips::nip19::ToBech32;
use nostr_sdk::Event;
use tracing::{debug, error, info, warn};

use crate::auth;
use crate::config::Config;
use crate::dns::Updater;
use crate::parser;
use crate::payment;
use crate::payment::Verifier;
use crate::store::Store;
use crate::types::{
    build_fqdn, record_type_to_u16, ClaimRequest, Delegation, Metrics, ParsedEvent, RegistrarKey,
    RenewalRequest,
};

// ---------------------------------------------------------------------------
// FQDN resolution
// ---------------------------------------------------------------------------

/// Resolve the correct FQDN for a record, accounting for delegated custom names.
///
/// If the npub has an active delegation in this zone, use the delegated domain
/// as the owner label (e.g., `alice.nodns.shop.` instead of `npub1xxx.nodns.shop.`).
/// Falls back to `build_fqdn` for non-delegated (npub-based) names.
pub(crate) fn resolve_fqdn(npub: &str, name: &str, zone: &str, store: &Arc<Store>) -> String {
    if let Ok(delegations) = store.get_delegations_by_pubkey(npub) {
        if let Some(del) = delegations.iter().find(|d| d.zone == zone) {
            if name == "@" || name.is_empty() {
                return format!("{}.{}.", del.domain, zone);
            }
            return format!("{}.{}.{}.", name, del.domain, zone);
        }
    }
    build_fqdn(npub, name, zone)
}

// ---------------------------------------------------------------------------
// Operator lease expiry helper
// ---------------------------------------------------------------------------

/// Parse a date string "YYYY-MM-DD" into a Unix timestamp (start of day, UTC).
/// Returns `None` if the field is unset or malformed.
fn parse_operator_expiry(date_str: &Option<String>) -> Option<i64> {
    let s = date_str.as_ref()?;
    let parts: Vec<&str> = s.trim().split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let year: i64 = parts[0].parse().ok()?;
    let month: i64 = parts[1].parse().ok()?;
    let day: i64 = parts[2].parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // Howard Hinnant's civil-from-days algorithm.
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let m_adj = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * m_adj + 2) / 5 + day - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    let days_since_epoch = era * 146_097 + doe - 719_468;
    Some(days_since_epoch * 86_400)
}

// ---------------------------------------------------------------------------
// Event processing
// ---------------------------------------------------------------------------

pub async fn process_nostr_event(
    evt: &Event,
    cfg: &Config,
    updaters: &Arc<HashMap<String, Updater>>,
    store: &Arc<Store>,
    authority: &auth::AuthorityChecker,
    zone_verifiers: &HashMap<String, Verifier>,
    metrics: &Metrics,
    epp_pool: Option<&crate::epp::EppPool>,
) {
    let event_id = evt.id.to_hex();
    let pubkey_hex = evt.pubkey.to_hex();
    let created_at = evt.created_at.as_secs() as i64;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    metrics.last_event_at.store(now, Ordering::Relaxed);

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

    if let Some(ref claim) = parsed.claim {
        process_claim(
            claim,
            &parsed.payments,
            &event_id,
            &pubkey_hex,
            &npub,
            created_at,
            cfg,
            store,
            authority,
            zone_verifiers,
            metrics,
        )
        .await;
    }

    if let Some(ref renewal) = parsed.renewal {
        process_renewal(
            renewal,
            &parsed.payments,
            &event_id,
            &pubkey_hex,
            &npub,
            created_at,
            cfg,
            store,
            zone_verifiers,
            metrics,
        )
        .await;
    }

    if let Some(ref delegation) = parsed.delegation {
        process_delegation(
            delegation,
            &event_id,
            &pubkey_hex,
            &npub,
            created_at,
            cfg,
            store,
            authority,
            metrics,
        );
    }
    if let Some(ref registrar) = parsed.registrar {
        process_registrar(
            registrar,
            &event_id,
            &pubkey_hex,
            &npub,
            created_at,
            store,
            authority,
            metrics,
        );
    }
    if !parsed.records.is_empty() {
        process_dns_update(
            &parsed,
            &event_id,
            &pubkey_hex,
            &npub,
            created_at,
            cfg,
            updaters,
            store,
            authority,
            zone_verifiers,
            metrics,
            epp_pool,
        )
        .await;
    }

    if !parsed.deletes.is_empty() {
        process_dns_deletes(
            &parsed,
            &event_id,
            &pubkey_hex,
            &npub,
            cfg,
            updaters,
            store,
            authority,
            metrics,
        )
        .await;
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

    let matched_zone = zones
        .iter()
        .find(|z| domain.ends_with(&format!(".{z}")) || domain == **z);

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
        event_id,
        domain_name,
        zone,
        &delegation.npub,
        pubkey_hex,
        delegation.valid_from,
        delegation.valid_until,
        delegation.renew_by,
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

    if let Err(e) = store.save_registrar_key(
        &registrar.zone,
        &registrar.pubkey_hex,
        npub,
        "nostr",
        event_id,
    ) {
        error!(event_id = %event_id, error = %e, "failed to save registrar key");
        metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
        return;
    }

    info!(event_id = %event_id, zone = %registrar.zone, "registrar key processed");
    metrics.events_processed.fetch_add(1, Ordering::Relaxed);
}

// ---------------------------------------------------------------------------
// process_claim
// ---------------------------------------------------------------------------

fn is_npub_name(name: &str) -> bool {
    name.starts_with("npub1")
}

/// Length-based registration price for $string names.
fn registration_price(name: &str, base_price: u64) -> u64 {
    match name.len() {
        1..=3 => base_price * 100,
        4..=6 => base_price * 10,
        _ => base_price * 2,
    }
}

async fn process_claim(
    claim: &ClaimRequest,
    payments: &[crate::types::Payment],
    event_id: &str,
    pubkey_hex: &str,
    npub: &str,
    _created_at: i64,
    cfg: &Config,
    store: &Arc<Store>,
    authority: &auth::AuthorityChecker,
    zone_verifiers: &HashMap<String, Verifier>,
    metrics: &Metrics,
) {
    let zone_name = &claim.zone;

    let zone_config = cfg.dns.zones.iter().find(|z| z.zone == *zone_name);
    let Some(zone_config) = zone_config else {
        warn!(event_id = %event_id, zone = %zone_name, "claim zone not found in config");
        metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
        return;
    };

    if is_npub_name(&claim.name) {
        info!(event_id = %event_id, name = %claim.name, "npub claim detected, skipping (not yet implemented)");
        metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
        return;
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    if claim.valid_until <= now {
        warn!(event_id = %event_id, valid_until = claim.valid_until, now, "claim valid_until must be in the future");
        metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
        return;
    }

    if let Some(operator_expiry) = parse_operator_expiry(&zone_config.lease.operator_lease_expires)
    {
        if claim.valid_until > operator_expiry {
            warn!(
                event_id = %event_id,
                valid_until = claim.valid_until,
                operator_lease_expires = operator_expiry,
                "claim valid_until exceeds operator's own domain lease expiry"
            );
            metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
            return;
        }
    }

    match store.is_name_available(&claim.name, zone_name) {
        Ok(true) => {}
        Ok(false) => {
            warn!(event_id = %event_id, name = %claim.name, zone = %zone_name, "name is not available");
            metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
            return;
        }
        Err(e) => {
            error!(event_id = %event_id, error = %e, "failed to check name availability");
            metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
            return;
        }
    }

    let base_price = zone_config.payment.create_price;
    let price = registration_price(&claim.name, base_price) as i64;

    if let Some(verifier) = zone_verifiers.get(zone_name) {
        if price > 0 {
            let total_paid: i64 = payments
                .iter()
                .filter(|p| p.method == "cashu")
                .map(|p| p.amount)
                .sum();

            if total_paid < price {
                warn!(
                    event_id = %event_id,
                    name = %claim.name,
                    zone = %zone_name,
                    required = price,
                    paid = total_paid,
                    "insufficient payment for claim"
                );
                metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
                return;
            }

            let mut verified_total: i64 = 0;
            for p in payments.iter().filter(|p| p.method == "cashu") {
                let remaining = price - verified_total;
                match verifier.verify_payment(&p.token, remaining).await {
                    Ok(amount) => verified_total += amount as i64,
                    Err(e) => {
                        warn!(event_id = %event_id, error = %e, "cashu verification failed for claim");
                    }
                }
                if verified_total >= price {
                    break;
                }
            }

            if verified_total < price {
                warn!(
                    event_id = %event_id,
                    required = price,
                    verified = verified_total,
                    "claim payment verification failed"
                );
                metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
                return;
            }
        }
    }

    let Some(registrar_pubkey) = authority.get_registrar_pubkey(zone_name) else {
        warn!(event_id = %event_id, zone = %zone_name, "no registrar key configured for zone");
        metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
        return;
    };

    let valid_from = now;
    let valid_until = claim.valid_until;
    let renew_by = valid_until - (30 * 24 * 3600);

    if let Err(e) = store.save_delegation_with_price(
        event_id,
        &claim.name,
        zone_name,
        npub,
        pubkey_hex,
        valid_from,
        valid_until,
        renew_by,
        &registrar_pubkey,
        base_price as i64,
    ) {
        error!(event_id = %event_id, error = %e, "failed to save claim delegation");
        metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
        return;
    }

    info!(
        event_id = %event_id,
        name = %claim.name,
        zone = %zone_name,
        npub = %npub,
        price = price,
        renewal_price = base_price,
        "claim processed — delegation created"
    );
    metrics.events_processed.fetch_add(1, Ordering::Relaxed);
}

// ---------------------------------------------------------------------------
// process_renewal
// ---------------------------------------------------------------------------

async fn process_renewal(
    renewal: &RenewalRequest,
    payments: &[crate::types::Payment],
    event_id: &str,
    pubkey_hex: &str,
    npub: &str,
    created_at: i64,
    cfg: &Config,
    store: &Arc<Store>,
    zone_verifiers: &HashMap<String, Verifier>,
    metrics: &Metrics,
) {
    let zone_name = &renewal.zone;

    let zone_config = cfg.dns.zones.iter().find(|z| z.zone == *zone_name);
    let Some(zone_config) = zone_config else {
        warn!(event_id = %event_id, zone = %zone_name, "renewal zone not found in config");
        metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
        return;
    };

    let delegation = match store.get_delegation(&renewal.name, zone_name) {
        Ok(Some(d)) => d,
        Ok(None) => {
            warn!(
                event_id = %event_id,
                name = %renewal.name,
                zone = %zone_name,
                "No delegation found for {}.{}", renewal.name, zone_name
            );
            metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
            return;
        }
        Err(e) => {
            error!(event_id = %event_id, error = %e, "failed to look up delegation for renewal");
            metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
            return;
        }
    };

    if delegation.pubkey != pubkey_hex {
        warn!(
            event_id = %event_id,
            delegation_pubkey = %delegation.pubkey,
            signer_pubkey = %pubkey_hex,
            "Only the delegation owner can renew"
        );
        metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
        return;
    }

    let grace_period_secs = i64::from(zone_config.lease.grace_period_days) * 24 * 3600;
    let grace_deadline = delegation.valid_until + grace_period_secs;
    if created_at >= grace_deadline {
        warn!(
            event_id = %event_id,
            created_at = created_at,
            grace_deadline = grace_deadline,
            "Delegation expired — name available for re-registration"
        );
        metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
        return;
    }

    let max_extension_secs = i64::from(zone_config.lease.max_lease_days) * 24 * 3600;
    let max_valid_until = delegation.valid_until + max_extension_secs;
    let base_valid_until = created_at + max_extension_secs;
    let effective_max = max_valid_until.max(base_valid_until);

    if renewal.new_valid_until <= delegation.valid_until {
        warn!(
            event_id = %event_id,
            new_valid_until = renewal.new_valid_until,
            current_valid_until = delegation.valid_until,
            "Invalid renewal period: new_valid_until must extend current lease"
        );
        metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
        return;
    }

    if renewal.new_valid_until > effective_max {
        warn!(
            event_id = %event_id,
            new_valid_until = renewal.new_valid_until,
            max_valid_until = effective_max,
            "Invalid renewal period: exceeds max lease duration"
        );
        metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
        return;
    }

    if let Some(operator_expiry) = parse_operator_expiry(&zone_config.lease.operator_lease_expires)
    {
        if renewal.new_valid_until > operator_expiry {
            warn!(
                event_id = %event_id,
                new_valid_until = renewal.new_valid_until,
                operator_lease_expires = operator_expiry,
                "renewal exceeds operator's own domain lease expiry"
            );
            metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
            return;
        }
    }

    let required_price = delegation.renewal_price;
    if required_price > 0 {
        if let Some(verifier) = zone_verifiers.get(zone_name) {
            let total_paid: i64 = payments
                .iter()
                .filter(|p| p.method == "cashu")
                .map(|p| p.amount)
                .sum();

            if total_paid < required_price {
                warn!(
                    event_id = %event_id,
                    required = required_price,
                    paid = total_paid,
                    "Payment of {} sats required, got {}", required_price, total_paid
                );
                metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
                return;
            }

            let mut verified_total: i64 = 0;
            for p in payments.iter().filter(|p| p.method == "cashu") {
                let remaining = required_price - verified_total;
                match verifier.verify_payment(&p.token, remaining).await {
                    Ok(amount) => verified_total += amount as i64,
                    Err(e) => {
                        warn!(event_id = %event_id, error = %e, "cashu verification failed for renewal");
                    }
                }
                if verified_total >= required_price {
                    break;
                }
            }

            if verified_total < required_price {
                warn!(
                    event_id = %event_id,
                    required = required_price,
                    verified = verified_total,
                    "Renewal payment verification failed"
                );
                metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
                return;
            }
        }
    }

    let new_renew_by = renewal.new_valid_until - grace_period_secs;

    if let Err(e) = store.renew_delegation(
        &renewal.name,
        zone_name,
        renewal.new_valid_until,
        new_renew_by,
        event_id,
    ) {
        error!(event_id = %event_id, error = %e, "failed to renew delegation");
        metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
        return;
    }

    info!(
        event_id = %event_id,
        name = %renewal.name,
        zone = %zone_name,
        npub = %npub,
        new_valid_until = renewal.new_valid_until,
        "renewal processed — delegation extended"
    );
    metrics.events_processed.fetch_add(1, Ordering::Relaxed);
}

// ---------------------------------------------------------------------------
// Proof-of-record TXT builder
// ---------------------------------------------------------------------------

fn build_proof_txt(
    event_id: &str,
    pubkey_hex: &str,
    sig: &str,
    created_at: i64,
    record_type: &str,
    record_name: &str,
    record_rdata: &str,
) -> Vec<String> {
    let proof = format!(
        "nostr:v=1;k=11111;p={pubkey_hex};s={sig};i={event_id};ts={created_at};rt={record_type};rn={record_name};rd={record_rdata}"
    );
    proof
        .as_bytes()
        .chunks(255)
        .map(|chunk| String::from_utf8_lossy(chunk).to_string())
        .collect()
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
    zone_verifiers: &HashMap<String, Verifier>,
    metrics: &Metrics,
    epp_pool: Option<&crate::epp::EppPool>,
) {
    if parsed.records.is_empty() {
        return;
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
        // Per-zone payment verification: skip this zone if payment fails,
        // but allow other zones to proceed independently.
        if let Some(v) = zone_verifiers.get(zone_name) {
            if let Err(e) = payment::check_event_payment(
                &parsed.payments,
                npub,
                &parsed.records,
                zone_name,
                store,
                Some(v),
            )
            .await
            {
                warn!(event_id = %event_id, zone = %zone_name, error = %e, "payment verification failed, skipping zone");
                metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
                all_ok = false;
                continue;
            }
        }

        if zone_name == "cv" || zone_name.ends_with(".cv") {
            let domain = format!("{npub}.{zone_name}");
            if let Some(pool) = epp_pool {
                if pool.is_simulated() {
                    info!(
                        event_id = %event_id, domain = %domain, zone = %zone_name,
                        "SIMULATED: would send EPP domain:create — skipping Cashu claim"
                    );
                } else {
                    match pool.domain_create(&domain, 1, &[], "", "").await {
                        Ok(_) => {
                            info!(event_id = %event_id, domain = %domain, "EPP domain:create succeeded");
                            if let Some(p) = parsed.payments.first() {
                                if let Err(e) =
                                    crate::payment::claim_payment(&p.token, &p.mint_url).await
                                {
                                    warn!(
                                        event_id = %event_id, domain = %domain, error = %e,
                                        "Cashu claim failed — domain registered but payment not collected (manual reconciliation)"
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            warn!(
                                event_id = %event_id, domain = %domain, error = %e,
                                "EPP domain:create failed — NOT claiming Cashu (user keeps tokens), skipping zone"
                            );
                            metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
                            all_ok = false;
                            continue;
                        }
                    }
                }
            }
        }

        for rec in &parsed.records {
            let fqdn = resolve_fqdn(npub, &rec.name, zone_name, store);

            if zone_name == "cv" || zone_name.ends_with(".cv") {
                let clean = fqdn.trim_end_matches('.');
                let name_class = crate::classify::classify_name(clean);
                if matches!(name_class, crate::classify::NameClass::Custom) {
                    warn!(event_id = %event_id, fqdn = %fqdn, "cv zone: pilot gate rejected — custom names not yet allowed");
                    metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
                    all_ok = false;
                    continue;
                }
                let mint_url = parsed
                    .payments
                    .first()
                    .map(|p| p.mint_url.as_str())
                    .unwrap_or("");
                let mint_class = crate::classify::classify_payment(mint_url, &[], &[]);
                if !crate::classify::allowed_combination(name_class, mint_class) {
                    warn!(
                        event_id = %event_id, fqdn = %fqdn,
                        name_class = %name_class.as_str(), mint_class = %mint_class.as_str(),
                        "cv zone: disallowed name+payment combination"
                    );
                    metrics.events_rejected.fetch_add(1, Ordering::Relaxed);
                    all_ok = false;
                    continue;
                }
            }

            if let Err(e) = authority.check_authority(&fqdn, zone_name, pubkey_hex) {
                warn!(event_id = %event_id, fqdn = %fqdn, error = %e, "authority check failed");
                all_ok = false;
                continue;
            }

            let is_cv = zone_name == "cv" || zone_name.ends_with(".cv");
            if is_cv && epp_pool.is_some_and(|p| p.is_simulated()) {
                debug!(event_id = %event_id, fqdn = %fqdn, "cv zone in simulate mode — skipping DDNS (no per-domain zone in Knot)");
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

            let store_proofs = cfg
                .dns
                .zones
                .iter()
                .find(|z| z.zone == *zone_name)
                .map(|z| z.store_proofs)
                .unwrap_or(true);
            if store_proofs {
                let proof_fqdn = format!("_nodns.{fqdn}");
                let proof_segments = build_proof_txt(
                    event_id,
                    pubkey_hex,
                    &parsed.sig,
                    created_at,
                    &rec.record_type,
                    &rec.name,
                    &rec.rdata,
                );
                if let Err(e) = updater
                    .update_txt_multi(&proof_fqdn, rec.ttl, &proof_segments)
                    .await
                {
                    warn!(event_id = %event_id, fqdn = %proof_fqdn, error = %e, "proof TXT update failed (non-fatal)");
                }
            }

            if let Err(e) = store.save_event(
                event_id,
                npub,
                pubkey_hex,
                &rec.name,
                &rec.record_type,
                rec.ttl,
                &rec.rdata,
                zone_name,
                created_at,
            ) {
                error!(event_id = %event_id, error = %e, "failed to save event");
            }
        }
    }

    if all_ok {
        metrics.events_processed.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_npub_name_true() {
        assert!(is_npub_name("npub1abc123"));
    }

    #[test]
    fn test_is_npub_name_false() {
        assert!(!is_npub_name("alice"));
        assert!(!is_npub_name("NPUB1abc"));
        assert!(!is_npub_name(""));
    }

    #[test]
    fn test_registration_price_short_names() {
        assert_eq!(registration_price("ab", 2), 200);
        assert_eq!(registration_price("abc", 2), 200);
    }

    #[test]
    fn test_registration_price_medium_names() {
        assert_eq!(registration_price("abcd", 2), 20);
        assert_eq!(registration_price("abcdef", 2), 20);
    }

    #[test]
    fn test_registration_price_long_names() {
        assert_eq!(registration_price("abcdefg", 2), 4);
        assert_eq!(registration_price("verylongname", 2), 4);
    }

    #[test]
    fn test_registration_price_with_different_base() {
        assert_eq!(registration_price("ab", 5), 500);
        assert_eq!(registration_price("abcd", 5), 50);
        assert_eq!(registration_price("abcdefg", 5), 10);
    }

    #[test]
    fn test_registration_price_single_char() {
        assert_eq!(registration_price("a", 2), 200);
    }

    #[test]
    fn parse_operator_expiry_none_when_unset() {
        assert_eq!(parse_operator_expiry(&None), None);
        assert_eq!(parse_operator_expiry(&Some(String::new())), None);
    }

    #[test]
    fn parse_operator_expiry_epoch() {
        assert_eq!(
            parse_operator_expiry(&Some("1970-01-01".to_string())),
            Some(0)
        );
    }

    #[test]
    fn parse_operator_expiry_known_date() {
        assert_eq!(
            parse_operator_expiry(&Some("2027-06-08".to_string())),
            Some(1812412800)
        );
    }

    #[test]
    fn parse_operator_expiry_malformed() {
        assert_eq!(parse_operator_expiry(&Some("not-a-date".to_string())), None);
        assert_eq!(parse_operator_expiry(&Some("2027".to_string())), None);
        assert_eq!(parse_operator_expiry(&Some("2027-13-01".to_string())), None);
        assert_eq!(parse_operator_expiry(&Some("2027-06-32".to_string())), None);
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
    cfg: &Config,
    updaters: &Arc<HashMap<String, Updater>>,
    store: &Arc<Store>,
    authority: &auth::AuthorityChecker,
    metrics: &Metrics,
) {
    for (zone_name, updater) in updaters.iter() {
        let store_proofs = cfg
            .dns
            .zones
            .iter()
            .find(|z| z.zone == *zone_name)
            .map(|z| z.store_proofs)
            .unwrap_or(true);

        for del in &parsed.deletes {
            let fqdn = resolve_fqdn(npub, &del.name, zone_name, store);

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

            if store_proofs {
                let proof_fqdn = format!("_nodns.{fqdn}");
                if let Err(e) = updater.delete_record(&proof_fqdn, 16).await {
                    warn!(fqdn = %proof_fqdn, error = %e, "proof TXT delete failed (non-fatal)");
                }
            }

            if let Err(e) =
                store.delete_records_by_key(npub, &del.record_type, &del.name, zone_name)
            {
                error!(event_id = %event_id, error = %e, "failed to mark records deleted");
            }
        }
    }
    metrics.events_processed.fetch_add(1, Ordering::Relaxed);
}
