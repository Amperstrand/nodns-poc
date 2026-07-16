//! NIP-17 payment processor — receives Cashu payments via encrypted DMs
//! and creates DNS name delegations for custom subdomains.
//!
//! Phase 3 of the two-component architecture split (docs/45).
//! Payment is out-of-band from kind 11111 events (locked decision).

use std::collections::HashMap;
use std::sync::Arc;

use nostr_sdk::{
    Client, Event, EventBuilder, Filter, Kind, PublicKey, RelayPoolNotification, Tag, ToBech32,
};
use serde::Deserialize;
use tracing::{error, info, warn};

use crate::config::Config;
use crate::payment::{PaymentError, Verifier};
use crate::store::Store;

const KIND_GIFT_WRAP: u16 = 1059;

#[derive(Deserialize)]
struct PaymentDm {
    #[serde(rename = "type")]
    msg_type: String,
    name: String,
    zone: String,
    #[serde(default)]
    duration_days: Option<u32>,
    token: String,
}

pub struct PaymentProcessor {
    client: Client,
    config: Arc<Config>,
    store: Arc<Store>,
    verifiers: HashMap<String, Verifier>,
}

impl PaymentProcessor {
    pub fn new(client: Client, config: Arc<Config>, store: Arc<Store>) -> Self {
        let mut verifiers = HashMap::new();
        for zc in &config.dns.zones {
            if zc.payment.enabled {
                verifiers.insert(zc.zone.clone(), Verifier::from_zone_config(&zc.payment));
            }
        }
        Self {
            client,
            config,
            store,
            verifiers,
        }
    }

    pub async fn run(&self) {
        if self.config.registrar.nsec_hex.is_empty() {
            warn!("payment processor: registrar.nsec_hex not set — DM listener disabled");
            return;
        }

        if self.verifiers.is_empty() {
            warn!("payment processor: no zones with payment enabled — DM listener disabled");
            return;
        }

        let keys = match nostr_sdk::Keys::parse(&self.config.registrar.nsec_hex) {
            Ok(k) => k,
            Err(e) => {
                error!(error = %e, "payment processor: invalid registrar nsec");
                return;
            }
        };
        let registrar_pk = keys.public_key();

        let filter = Filter::new()
            .kind(Kind::Custom(KIND_GIFT_WRAP))
            .pubkey(registrar_pk);

        if let Err(e) = self.client.subscribe(filter, None).await {
            error!(error = %e, "payment processor: subscription failed");
            return;
        }

        info!(
            registrar_pubkey = %registrar_pk.to_hex(),
            zones = ?self.verifiers.keys().collect::<Vec<_>>(),
            "payment processor: listening for NIP-17 payment DMs"
        );

        let store = self.store.clone();
        let config = self.config.clone();
        let verifiers = self.verifiers.clone();
        let client = self.client.clone();

        let _ = client
            .handle_notifications(|notification| {
                let store = store.clone();
                let config = config.clone();
                let verifiers = verifiers.clone();
                let client = client.clone();

                async move {
                    if let RelayPoolNotification::Event { event, .. } = notification {
                        if event.kind.as_u16() != KIND_GIFT_WRAP {
                            return Ok(false);
                        }

                        let event_id = event.id.to_hex();
                        if store.is_dm_processed(&event_id).unwrap_or(false) {
                            tracing::debug!(dm_id = %event_id, "DM already processed, skipping");
                            return Ok(false);
                        }

                        Box::pin(process_gift_wrap(
                            &event, &client, &store, &config, &verifiers,
                        ))
                        .await;
                    }
                    Ok(false)
                }
            })
            .await;
    }
}

