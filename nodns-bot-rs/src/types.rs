//! Shared types used across all nodns-bot modules.
//!
//! These map directly to the Go bot's data structures for 1:1 port fidelity.

use std::fmt;

/// DNS record type constants.
pub const KIND_DNS_RECORD: u64 = 11111;
pub const DEFAULT_TTL: u32 = 3600;

/// A parsed DNS record from a Nostr record tag.
#[derive(Debug, Clone)]
pub struct DnsRecord {
    pub record_type: String, // A, AAAA, CNAME, TXT, MX, SRV
    pub name: String,        // "@" for root, or subdomain name
    pub ttl: u32,
    pub rdata: String,
}

/// A parsed delegation tag.
/// Format: ["delegation", DOMAIN, NPUB, VALID_FROM, VALID_UNTIL, RENEW_BY]
#[derive(Debug, Clone)]
pub struct Delegation {
    pub domain: String,     // e.g., "alice.cv"
    pub npub: String,       // npub receiving delegation
    pub valid_from: i64,    // unix timestamp
    pub valid_until: i64,   // unix timestamp
    pub renew_by: i64,      // unix timestamp
}

/// A parsed registrar key publication tag.
/// Format: ["registrar", ZONE, PUBKEY_HEX]
#[derive(Debug, Clone)]
pub struct RegistrarKey {
    pub zone: String,        // e.g., "cv", "nodns.shop"
    pub pubkey_hex: String,  // nostr pubkey in hex
}

/// A parsed payment proof tag.
#[derive(Debug, Clone)]
pub struct Payment {
    pub method: String,  // "cashu" or "zap"
    pub token: String,   // cashu token or zap receipt event ID
    pub mint_url: String, // cashu mint URL (empty for zap)
    pub amount: i64,     // sats
}

/// Result of parsing a kind 11111 event.
#[derive(Debug, Clone)]
pub struct ParsedEvent {
    pub records: Vec<DnsRecord>,
    pub delegation: Option<Delegation>,
    pub registrar: Option<RegistrarKey>,
    pub payments: Vec<Payment>,
}

/// A stored DNS event record (maps to `events` table).
#[derive(Debug, Clone)]
pub struct EventRecord {
    pub event_id: String,
    pub npub: String,
    pub pubkey: String,
    pub name: String,
    pub record_type: String,
    pub ttl: u32,
    pub rdata: String,
    pub zone: String,
    pub created_at: i64,
    pub processed_at: i64,
    pub deleted: bool,
}

/// A stored delegation record (maps to `delegations` table).
#[derive(Debug, Clone)]
pub struct DelegationRecord {
    pub event_id: String,
    pub domain: String,
    pub zone: String,
    pub npub: String,
    pub pubkey: String,
    pub valid_from: i64,
    pub valid_until: i64,
    pub renew_by: i64,
    pub registrar_pubkey: String,
    pub created_at: i64,
    pub processed_at: i64,
}

/// Metrics tracked by the bot.
#[derive(Debug, Default)]
pub struct Metrics {
    pub events_processed: std::sync::atomic::AtomicI64,
    pub events_rejected: std::sync::atomic::AtomicI64,
    pub ddns_successes: std::sync::atomic::AtomicI64,
    pub ddns_failures: std::sync::atomic::AtomicI64,
}

/// Build a fully qualified domain name: [name.]npub1xxx.zone.
pub fn build_fqdn(npub: &str, name: &str, zone: &str) -> String {
    if name == "@" || name.is_empty() {
        format!("{}.{}.", npub, zone)
    } else {
        format!("{}.{}.{}.", name, npub, zone)
    }
}

/// DNS record type string to u16 constant mapping.
pub fn record_type_to_u16(t: &str) -> u16 {
    match t {
        "A" => 1,
        "NS" => 2,
        "CNAME" => 5,
        "PTR" => 12,
        "MX" => 15,
        "TXT" => 16,
        "AAAA" => 28,
        "SRV" => 33,
        _ => 0,
    }
}

impl fmt::Display for DnsRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} IN {} {}", self.name, self.ttl, self.record_type, self.rdata)
    }
}
