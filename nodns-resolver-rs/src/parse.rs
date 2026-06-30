//! Nostr event parsing — record extraction and FQDN computation.

use nostr_sdk::{Event, PublicKey, ToBech32};

use crate::types::{NostrDnsRecord, DEFAULT_ZONE};

const DEFAULT_TTL: u32 = 3600;

#[must_use]
pub fn is_npub_derived_name(name: &str) -> bool {
    name.is_empty() || name == "@"
}

fn safe_npub_encode(pubkey_hex: &str) -> String {
    PublicKey::from_hex(pubkey_hex)
        .ok()
        .and_then(|pk| pk.to_bech32().ok())
        .unwrap_or_else(|| pubkey_hex.chars().take(16).collect())
}

#[must_use]
pub fn compute_fqdn(name: &str, pubkey: &str, zone: &str) -> String {
    if is_npub_derived_name(name) {
        format!("{}.{}", safe_npub_encode(pubkey), zone)
    } else {
        format!("{}.{}", name, zone)
    }
}

#[must_use]
pub fn compute_fqdn_default(name: &str, pubkey: &str) -> String {
    compute_fqdn(name, pubkey, DEFAULT_ZONE)
}

fn parse_ttl_from_tag(tag: &[String]) -> u32 {
    if tag.len() > 10 {
        if let Ok(parsed) = tag[10].parse::<u32>() {
            if parsed > 0 {
                return parsed;
            }
        }
    }
    if tag.len() > 4 {
        for i in (4..tag.len()).rev() {
            if let Ok(parsed) = tag[i].parse::<u32>() {
                if parsed > 0 {
                    return parsed;
                }
            }
        }
    }
    DEFAULT_TTL
}

#[must_use]
pub fn parse_records_from_event(event: &Event, zone: &str) -> Vec<NostrDnsRecord> {
    let mut records = Vec::new();
    let pubkey_hex = event.pubkey.to_hex();
    let event_id = event.id.to_hex();
    let created_at = event.created_at.as_secs() as i64;

    for tag in event.tags.iter() {
        let slice = tag.as_slice();
        if slice.is_empty() || slice[0] != "record" {
            continue;
        }
        if slice.len() < 4 {
            continue;
        }

        let record_type = slice[1].clone();
        let name = if slice[2].is_empty() {
            "@".to_string()
        } else {
            slice[2].clone()
        };
        let value = slice[3].clone();

        if record_type.is_empty() || value.is_empty() {
            continue;
        }

        let ttl = parse_ttl_from_tag(slice);
        let fqdn = compute_fqdn(&name, &pubkey_hex, zone);

        records.push(NostrDnsRecord {
            record_type,
            name,
            value,
            ttl,
            fqdn,
            pubkey: pubkey_hex.clone(),
            event_id: event_id.clone(),
            created_at,
        });
    }

    records
}

#[must_use]
pub fn deduplicate_records(records: Vec<NostrDnsRecord>) -> Vec<NostrDnsRecord> {
    let mut seen: std::collections::HashMap<String, NostrDnsRecord> =
        std::collections::HashMap::new();

    for r in records {
        let key = format!("{}:{}:{}:{}", r.fqdn, r.record_type, r.name, r.value);
        match seen.get(&key) {
            Some(existing) if r.created_at <= existing.created_at => {}
            _ => {
                seen.insert(key, r);
            }
        }
    }

    let mut result: Vec<NostrDnsRecord> = seen.into_values().collect();
    result.sort_by_key(|r| std::cmp::Reverse(r.created_at));
    result
}