async fn process_gift_wrap(
    gift_wrap: &Event,
    client: &Client,
    store: &Arc<Store>,
    config: &Config,
    verifiers: &HashMap<String, Verifier>,
) {
    let gw_id = gift_wrap.id.to_hex();

    let unwrapped = match client.unwrap_gift_wrap(gift_wrap).await {
        Ok(u) => u,
        Err(e) => {
            warn!(gift_wrap_id = %gw_id, error = %e, "failed to unwrap gift wrap");
            return;
        }
    };

    let sender = unwrapped.sender;
    let sender_hex = sender.to_hex();
    let sender_npub = sender.to_bech32().unwrap_or_else(|_| sender_hex.clone());

    let dm: PaymentDm = match serde_json::from_str(&unwrapped.rumor.content) {
        Ok(dm) => dm,
        Err(e) => {
            let msg = format!(
                "Invalid DM format: {e}. Expected JSON: {{\"type\":\"register\",\"name\":\"alice\",\"zone\":\"nodns.shop\",\"token\":\"cashuA...\",\"duration_days\":365}}"
            );
            warn!(sender = %sender_npub, error = %e, "invalid payment DM");
            send_reply(client, config, &sender, &msg).await;
            mark_processed(store, &gw_id, &sender_npub, "?", "?", 0, "invalid_json");
            return;
        }
    };

    if dm.msg_type != "register" {
        let msg = format!("Unknown DM type '{}'. Supported: 'register'", dm.msg_type);
        send_reply(client, config, &sender, &msg).await;
        mark_processed(
            store,
            &gw_id,
            &sender_npub,
            &dm.name,
            &dm.zone,
            0,
            "unknown_type",
        );
        return;
    }

    let name = dm.name.trim().to_lowercase();
    let zone = dm.zone.trim().to_lowercase();

    if let Err(reason) = validate_name(&name) {
        send_reply(client, config, &sender, &reason).await;
        mark_processed(store, &gw_id, &sender_npub, &name, &zone, 0, "invalid_name");
        return;
    }

    let Some(zone_config) = config.dns.zones.iter().find(|z| z.zone == zone) else {
        let msg = format!("Zone '{}' is not served by this registrar.", zone);
        send_reply(client, config, &sender, &msg).await;
        mark_processed(store, &gw_id, &sender_npub, &name, &zone, 0, "unknown_zone");
        return;
    };

    if !zone_config.payment.enabled {
        let msg = format!(
            "Zone '{}' does not require payment. Publish a kind 11111 event directly.",
            zone
        );
        send_reply(client, config, &sender, &msg).await;
        mark_processed(
            store,
            &gw_id,
            &sender_npub,
            &name,
            &zone,
            0,
            "payment_disabled",
        );
        return;
    }

    let Some(verifier) = verifiers.get(&zone) else {
        let msg = format!("No payment verifier configured for zone '{}'.", zone);
        send_reply(client, config, &sender, &msg).await;
        mark_processed(store, &gw_id, &sender_npub, &name, &zone, 0, "no_verifier");
        return;
    };

    let fqdn = format!("{}.{}", name, zone);
    if !store.is_name_available(&name, &zone).unwrap_or(false) {
        let msg = format!("'{}' is already registered or in grace period.", fqdn);
        send_reply(client, config, &sender, &msg).await;
        mark_processed(store, &gw_id, &sender_npub, &name, &zone, 0, "name_taken");
        return;
    }

    let duration_days = dm
        .duration_days
        .unwrap_or(zone_config.lease.max_lease_days)
        .min(zone_config.lease.max_lease_days)
        .max(1);

    let required_sats = zone_config.payment.create_price as i64;

    let verified_amount = match verifier.verify_payment(&dm.token, required_sats).await {
        Ok(amount) => amount,
        Err(e) => {
            let reason = format_payment_error(&e, required_sats, &fqdn);
            warn!(sender = %sender_npub, name = %fqdn, error = %e, "payment verification failed");
            send_reply(client, config, &sender, &reason).await;
            mark_processed(
                store,
                &gw_id,
                &sender_npub,
                &name,
                &zone,
                0,
                "payment_failed",
            );
            return;
        }
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let valid_from = now;
    let valid_until = now + (duration_days as i64) * 86400;
    let renew_by = valid_until - (zone_config.lease.grace_period_days as i64) * 86400;

    let registrar_hex = match nostr_sdk::Keys::parse(&config.registrar.nsec_hex) {
        Ok(k) => k.public_key().to_hex(),
        Err(_) => {
            error!("registrar nsec became invalid during processing");
            send_reply(
                client,
                config,
                &sender,
                "Internal error: registrar key unavailable. Please contact the operator.",
            )
            .await;
            mark_processed(
                store,
                &gw_id,
                &sender_npub,
                &name,
                &zone,
                0,
                "internal_error",
            );
            return;
        }
    };

    if let Err(e) = store.save_delegation_with_price(
        &gw_id,
        &name,
        &zone,
        &sender_npub,
        &sender_hex,
        valid_from,
        valid_until,
        renew_by,
        &registrar_hex,
        required_sats,
    ) {
        error!(error = %e, name = %fqdn, "failed to save delegation");
        send_reply(
            client,
            config,
            &sender,
            "Internal error: could not save delegation. Your payment was verified — please contact the operator.",
        )
        .await;
        mark_processed(
            store,
            &gw_id,
            &sender_npub,
            &name,
            &zone,
            verified_amount as i64,
            "store_error",
        );
        return;
    }

    publish_delegation_event(
        client,
        config,
        &name,
        &zone,
        &sender_npub,
        &sender_hex,
        valid_from,
        valid_until,
        renew_by,
    )
    .await;

    let duration_months = duration_days / 30;
    let confirmation = format!(
        "Confirmed: {}.{} registered for {} days ({} months).\n\
         Payment: {} sats verified.\n\
         Valid until: {} (renew by {}).",
        name,
        zone,
        duration_days,
        duration_months,
        verified_amount,
        format_unix_date(valid_until),
        format_unix_date(renew_by),
    );

    info!(
        sender = %sender_npub,
        name = %fqdn,
        amount = verified_amount,
        duration_days,
        "payment processed: delegation created"
    );

    send_reply(client, config, &sender, &confirmation).await;
    mark_processed(
        store,
        &gw_id,
        &sender_npub,
        &name,
        &zone,
        verified_amount as i64,
        "confirmed",
    );
}

async fn publish_delegation_event(
    client: &Client,
    config: &Config,
    name: &str,
    zone: &str,
    sender_npub: &str,
    _sender_hex: &str,
    valid_from: i64,
    valid_until: i64,
    renew_by: i64,
) {
    let fqdn = format!("{}.{}", name, zone);

    let delegation_tag = Tag::custom(
        nostr_sdk::TagKind::custom("delegation"),
        [
            fqdn.clone(),
            sender_npub.to_string(),
            valid_from.to_string(),
            valid_until.to_string(),
            renew_by.to_string(),
        ],
    );

    let mut tags = vec![delegation_tag];
    for relay in &config.nostr.relays {
        tags.push(Tag::custom(
            nostr_sdk::TagKind::custom("relay"),
            [relay.clone()],
        ));
    }

    let builder = EventBuilder::new(Kind::Custom(11111), "").tags(tags);

    match client.send_event_builder(builder).await {
        Ok(output) => {
            info!(
                event_id = %output.id().to_hex(),
                delegation = %fqdn,
                "delegation event published"
            );
        }
        Err(e) => {
            warn!(error = %e, delegation = %fqdn, "failed to publish delegation event (delegation saved locally)");
        }
    }
}

async fn send_reply(client: &Client, config: &Config, receiver: &PublicKey, message: &str) {
    if config.registrar.nsec_hex.is_empty() {
        return;
    }

    match client.send_private_msg(*receiver, message, []).await {
        Ok(_) => {
            tracing::debug!(receiver = %receiver.to_hex(), "reply DM sent");
        }
        Err(e) => {
            warn!(error = %e, receiver = %receiver.to_hex(), "failed to send reply DM");
        }
    }
}

fn mark_processed(
    store: &Arc<Store>,
    dm_event_id: &str,
    sender_npub: &str,
    name: &str,
    zone: &str,
    amount: i64,
    result: &str,
) {
    if let Err(e) = store.mark_dm_processed(dm_event_id, sender_npub, name, zone, amount, result) {
        error!(error = %e, dm_event_id = %dm_event_id, "failed to mark DM as processed");
    }
}

fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Name cannot be empty.".into());
    }
    if name.len() > 63 {
        return Err("Name exceeds 63 characters (DNS label limit).".into());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("Name contains invalid characters. Use a-z, 0-9, hyphen, underscore.".into());
    }
    if name.starts_with('-') || name.ends_with('-') {
        return Err("Name cannot start or end with a hyphen.".into());
    }
    Ok(())
}

