//! Nostr relay communication — event querying and record extraction.

use std::time::Duration;

use anyhow::Result;
use nostr_sdk::{Client, Event, Filter, FromBech32, Kind, PublicKey, ToBech32};

use crate::parse::{deduplicate_records, parse_records_from_event};
use crate::types::{NostrDnsRecord, DEFAULT_RELAYS, RECORD_KIND};

const DEFAULT_QUERY_LIMIT: usize = 100;
const QUERY_TIMEOUT_SECS: u64 = 10;

#[must_use]
pub fn normalize_pubkey(pubkey: &str) -> String {
    if pubkey.starts_with("npub1") {
        if let Ok(pk) = PublicKey::from_bech32(pubkey) {
            return pk.to_hex();
        }
    }
    pubkey.to_string()
}

pub fn pubkey_to_npub(pubkey: &str) -> String {
    PublicKey::from_hex(pubkey)
        .ok()
        .and_then(|pk| pk.to_bech32().ok())
        .unwrap_or_else(|| pubkey.to_string())
}

pub fn default_relays() -> Vec<String> {
    DEFAULT_RELAYS.iter().map(|s| s.to_string()).collect()
}

async fn create_client(relays: &[String]) -> Result<Client> {
    let client = Client::builder().build();
    for url in relays {
        client.add_relay(url).await?;
    }
    client.connect().await;
    Ok(client)
}

pub async fn query_records_by_pubkey(
    pubkey: &str,
    zone: &str,
    relays: &[String],
) -> Result<Vec<NostrDnsRecord>> {
    let hex_pubkey = normalize_pubkey(pubkey);
    let pk = PublicKey::from_hex(&hex_pubkey)?;

    let client = create_client(relays).await?;

    let filter = Filter::new()
        .kind(Kind::Custom(RECORD_KIND as u16))
        .author(pk)
        .limit(DEFAULT_QUERY_LIMIT);

    let events = client
        .fetch_events(filter, Duration::from_secs(QUERY_TIMEOUT_SECS))
        .await?;

    client.disconnect().await;

    let mut all_records = Vec::new();
    for event in events.into_iter() {
        all_records.extend(parse_records_from_event(&event, zone));
    }

    Ok(deduplicate_records(all_records))
}

pub async fn query_records_by_domain(
    fqdn: &str,
    zone: &str,
    relays: &[String],
) -> Result<Vec<NostrDnsRecord>> {
    let client = create_client(relays).await?;

    let filter = Filter::new()
        .kind(Kind::Custom(RECORD_KIND as u16))
        .limit(500);

    let events = client
        .fetch_events(filter, Duration::from_secs(QUERY_TIMEOUT_SECS))
        .await?;

    client.disconnect().await;

    let mut all_records = Vec::new();
    for event in events.into_iter() {
        let records = parse_records_from_event(&event, zone);
        for r in records {
            if r.fqdn == fqdn {
                all_records.push(r);
            }
        }
    }

    Ok(deduplicate_records(all_records))
}

pub async fn query_all_recent_records(
    zone: &str,
    relays: &[String],
) -> Result<Vec<NostrDnsRecord>> {
    let client = create_client(relays).await?;

    let filter = Filter::new()
        .kind(Kind::Custom(RECORD_KIND as u16))
        .limit(200);

    let events = client
        .fetch_events(filter, Duration::from_secs(QUERY_TIMEOUT_SECS))
        .await?;

    client.disconnect().await;

    let mut all_records = Vec::new();
    for event in events.into_iter() {
        all_records.extend(parse_records_from_event(&event, zone));
    }

    all_records.sort_by_key(|r| std::cmp::Reverse(r.created_at));
    Ok(all_records)
}

pub async fn fetch_events(relays: &[String], pubkey: &str, limit: usize) -> Result<Vec<Event>> {
    let hex_pubkey = normalize_pubkey(pubkey);
    let pk = PublicKey::from_hex(&hex_pubkey)?;

    let client = create_client(relays).await?;

    let filter = Filter::new()
        .kind(Kind::Custom(RECORD_KIND as u16))
        .author(pk)
        .limit(limit);

    let mut events = client
        .fetch_events(filter, Duration::from_secs(QUERY_TIMEOUT_SECS))
        .await?
        .into_iter()
        .collect::<Vec<_>>();

    client.disconnect().await;

    events.sort_by_key(|e| std::cmp::Reverse(e.created_at));
    Ok(events)
}
