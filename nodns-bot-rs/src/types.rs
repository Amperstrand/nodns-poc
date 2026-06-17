//! Shared types used across all nodns-bot modules.
//!
//! These map directly to the Go bot's data structures for 1:1 port fidelity.

use std::fmt;
use std::str::FromStr;

/// DNS record type constants.
pub const KIND_DNS_RECORD: u64 = 11111;
pub const DEFAULT_TTL: u32 = 3600;

/// A parsed delete request from a Nostr delete tag.
/// Format: ["delete", TYPE, NAME]
#[derive(Debug, Clone)]
pub struct DeleteRequest {
    pub record_type: String,
    pub name: String, // "@" for root, or subdomain name
}

/// A parsed DNS record from a Nostr record tag.
#[derive(Debug, Clone)]
pub struct DnsRecord {
    pub record_type: String, // A, AAAA, CNAME, TXT, MX, SRV
    pub name: String,        // "@" for root, or subdomain name
    pub ttl: u32,
    pub rdata: String,
}

/// A parsed delegation tag.
/// Format: ["delegation", DOMAIN, NPUB, `VALID_FROM`, `VALID_UNTIL`, `RENEW_BY`]
#[derive(Debug, Clone)]
pub struct Delegation {
    pub domain: String,   // e.g., "alice.test.shop"
    pub npub: String,     // npub receiving delegation
    pub valid_from: i64,  // unix timestamp
    pub valid_until: i64, // unix timestamp
    pub renew_by: i64,    // unix timestamp
}

/// A parsed registrar key publication tag.
/// Format: ["registrar", ZONE, `PUBKEY_HEX`]
#[derive(Debug, Clone)]
pub struct RegistrarKey {
    pub zone: String,       // e.g., "test.shop", "nodns.shop"
    pub pubkey_hex: String, // nostr pubkey in hex
}

/// A parsed payment proof tag.
#[derive(Debug, Clone)]
pub struct Payment {
    pub method: String,   // "cashu" or "zap"
    pub token: String,    // cashu token or zap receipt event ID
    pub mint_url: String, // cashu mint URL (empty for zap)
    pub amount: i64,      // sats
}

/// Result of parsing a kind 11111 event.
#[derive(Debug, Clone)]
pub struct ParsedEvent {
    pub records: Vec<DnsRecord>,
    pub deletes: Vec<DeleteRequest>,
    pub delegation: Option<Delegation>,
    pub registrar: Option<RegistrarKey>,
    pub payments: Vec<Payment>,
    pub claim: Option<ClaimRequest>,
    pub renewal: Option<RenewalRequest>,
    pub sig: String,
    pub raw_tags: Vec<Vec<String>>,
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

/// Parsed from `["claim", NAME, ZONE, VALID_UNTIL_TIMESTAMP]` tags.
#[derive(Debug, Clone)]
pub struct ClaimRequest {
    pub name: String,
    pub zone: String,
    pub valid_until: i64,
}

/// Parsed from `["renewal", NAME, ZONE, NEW_VALID_UNTIL]` tags.
#[derive(Debug, Clone)]
pub struct RenewalRequest {
    pub name: String,
    pub zone: String,
    pub new_valid_until: i64, // Unix timestamp
}

#[derive(Debug, Clone, PartialEq)]
pub enum DelegationState {
    Active,
    Grace,   // past valid_until but within grace period
    Expired, // past grace period
}

impl DelegationState {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            DelegationState::Active => "active",
            DelegationState::Grace => "grace",
            DelegationState::Expired => "expired",
        }
    }
}

impl FromStr for DelegationState {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "grace" => DelegationState::Grace,
            "expired" => DelegationState::Expired,
            _ => DelegationState::Active,
        })
    }
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
    pub renewal_price: i64, // locked price in sats (0 = free/not set)
    pub status: String,     // "active", "grace", "expired"
    pub created_at: i64,
    pub processed_at: i64,
}