fn format_payment_error(e: &PaymentError, _required: i64, fqdn: &str) -> String {
    match e {
        PaymentError::InsufficientPayment { got, needed } => {
            format!(
                "Insufficient payment for {}: sent {} sats, required {} sats.",
                fqdn, got, needed
            )
        }
        PaymentError::MintMismatch {
            token_mint,
            configured_mint,
        } => {
            format!(
                "Mint rejected for {}: '{}'. Policy: {}",
                fqdn, token_mint, configured_mint
            )
        }
        PaymentError::ProofNotUnspent { y, state } => {
            format!(
                "Token already spent for {}: proof {} is {}.",
                fqdn, y, state
            )
        }
        PaymentError::MintUnavailable { mint } => {
            format!(
                "Mint '{}' is temporarily unavailable for {}. Please try again later.",
                mint, fqdn
            )
        }
        PaymentError::MintCheckFailed(msg) => {
            format!("Could not verify token with mint for {}: {}", fqdn, msg)
        }
        PaymentError::TokenDecode(msg) => {
            format!("Invalid Cashu token for {}: {}", fqdn, msg)
        }
        PaymentError::NoProofs => {
            format!("Token contains no proofs for {}.", fqdn)
        }
        PaymentError::HashToCurve(msg) => {
            format!("Token verification error for {}: {}", fqdn, msg)
        }
        _ => format!("Payment verification failed for {}: {}", fqdn, e),
    }
}

