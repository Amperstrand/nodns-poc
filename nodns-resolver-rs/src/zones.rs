//! Zone discovery — find nodns zones via Nostr handler events and DNS TXT verification.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;
use nostr_sdk::{Client, Event, Filter, Kind};

use crate::dns::query_doh;
use crate::types::{ZonePricing, ZoneStatus, ZoneStatusLevel, ZoneTxtFields, ZONE_HANDLER_KIND};

const QUERY_TIMEOUT_SECS: u64 = 6;

#[must_use]
pub fn parse_zone_txt(txt: &str) -> ZoneTxtFields {
    let mut result = HashMap::new();
    for part in txt.split(';') {
        if let Some(eq) = part.find('=') {
            let key = part[..eq].trim().to_string();
            let val = part[eq + 1..].trim().to_string();
            if !key.is_empty() {
                result.insert(key, val);
            }
        }
    }
    result
}

fn strip_txt_quotes(data: &str) -> String {
    data.trim_start_matches('"')
        .trim_end_matches('"')
        .replace("\" \"", "")
        .replace("\"", "")
}

pub async fn fetch_dns_txt(
    zone: &str,
    doh_endpoint: &str,
    http: &reqwest::Client,
) -> Option<String> {
    let name = format!("_nodns.{zone}");
    match query_doh(&name, "TXT", doh_endpoint, http).await {
        Ok(resp) => {
            if let Some(answers) = resp.answer {
                for a in answers {
                    if a.type_num == 16 {
                        return Some(strip_txt_quotes(&a.data));
                    }
                }
            }
            None
        }
        Err(_) => None,
    }
}

fn parse_pricing_tag(tags_raw: &[Vec<String>]) -> Option<ZonePricing> {
    let tag = tags_raw
        .iter()
        .find(|t| !t.is_empty() && t[0] == "pricing")?;
    let mut create = 0u64;
    let mut update = 0u64;
    let mut delete = 0u64;
    let mut found = false;

    for entry in &tag[1..] {
        if let Some(eq) = entry.find('=') {
            let key = entry[..eq].trim();
            let val_str = entry[eq + 1..].trim();
            if let Ok(val) = val_str.parse::<u64>() {
                found = true;
                match key {
                    "create" => create = val,
                    "update" => update = val,
                    "delete" => delete = val,
                    _ => {}
                }
            }
        }
    }

    if found {
        Some(ZonePricing {
            create,
            update,
            delete,
        })
    } else {
        None
    }
}

fn parse_status_tag(tags_raw: &[Vec<String>]) -> (ZoneStatusLevel, Option<String>) {
    for t in tags_raw {
        if t.len() >= 2 && !t.is_empty() && t[0] == "status" {
            let value = t[1].to_lowercase();
            let reason = t.get(2).cloned();
            return match value.as_str() {
                "testing" => (ZoneStatusLevel::Testing, reason),
                "preview" => (ZoneStatusLevel::Preview, reason),
                "production" => (ZoneStatusLevel::Production, reason),
                _ => (ZoneStatusLevel::Unknown, reason),
            };
        }
    }
    (ZoneStatusLevel::Unknown, None)
}

fn event_to_partial_zone(event: &Event) -> Option<ZoneStatus> {
    let tags_raw: Vec<Vec<String>> = event.tags.iter().map(|t| t.as_slice().to_vec()).collect();

    let zone_tag = tags_raw.iter().find(|t| t.len() >= 2 && t[0] == "zone")?;
    let zone = zone_tag[1].to_lowercase();

    let pricing = parse_pricing_tag(&tags_raw);
    let (status, status_reason) = parse_status_tag(&tags_raw);

    let testnet = tags_raw.iter().any(|t| !t.is_empty() && t[0] == "testnet");

    let dnskey_hash = tags_raw
        .iter()
        .find(|t| t.len() >= 2 && t[0] == "dnskey_hash")
        .map(|t| t[1].clone());

    let dnskey_alg = tags_raw
        .iter()
        .find(|t| t.len() >= 2 && t[0] == "dnskey_alg")
        .map(|t| t[1].clone());

    let mint = tags_raw
        .iter()
        .find(|t| t.len() >= 2 && t[0] == "mint")
        .map(|t| t[1].clone());

    let web = tags_raw
        .iter()
        .find(|t| t.len() >= 2 && t[0] == "web")
        .map(|t| t[1].clone());

    Some(ZoneStatus {
        zone,
        pubkey: event.pubkey.to_hex(),
        status,
        testnet,
        status_reason,
        dnskey_hash,
        dnskey_alg,
        pricing,
        mint,
        web,
        verified: false,
        verification_error: None,
    })
}

async fn create_client(relays: &[String]) -> Result<Client> {
    let client = Client::builder().build();
    for url in relays {
        client.add_relay(url).await?;
    }
    client.connect().await;
    Ok(client)
}

async fn query_zone_handler_events(relays: &[String]) -> Vec<Event> {
    let client = match create_client(relays).await {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let filter = Filter::new()
        .kind(Kind::Custom(ZONE_HANDLER_KIND as u16))
        .limit(100);

    let events = client
        .fetch_events(filter, Duration::from_secs(QUERY_TIMEOUT_SECS))
        .await
        .map(|e| e.into_iter().collect::<Vec<_>>())
        .unwrap_or_default();

    client.disconnect().await;
    events
}

pub async fn discover_zones(
    relays: &[String],
    doh_endpoint: &str,
    http: &reqwest::Client,
) -> Vec<ZoneStatus> {
    let events = query_zone_handler_events(relays).await;

    let mut by_zone: HashMap<String, (Event, u64)> = HashMap::new();

    for event in &events {
        let tags_raw: Vec<Vec<String>> = event.tags.iter().map(|t| t.as_slice().to_vec()).collect();

        let k_tags: Vec<&str> = tags_raw
            .iter()
            .filter(|t| !t.is_empty() && t[0] == "k")
            .filter_map(|t| t.get(1).map(|s| s.as_str()))
            .collect();

        if !k_tags.is_empty() && !k_tags.contains(&"11111") {
            continue;
        }

        let zone_tag = tags_raw.iter().find(|t| t.len() >= 2 && t[0] == "zone");
        let Some(zone_tag) = zone_tag else { continue };
        let zone = zone_tag[1].to_lowercase();
        let ts = event.created_at.as_secs();

        match by_zone.get(&zone) {
            Some(existing) if ts <= existing.1 => {}
            _ => {
                by_zone.insert(zone, (event.clone(), ts));
            }
        }
    }

    let mut zones: Vec<ZoneStatus> = by_zone
        .values()
        .filter_map(|(event, _)| event_to_partial_zone(event))
        .collect();

    for zone in &mut zones {
        let txt = fetch_dns_txt(&zone.zone, doh_endpoint, http).await;

        let Some(txt) = txt else {
            zone.verification_error = Some("No _nodns TXT record found".to_string());
            continue;
        };

        let parsed = parse_zone_txt(&txt);
        let Some(txt_npub) = parsed.get("npub") else {
            zone.verification_error = Some("TXT record missing npub field".to_string());
            continue;
        };

        if txt_npub.to_lowercase() != zone.pubkey.to_lowercase() {
            zone.verification_error = Some("TXT npub does not match event signer".to_string());
            continue;
        }

        if parsed.get("testnet").map(|s| s.as_str()) == Some("1") {
            zone.testnet = true;
        }

        zone.verified = true;
    }

    zones.sort_by(|a, b| a.zone.cmp(&b.zone));
    zones
}
