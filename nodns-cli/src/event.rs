use nostr_sdk::nips::nip19::{FromBech32, ToBech32};
use nostr_sdk::{
    Client, Event, EventBuilder, Filter, Keys, Kind, PublicKey, SecretKey, Tag,
};

use crate::config::Config;

const KIND_DNS: u16 = 11111; // EXPERIMENTAL — will change before v1, see docs/11-protocol-experimental-draft.md

pub fn resolve_secret_key(cfg: &Config) -> Result<Keys, String> {
    let sec = cfg
        .secret_key
        .as_deref()
        .ok_or("no secret key provided. Use --sec, NODNS_SECRET_KEY, or config file")?;

    if sec.starts_with("nsec1") {
        let sk = SecretKey::from_bech32(sec).map_err(|e| format!("invalid nsec: {e}"))?;
        return Ok(Keys::new(sk));
    }

    let sk = SecretKey::from_hex(sec).map_err(|e| format!("invalid hex key: {e}"))?;
    Ok(Keys::new(sk))
}

pub fn build_record_tags(
    records: &[(String, String, String, u32)],
) -> Vec<Tag> {
    records
        .iter()
        .map(|(rtype, name, rdata, ttl)| {
            Tag::parse([
                "record",
                rtype,
                if name.is_empty() { "@" } else { name },
                &ttl.to_string(),
                rdata,
            ])
            .expect("invalid tag")
        })
        .collect()
}

pub fn build_delete_tags(deletes: &[(String, String)]) -> Vec<Tag> {
    deletes
        .iter()
        .map(|(rtype, name)| {
            Tag::parse([
                "delete",
                rtype,
                if name.is_empty() { "@" } else { name },
            ])
            .expect("invalid tag")
        })
        .collect()
}

pub async fn publish_event(
    cfg: &Config,
    tags: Vec<Tag>,
    dry_run: bool,
) -> Result<Event, String> {
    let keys = resolve_secret_key(cfg)?;

    let event = EventBuilder::new(Kind::Custom(KIND_DNS), "")
        .tags(tags)
        .sign_with_keys(&keys)
        .map_err(|e| format!("signing failed: {e}"))?;

    let json = serde_json::to_string_pretty(&event).map_err(|e| format!("json error: {e}"))?;
    println!("{json}");

    if dry_run {
        eprintln!("(dry run — not publishing)");
        return Ok(event);
    }

    let npub = keys.public_key().to_bech32().map_err(|e| format!("npub error: {e}"))?;

    let client = Client::default();
    for relay in &cfg.relays {
        client
            .add_relay(relay)
            .await
            .map_err(|e| format!("relay {relay}: {e}"))?;
    }
    client
        .connect()
        .await;

    let output = client
        .send_event(&event)
        .await
        .map_err(|e| format!("publish failed: {e}"))?;

    for relay in &output.success {
        eprintln!("✓ {relay} — {}", output.id());
    }
    for (relay, err) in &output.failed {
        eprintln!("✗ {relay} — {err}");
    }

    eprintln!("\nRecord live at {npub}.{}", cfg.zone);

    Ok(event)
}

pub async fn fetch_events(
    cfg: &Config,
    pubkey: Option<PublicKey>,
) -> Result<Vec<Event>, String> {
    let pk = match pubkey {
        Some(pk) => pk,
        None => resolve_secret_key(cfg)?.public_key(),
    };

    let client = Client::default();
    for relay in &cfg.relays {
        client
            .add_relay(relay)
            .await
            .map_err(|e| format!("relay error: {e}"))?;
    }
    client.connect().await;

    let filter = Filter::new()
        .kind(Kind::Custom(KIND_DNS))
        .author(pk)
        .limit(100);

    let events = client
        .fetch_events(filter, std::time::Duration::from_secs(10))
        .await
        .map_err(|e| format!("query failed: {e}"))?;

    Ok(events.to_vec())
}