fn format_unix_date(ts: i64) -> String {
    let days = ts / 86400;
    format!("day {} ({})", days, ts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_name_valid() {
        assert!(validate_name("alice").is_ok());
        assert!(validate_name("bob-123").is_ok());
        assert!(validate_name("a").is_ok());
        assert!(validate_name("test_name").is_ok());
    }

    #[test]
    fn test_validate_name_invalid() {
        assert!(validate_name("").is_err());
        assert!(validate_name("-alice").is_err());
        assert!(validate_name("alice-").is_err());
        assert!(validate_name("alice.example").is_err());
        assert!(validate_name(&"a".repeat(64)).is_err());
        assert!(validate_name("café").is_err());
    }

    #[test]
    fn test_payment_dm_parse_full() {
        let json = r#"{"type":"register","name":"alice","zone":"nodns.shop","duration_days":365,"token":"cashuAabc"}"#;
        let dm: PaymentDm = serde_json::from_str(json).unwrap();
        assert_eq!(dm.msg_type, "register");
        assert_eq!(dm.name, "alice");
        assert_eq!(dm.zone, "nodns.shop");
        assert_eq!(dm.duration_days, Some(365));
        assert_eq!(dm.token, "cashuAabc");
    }

    #[test]
    fn test_payment_dm_parse_minimal() {
        let json = r#"{"type":"register","name":"bob","zone":"nodns.shop","token":"cashuAxyz"}"#;
        let dm: PaymentDm = serde_json::from_str(json).unwrap();
        assert_eq!(dm.name, "bob");
        assert_eq!(dm.duration_days, None);
    }

    #[test]
    fn test_payment_dm_parse_missing_token() {
        let json = r#"{"type":"register","name":"alice","zone":"nodns.shop"}"#;
        let result: Result<PaymentDm, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_format_payment_error_insufficient() {
        let e = PaymentError::InsufficientPayment {
            got: 50,
            needed: 100,
        };
        let msg = format_payment_error(&e, 100, "alice.nodns.shop");
        assert!(msg.contains("50"));
        assert!(msg.contains("100"));
        assert!(msg.contains("alice.nodns.shop"));
    }
}