/// A stored ACME certificate order (maps to `acme_orders` table).
#[derive(Debug, Clone)]
pub struct AcmeOrder {
    pub id: String,
    pub domain: String,
    pub npub: String,
    pub status: String,
    pub certificate_pem: Option<String>,
    pub private_key_pem: Option<String>,
    pub error: Option<String>,
    pub csr_der: Option<String>,
    pub environment: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A stored ACME order progress log entry (maps to `acme_order_logs` table).
#[derive(Debug, Clone, serde::Serialize)]
pub struct AcmeOrderLog {
    pub id: i64,
    pub order_id: String,
    pub stage: String,
    pub message: String,
    pub details: Option<String>,
    pub created_at: i64,
}

/// A stored acme-dns registration (maps to `acme_dns_registrations` table).
#[derive(Debug, Clone)]
pub struct AcmeDnsRegistration {
    pub subdomain: String,
    pub username: String,
    pub password: String,
    pub npub: String,
    pub zone: String,
    pub fulldomain: String,
    pub txt_value: Option<String>,
    pub txt_value_prev: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Metrics tracked by the bot.
#[derive(Debug, Default)]
pub struct Metrics {
    pub events_processed: std::sync::atomic::AtomicI64,
    pub events_rejected: std::sync::atomic::AtomicI64,
    pub ddns_successes: std::sync::atomic::AtomicI64,
    pub ddns_failures: std::sync::atomic::AtomicI64,
    pub last_event_at: std::sync::atomic::AtomicI64,
}

/// Build a fully qualified domain name: [name.]npub1xxx.zone.
#[must_use]
pub fn build_fqdn(npub: &str, name: &str, zone: &str) -> String {
    if name == "@" || name.is_empty() {
        format!("{npub}.{zone}.")
    } else {
        format!("{name}.{npub}.{zone}.")
    }
}

/// DNS record type string to u16 constant mapping.
#[must_use]
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
        write!(
            f,
            "{} {} IN {} {}",
            self.name, self.ttl, self.record_type, self.rdata
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delegation_state_as_str() {
        assert_eq!(DelegationState::Active.as_str(), "active");
        assert_eq!(DelegationState::Grace.as_str(), "grace");
        assert_eq!(DelegationState::Expired.as_str(), "expired");
    }

    #[test]
    fn delegation_state_from_str() {
        assert_eq!(
            DelegationState::from_str("active").unwrap(),
            DelegationState::Active
        );
        assert_eq!(
            DelegationState::from_str("grace").unwrap(),
            DelegationState::Grace
        );
        assert_eq!(
            DelegationState::from_str("expired").unwrap(),
            DelegationState::Expired
        );
        assert_eq!(
            DelegationState::from_str("unknown").unwrap(),
            DelegationState::Active
        );
    }

    #[test]
    fn delegation_state_equality() {
        assert_eq!(DelegationState::Active, DelegationState::Active);
        assert_ne!(DelegationState::Active, DelegationState::Grace);
        assert_ne!(DelegationState::Grace, DelegationState::Expired);
    }

    #[test]
    fn build_fqdn_apex_name() {
        let fqdn = build_fqdn("npub1abc", "@", "nodns.shop");
        assert_eq!(fqdn, "npub1abc.nodns.shop.");
    }

    #[test]
    fn build_fqdn_empty_name() {
        let fqdn = build_fqdn("npub1abc", "", "nodns.shop");
        assert_eq!(fqdn, "npub1abc.nodns.shop.");
    }

    #[test]
    fn build_fqdn_subdomain_name() {
        let fqdn = build_fqdn("npub1abc", "www", "nodns.shop");
        assert_eq!(fqdn, "www.npub1abc.nodns.shop.");
    }

    #[test]
    fn build_fqdn_deep_subdomain() {
        let fqdn = build_fqdn("npub1abc", "blog.api", "nodns.shop");
        assert_eq!(fqdn, "blog.api.npub1abc.nodns.shop.");
    }

    #[test]
    fn build_fqdn_different_zone() {
        let fqdn = build_fqdn("npub1xyz", "@", "test.shop");
        assert_eq!(fqdn, "npub1xyz.test.shop.");
    }

    #[test]
    fn record_type_to_u16_all_known_types() {
        assert_eq!(record_type_to_u16("A"), 1);
        assert_eq!(record_type_to_u16("NS"), 2);
        assert_eq!(record_type_to_u16("CNAME"), 5);
        assert_eq!(record_type_to_u16("PTR"), 12);
        assert_eq!(record_type_to_u16("MX"), 15);
        assert_eq!(record_type_to_u16("TXT"), 16);
        assert_eq!(record_type_to_u16("AAAA"), 28);
        assert_eq!(record_type_to_u16("SRV"), 33);
    }

    #[test]
    fn record_type_to_u16_unknown_type() {
        assert_eq!(record_type_to_u16("UNKNOWN"), 0);
        assert_eq!(record_type_to_u16("LOC"), 0);
        assert_eq!(record_type_to_u16("DNSKEY"), 0);
    }

    #[test]
    fn record_type_to_u16_case_sensitive() {
        assert_eq!(record_type_to_u16("a"), 0);
        assert_eq!(record_type_to_u16("txt"), 0);
        assert_eq!(record_type_to_u16("aaaa"), 0);
    }

    #[test]
    fn dns_record_display_a_record() {
        let record = DnsRecord {
            record_type: "A".to_string(),
            name: "@".to_string(),
            ttl: 3600,
            rdata: "1.2.3.4".to_string(),
        };
        assert_eq!(format!("{record}"), "@ 3600 IN A 1.2.3.4");
    }

    #[test]
    fn dns_record_display_txt_record() {
        let record = DnsRecord {
            record_type: "TXT".to_string(),
            name: "@".to_string(),
            ttl: 300,
            rdata: "hello world".to_string(),
        };
        assert_eq!(format!("{record}"), "@ 300 IN TXT hello world");
    }

    #[test]
    fn dns_record_display_custom_name() {
        let record = DnsRecord {
            record_type: "AAAA".to_string(),
            name: "www".to_string(),
            ttl: 7200,
            rdata: "::1".to_string(),
        };
        assert_eq!(format!("{record}"), "www 7200 IN AAAA ::1");
    }

    #[test]
    fn constants_have_expected_values() {
        assert_eq!(DEFAULT_TTL, 3600);
        assert_eq!(KIND_DNS_RECORD, 11111);
    }
}
