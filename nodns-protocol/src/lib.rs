//! `nodns-protocol` — parse and validate NoDNS `kind 11111` record tags.
//!
//! A standalone, dependency-light crate that parses the 5-element zone-file
//! `record` tag form defined by the [NoDNS protocol]:
//!
//! ```text
//! ["record", "TYPE", "name", "TTL", "rdata"]
//! ```
//!
//! Produces validated [`Record`]s using the same naming as `nodns-lease`'s
//! `nodns-sync-core`. Usable by the bot, sync daemons, CLIs, and third-party
//! clients — anyone who needs to turn a Nostr event's tags into typed DNS
//! records.
//!
//! [NoDNS protocol]: https://gitworkshop.dev/npub1hw6amg8p24ne08c9gdq8hhpqx0t0pwanpae9z25crn7m9uy7yarse465gr/nodns-protocol/tree/main

use std::net::IpAddr;
use std::str::FromStr;

use ipnet::IpNet;
use thiserror::Error;

// ── Constants ──────────────────────────────────────────────────────────────

/// Default TTL when the tag's TTL field is empty or zero.
pub const DEFAULT_TTL: u32 = 3600;

/// Private/reserved IP networks blocked when `policy.block_private_ip` is set.
///
/// Broader than RFC 1918 — also covers loopback, link-local, carrier-grade NAT
/// (`100.64.0.0/10`), unspecified (`0.0.0.0/8`), and IPv6 equivalents.
const PRIVATE_NETWORKS: &[&str] = &[
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "127.0.0.0/8",
    "169.254.0.0/16",
    "0.0.0.0/8",
    "100.64.0.0/10",
    "fc00::/7",
    "fe80::/10",
    "::1/128",
];

// ── Types ───────────────────────────────────────────────────────────────────

/// One DNS record, 5-element zone-file form.
///
/// Naming matches `nodns-lease`'s `nodns_sync_core::Record` so the eventual
/// port is a clean copy with no renaming.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// Record type: `"A"`, `"AAAA"`, `"CNAME"`, `"TXT"`, `"MX"`, `"SRV"`, etc.
    pub rtype: String,
    /// Subdomain label: `"@"` for apex, `""` → `"@"`, or a label like `"www"`.
    pub name: String,
    /// TTL in seconds; `0` is normalised to [`DEFAULT_TTL`] during parsing.
    pub ttl: u32,
    /// Zone-file text: single value or space-separated fields (MX, SRV).
    pub rdata: String,
}

/// Validation policy, injected by the caller — the parser never reads from
/// global config.
#[derive(Debug, Clone)]
pub struct ValidationPolicy {
    /// Whitelist of allowed record types (uppercase). Empty = accept all known.
    pub allowed_types: Vec<String>,
    /// Reject private/reserved IPs in `A`/`AAAA` rdata.
    pub block_private_ip: bool,
    /// Maximum TXT record length in characters. `0` = no limit.
    pub max_txt_length: usize,
}

impl Default for ValidationPolicy {
    fn default() -> Self {
        Self {
            allowed_types: vec![
                "A".into(),
                "AAAA".into(),
                "CNAME".into(),
                "TXT".into(),
                "MX".into(),
            ],
            block_private_ip: true,
            max_txt_length: 512,
        }
    }
}

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("tag {index}: {message}")]
    TagError { index: usize, message: String },
    #[error("{0}")]
    Validation(String),
    #[error("CNAME records cannot coexist with other record types at the same name")]
    CannotCoexistWithCname,
}

// ── Public API ──────────────────────────────────────────────────────────────

/// Parse all `["record", ...]` tags from a Nostr event's tag list.
///
/// Each tag must be the 5-element form: `["record", "TYPE", "name", "TTL", "rdata"]`.
/// Non-record tags are silently skipped. Returns validated records plus a
/// cross-record constraint check (CNAME coexistence).
pub fn parse_records(
    tags: &[Vec<String>],
    policy: &ValidationPolicy,
) -> Result<Vec<Record>, ParseError> {
    let mut records = Vec::new();

    for (i, tag) in tags.iter().enumerate() {
        if tag.is_empty() || tag[0] != "record" {
            continue;
        }
        let rec = parse_record(tag, policy)
            .map_err(|e| ParseError::TagError { index: i, message: e.to_string() })?;
        records.push(rec);
    }

    validate_record_set(&records)?;
    Ok(records)
}

/// Parse a single 5-element record tag.
///
/// Format: `["record", "TYPE", "name", "TTL", "rdata"]`
pub fn parse_record(tag: &[String], policy: &ValidationPolicy) -> Result<Record, ParseError> {
    if tag.is_empty() || tag[0] != "record" {
        return Err(ParseError::Validation(
            "first element must be 'record'".into(),
        ));
    }

    if tag.len() != 5 {
        return Err(ParseError::Validation(format!(
            "record tag must have 5 elements, got {}",
            tag.len()
        )));
    }

    let rtype = tag[1].to_uppercase();
    if rtype.is_empty() {
        return Err(ParseError::Validation("record type cannot be empty".into()));
    }

    let allowed_set: Vec<String> = policy.allowed_types.iter().map(|t| t.to_uppercase()).collect();
    if !allowed_set.is_empty() && !allowed_set.contains(&rtype) {
        return Err(ParseError::Validation(format!(
            "record type {rtype:?} not allowed"
        )));
    }

    let name = if tag[2].is_empty() {
        "@".to_string()
    } else {
        tag[2].clone()
    };
    validate_dns_label(&name)?;

    let rdata = tag[4].clone();

    let mut ttl = DEFAULT_TTL;
    if !tag[3].is_empty() {
        let parsed: u32 = tag[3]
            .parse()
            .map_err(|e| ParseError::Validation(format!("invalid TTL {:?}: {}", tag[3], e)))?;
        ttl = parsed;
    }
    if ttl == 0 {
        ttl = DEFAULT_TTL;
    }

    let rec = Record { rtype, name, ttl, rdata };
    validate_record(&rec, policy)?;
    Ok(rec)
}

/// Validate a single record against the policy.
pub fn validate_record(rec: &Record, policy: &ValidationPolicy) -> Result<(), ParseError> {
    if rec.rdata.is_empty() && rec.rtype != "TXT" {
        return Err(ParseError::Validation(format!(
            "{} record requires rdata",
            rec.rtype
        )));
    }

    if rec.rtype == "TXT" && policy.max_txt_length > 0 && rec.rdata.len() > policy.max_txt_length {
        return Err(ParseError::Validation(format!(
            "TXT record exceeds max length {}: got {}",
            policy.max_txt_length,
            rec.rdata.len()
        )));
    }

    let fields: Vec<&str> = rec.rdata.split_whitespace().collect();

    match rec.rtype.as_str() {
        "A" => {
            let ip = std::net::Ipv4Addr::from_str(&rec.rdata).map_err(|_| {
                ParseError::Validation(format!("invalid IPv4 address: {}", rec.rdata))
            })?;
            if policy.block_private_ip && is_private_ip(IpAddr::V4(ip)) {
                return Err(ParseError::Validation(format!(
                    "private IP address blocked: {}",
                    rec.rdata
                )));
            }
        }
        "AAAA" => {
            let ip = std::net::Ipv6Addr::from_str(&rec.rdata).map_err(|_| {
                ParseError::Validation(format!("invalid IPv6 address: {}", rec.rdata))
            })?;
            if policy.block_private_ip && is_private_ip(IpAddr::V6(ip)) {
                return Err(ParseError::Validation(format!(
                    "private IP address blocked: {}",
                    rec.rdata
                )));
            }
        }
        "CNAME" | "NS" | "PTR" => {
            validate_hostname(&rec.rdata)?;
        }
        "TXT" => {
            // Reserved-name spoofing protections.
            if rec.name == "_dmarc" {
                return Err(ParseError::Validation(
                    "TXT record with name '_dmarc' is reserved (DMARC spoofing protection)".into(),
                ));
            }
            if rec.name.starts_with("_domainkey") {
                return Err(ParseError::Validation(
                    "TXT record with name starting with '_domainkey' is reserved (DKIM spoofing protection)".into(),
                ));
            }
            if rec.name == "@" && rec.rdata.trim().starts_with("v=spf1") {
                return Err(ParseError::Validation(
                    "TXT record at apex with SPF data is reserved (SPF spoofing protection)".into(),
                ));
            }
        }
        "MX" => {
            if fields.len() < 2 {
                return Err(ParseError::Validation(
                    "MX record requires: priority mailserver".into(),
                ));
            }
            let _priority: u16 = fields[0].parse().map_err(|_| {
                ParseError::Validation(format!("invalid MX priority: {}", fields[0]))
            })?;
            validate_hostname(fields[1])?;
        }
        "SRV" => {
            if fields.len() < 4 {
                return Err(ParseError::Validation(
                    "SRV record requires: priority weight port target".into(),
                ));
            }
            for (i, field_name) in ["priority", "weight", "port"].iter().enumerate() {
                let _: u16 = fields[i].parse().map_err(|_| {
                    ParseError::Validation(format!("invalid SRV {}: {}", field_name, fields[i]))
                })?;
            }
            validate_hostname(fields[3])?;
        }
        _ => {
            return Err(ParseError::Validation(format!(
                "unsupported record type: {}",
                rec.rtype
            )));
        }
    }

    Ok(())
}

/// Check cross-record constraints across a record set.
///
/// Currently enforces RFC 1912: CNAME cannot coexist with other record types
/// at the same name.
pub fn validate_record_set(records: &[Record]) -> Result<(), ParseError> {
    use std::collections::HashSet;

    let mut cname_names: HashSet<&str> = HashSet::new();
    let mut other_names: HashSet<&str> = HashSet::new();

    for rec in records {
        if rec.rtype == "CNAME" {
            cname_names.insert(&rec.name);
        } else {
            other_names.insert(&rec.name);
        }
    }

    for name in &cname_names {
        if other_names.contains(name) {
            return Err(ParseError::CannotCoexistWithCname);
        }
    }

    Ok(())
}

/// Check if an IP address is in a private/reserved range.
pub fn is_private_ip(ip: IpAddr) -> bool {
    PRIVATE_NETWORKS
        .iter()
        .filter_map(|cidr| IpNet::from_str(cidr).ok())
        .any(|net| net.contains(&ip))
}

/// Validate a DNS label (the `name` field → subdomain label).
///
/// Returns Ok for `"@"` (apex) and empty strings. Otherwise enforces:
/// - max 63 characters
/// - lowercase alphanumeric, hyphens, and underscores (for service labels)
/// - cannot start or end with a hyphen
pub fn validate_dns_label(name: &str) -> Result<(), ParseError> {
    if name == "@" || name.is_empty() {
        return Ok(());
    }
    if name.len() > 63 {
        return Err(ParseError::Validation(format!(
            "DNS label too long: {} characters (max 63)",
            name.len()
        )));
    }
    if name.starts_with('-') {
        return Err(ParseError::Validation(
            "DNS label cannot start with a hyphen".into(),
        ));
    }
    if name.ends_with('-') {
        return Err(ParseError::Validation(
            "DNS label cannot end with a hyphen".into(),
        ));
    }
    for ch in name.chars() {
        if !ch.is_ascii_lowercase() && !ch.is_ascii_digit() && ch != '-' && ch != '_' {
            if ch.is_ascii_uppercase() {
                return Err(ParseError::Validation(format!(
                    "DNS label must be lowercase, found uppercase: '{ch}'"
                )));
            }
            return Err(ParseError::Validation(format!(
                "DNS label contains invalid character: '{ch}'"
            )));
        }
    }
    Ok(())
}

/// Validate a hostname / domain name in rdata (CNAME target, MX exchange, etc.).
///
/// Lightweight std-only check: dot-separated labels, each 1–63 chars,
/// alphanumeric + hyphen, optional trailing dot. Catches obviously broken
/// input; the provider (Cloudflare/Knot) does its own strict validation.
fn validate_hostname(name: &str) -> Result<(), ParseError> {
    if name.is_empty() {
        return Err(ParseError::Validation("domain name cannot be empty".into()));
    }

    let trimmed = name.strip_suffix('.').unwrap_or(name);
    if trimmed.is_empty() || trimmed.len() > 253 {
        return Err(ParseError::Validation(format!(
            "domain name length invalid: {} chars",
            trimmed.len()
        )));
    }

    for label in trimmed.split('.') {
        if label.is_empty() {
            return Err(ParseError::Validation(
                "domain name contains empty label".into(),
            ));
        }
        if label.len() > 63 {
            return Err(ParseError::Validation(format!(
                "domain label too long: {} chars (max 63)",
                label.len()
            )));
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err(ParseError::Validation(
                "domain label cannot start or end with a hyphen".into(),
            ));
        }
        for ch in label.chars() {
            if !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_' {
                return Err(ParseError::Validation(format!(
                    "domain label contains invalid character: '{ch}'"
                )));
            }
        }
    }

    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn record(rtype: &str, name: &str, ttl: u32, rdata: &str) -> Record {
        Record {
            rtype: rtype.into(),
            name: name.into(),
            ttl,
            rdata: rdata.into(),
        }
    }

    // ── parse_record basics ──

    #[test]
    fn parse_record_a_basic() {
        let t = tag(&["record", "A", "@", "3600", "1.2.3.4"]);
        let rec = parse_record(&t, &ValidationPolicy::default()).unwrap();
        assert_eq!(rec.rtype, "A");
        assert_eq!(rec.name, "@");
        assert_eq!(rec.ttl, 3600);
        assert_eq!(rec.rdata, "1.2.3.4");
    }

    #[test]
    fn parse_record_empty_name_defaults_to_at() {
        let t = tag(&["record", "A", "", "3600", "1.2.3.4"]);
        let rec = parse_record(&t, &ValidationPolicy::default()).unwrap();
        assert_eq!(rec.name, "@");
    }

    #[test]
    fn parse_record_ttl_zero_defaults() {
        let t = tag(&["record", "A", "@", "0", "1.2.3.4"]);
        let rec = parse_record(&t, &ValidationPolicy::default()).unwrap();
        assert_eq!(rec.ttl, DEFAULT_TTL);
    }

    #[test]
    fn parse_record_ttl_empty_defaults() {
        let t = tag(&["record", "A", "@", "", "1.2.3.4"]);
        let rec = parse_record(&t, &ValidationPolicy::default()).unwrap();
        assert_eq!(rec.ttl, DEFAULT_TTL);
    }

    #[test]
    fn parse_record_wrong_arity_rejected() {
        let t = tag(&["record", "A"]);
        let err = parse_record(&t, &ValidationPolicy::default()).unwrap_err();
        assert!(err.to_string().contains("must have 5 elements"));
    }

    #[test]
    fn parse_record_type_not_in_allowed_list() {
        let policy = ValidationPolicy {
            allowed_types: vec!["A".into(), "AAAA".into()],
            ..Default::default()
        };
        let t = tag(&["record", "CNAME", "@", "3600", "example.com"]);
        let err = parse_record(&t, &policy).unwrap_err();
        assert!(err.to_string().contains("not allowed"));
    }

    #[test]
    fn parse_record_type_case_insensitive() {
        let t = tag(&["record", "a", "@", "3600", "1.2.3.4"]);
        let rec = parse_record(&t, &ValidationPolicy::default()).unwrap();
        assert_eq!(rec.rtype, "A");
    }

    // ── private IP blocking ──

    #[test]
    fn parse_record_blocks_private_ipv4() {
        let t = tag(&["record", "A", "@", "3600", "10.0.0.1"]);
        let err = parse_record(&t, &ValidationPolicy::default()).unwrap_err();
        assert!(err.to_string().contains("private IP address blocked"));
    }

    #[test]
    fn parse_record_blocks_private_ipv6() {
        let t = tag(&["record", "AAAA", "@", "3600", "fc00::1"]);
        let err = parse_record(&t, &ValidationPolicy::default()).unwrap_err();
        assert!(err.to_string().contains("private IP address blocked"));
    }

    #[test]
    fn parse_record_allows_public_ipv6() {
        let t = tag(&["record", "AAAA", "@", "3600", "2001:db8::1"]);
        let rec = parse_record(&t, &ValidationPolicy::default()).unwrap();
        assert_eq!(rec.rtype, "AAAA");
    }

    #[test]
    fn parse_record_rejects_invalid_ipv4() {
        let t = tag(&["record", "A", "@", "3600", "not.an.ip.address"]);
        let err = parse_record(&t, &ValidationPolicy::default()).unwrap_err();
        assert!(err.to_string().contains("invalid IPv4"));
    }

    #[test]
    fn parse_record_blocks_cgn_range() {
        let t = tag(&["record", "A", "@", "3600", "100.64.0.1"]);
        let err = parse_record(&t, &ValidationPolicy::default()).unwrap_err();
        assert!(err.to_string().contains("private IP"));
    }

    #[test]
    fn parse_record_blocks_unspecified_range() {
        let t = tag(&["record", "A", "@", "3600", "0.0.0.1"]);
        let err = parse_record(&t, &ValidationPolicy::default()).unwrap_err();
        assert!(err.to_string().contains("private IP"));
    }

    #[test]
    fn private_ip_blocking_disabled() {
        let policy = ValidationPolicy {
            block_private_ip: false,
            ..Default::default()
        };
        let t = tag(&["record", "A", "@", "3600", "10.0.0.1"]);
        let rec = parse_record(&t, &policy).unwrap();
        assert_eq!(rec.rdata, "10.0.0.1");
    }

    // ── is_private_ip ──

    #[test]
    fn is_private_ip_v4_ranges() {
        assert!(is_private_ip("10.0.0.1".parse().unwrap()));
        assert!(is_private_ip("172.16.0.1".parse().unwrap()));
        assert!(is_private_ip("192.168.1.1".parse().unwrap()));
        assert!(is_private_ip("127.0.0.1".parse().unwrap()));
        assert!(is_private_ip("169.254.1.1".parse().unwrap()));
        assert!(is_private_ip("0.0.0.1".parse().unwrap()));
        assert!(is_private_ip("100.64.0.1".parse().unwrap()));
        assert!(!is_private_ip("1.1.1.1".parse().unwrap()));
        assert!(!is_private_ip("8.8.8.8".parse().unwrap()));
    }

    #[test]
    fn is_private_ip_v6_ranges() {
        assert!(is_private_ip("::1".parse::<IpAddr>().unwrap()));
        assert!(is_private_ip("fc00::1".parse::<IpAddr>().unwrap()));
        assert!(is_private_ip("fe80::1".parse::<IpAddr>().unwrap()));
        assert!(!is_private_ip("2001:db8::1".parse::<IpAddr>().unwrap()));
    }

    // ── TXT validation ──

    #[test]
    fn txt_length_check() {
        let policy = ValidationPolicy {
            max_txt_length: 50,
            ..Default::default()
        };
        let t = tag(&["record", "TXT", "@", "3600", &"a".repeat(100)]);
        let err = parse_record(&t, &policy).unwrap_err();
        assert!(err.to_string().contains("exceeds max length"));
    }

    #[test]
    fn txt_empty_rdata_ok() {
        let t = tag(&["record", "TXT", "@", "3600", ""]);
        let rec = parse_record(&t, &ValidationPolicy::default()).unwrap();
        assert_eq!(rec.rtype, "TXT");
    }

    #[test]
    fn txt_non_empty_rdata_required_for_a() {
        let t = tag(&["record", "A", "@", "3600", ""]);
        let err = parse_record(&t, &ValidationPolicy::default()).unwrap_err();
        assert!(err.to_string().contains("requires rdata"));
    }

    // ── Reserved TXT name protections ──

    #[test]
    fn txt_dmarc_blocked() {
        let rec = record("TXT", "_dmarc", 3600, "v=DMARC1; p=none");
        let err = validate_record(&rec, &ValidationPolicy::default()).unwrap_err();
        assert!(err.to_string().contains("_dmarc") && err.to_string().contains("reserved"));
    }

    #[test]
    fn txt_domainkey_blocked() {
        let rec = record("TXT", "_domainkey", 3600, "o=-");
        let err = validate_record(&rec, &ValidationPolicy::default()).unwrap_err();
        assert!(err.to_string().contains("_domainkey") && err.to_string().contains("reserved"));
    }

    #[test]
    fn txt_domainkey_subdomain_blocked() {
        let rec = record("TXT", "_domainkey.selector", 3600, "p=MIGfMA0...");
        let err = validate_record(&rec, &ValidationPolicy::default()).unwrap_err();
        assert!(err.to_string().contains("_domainkey"));
    }

    #[test]
    fn txt_spf_at_apex_blocked() {
        let rec = record("TXT", "@", 3600, "v=spf1 include:_spf.google.com ~all");
        let err = validate_record(&rec, &ValidationPolicy::default()).unwrap_err();
        assert!(err.to_string().contains("SPF"));
    }

    #[test]
    fn txt_spf_with_leading_whitespace_blocked() {
        let rec = record("TXT", "@", 3600, "  v=spf1 include:example.com ~all");
        let err = validate_record(&rec, &ValidationPolicy::default()).unwrap_err();
        assert!(err.to_string().contains("SPF"));
    }

    #[test]
    fn txt_non_reserved_ok() {
        let rec = record("TXT", "@", 3600, "just a normal txt record");
        assert!(validate_record(&rec, &ValidationPolicy::default()).is_ok());
    }

    #[test]
    fn txt_spf_not_at_apex_ok() {
        let rec = record("TXT", "something", 3600, "v=spf1 include:example.com ~all");
        assert!(validate_record(&rec, &ValidationPolicy::default()).is_ok());
    }

    // ── MX / SRV / CNAME / NS validation ──

    #[test]
    fn mx_valid() {
        let rec = record("MX", "@", 3600, "10 mail.example.com");
        assert!(validate_record(&rec, &ValidationPolicy::default()).is_ok());
    }

    #[test]
    fn mx_bad_priority() {
        let rec = record("MX", "@", 3600, "notanumber mail.example.com");
        assert!(validate_record(&rec, &ValidationPolicy::default()).is_err());
    }

    #[test]
    fn mx_missing_exchange() {
        let rec = record("MX", "@", 3600, "10");
        assert!(validate_record(&rec, &ValidationPolicy::default()).is_err());
    }

    #[test]
    fn srv_valid() {
        let rec = record("SRV", "@", 3600, "10 20 443 server.example.com");
        assert!(validate_record(&rec, &ValidationPolicy::default()).is_ok());
    }

    #[test]
    fn srv_too_few_fields() {
        let rec = record("SRV", "@", 3600, "10 20");
        assert!(validate_record(&rec, &ValidationPolicy::default()).is_err());
    }

    #[test]
    fn ns_valid() {
        let rec = record("NS", "@", 3600, "ns1.example.com");
        assert!(validate_record(&rec, &ValidationPolicy::default()).is_ok());
    }

    #[test]
    fn cname_valid() {
        let rec = record("CNAME", "@", 3600, "target.example.com");
        assert!(validate_record(&rec, &ValidationPolicy::default()).is_ok());
    }

    #[test]
    fn cname_with_trailing_dot() {
        let rec = record("CNAME", "@", 3600, "target.example.com.");
        assert!(validate_record(&rec, &ValidationPolicy::default()).is_ok());
    }

    #[test]
    fn cname_invalid_domain() {
        let rec = record("CNAME", "@", 3600, "invalid..double-dot");
        assert!(validate_record(&rec, &ValidationPolicy::default()).is_err());
    }

    // ── DNS label validation ──

    #[test]
    fn dns_label_apex_ok() {
        assert!(validate_dns_label("@").is_ok());
    }

    #[test]
    fn dns_label_empty_ok() {
        assert!(validate_dns_label("").is_ok());
    }

    #[test]
    fn dns_label_simple_alnum() {
        assert!(validate_dns_label("abc123").is_ok());
    }

    #[test]
    fn dns_label_with_hyphens() {
        assert!(validate_dns_label("my-site").is_ok());
    }

    #[test]
    fn dns_label_with_underscore() {
        assert!(validate_dns_label("_dmarc").is_ok());
        assert!(validate_dns_label("_domainkey").is_ok());
    }

    #[test]
    fn dns_label_too_long() {
        let name = "a".repeat(64);
        assert!(validate_dns_label(&name).is_err());
    }

    #[test]
    fn dns_label_starts_with_hyphen() {
        assert!(validate_dns_label("-bad").is_err());
    }

    #[test]
    fn dns_label_ends_with_hyphen() {
        assert!(validate_dns_label("bad-").is_err());
    }

    #[test]
    fn dns_label_uppercase_rejected() {
        assert!(validate_dns_label("CamelCase").is_err());
    }

    // ── CNAME coexistence ──

    #[test]
    fn cname_alone_ok() {
        let records = vec![record("CNAME", "@", 3600, "target.example.com")];
        assert!(validate_record_set(&records).is_ok());
    }

    #[test]
    fn cname_coexist_with_a_same_name_rejected() {
        let records = vec![
            record("A", "@", 3600, "1.2.3.4"),
            record("CNAME", "@", 3600, "target.example.com"),
        ];
        assert!(matches!(
            validate_record_set(&records),
            Err(ParseError::CannotCoexistWithCname)
        ));
    }

    #[test]
    fn cname_different_name_ok() {
        let records = vec![
            record("A", "@", 3600, "1.2.3.4"),
            record("CNAME", "www", 3600, "target.example.com"),
        ];
        assert!(validate_record_set(&records).is_ok());
    }

    // ── parse_records (multi-tag) ──

    #[test]
    fn parse_records_multiple_a() {
        let tags = vec![
            tag(&["record", "A", "@", "3600", "1.2.3.4"]),
            tag(&["record", "A", "www", "3600", "5.6.7.8"]),
        ];
        let records = parse_records(&tags, &ValidationPolicy::default()).unwrap();
        assert_eq!(records.len(), 2);
    }

    #[test]
    fn parse_records_skips_non_record_tags() {
        let tags = vec![
            tag(&["d", "some-id"]),
            tag(&["record", "A", "@", "3600", "1.2.3.4"]),
            tag(&["cashu", "token", "mint", "100"]),
        ];
        let records = parse_records(&tags, &ValidationPolicy::default()).unwrap();
        assert_eq!(records.len(), 1);
    }

    #[test]
    fn parse_records_cname_coexist_rejected() {
        let tags = vec![
            tag(&["record", "A", "@", "3600", "1.2.3.4"]),
            tag(&["record", "CNAME", "@", "3600", "target.example.com"]),
        ];
        let err = parse_records(&tags, &ValidationPolicy::default()).unwrap_err();
        assert!(matches!(err, ParseError::CannotCoexistWithCname));
    }

    #[test]
    fn parse_records_empty_input_returns_empty() {
        let records = parse_records(&[], &ValidationPolicy::default()).unwrap();
        assert!(records.is_empty());
    }

    #[test]
    fn parse_records_reports_tag_index_on_error() {
        let tags = vec![
            tag(&["record", "A", "@", "3600", "1.2.3.4"]),
            tag(&["record", "A", "@", "3600", "10.0.0.1"]), // index 1: private IP
        ];
        let err = parse_records(&tags, &ValidationPolicy::default()).unwrap_err();
        match err {
            ParseError::TagError { index, .. } => assert_eq!(index, 1),
            other => panic!("expected TagError, got {other:?}"),
        }
    }

    #[test]
    fn hostname_simple_valid() {
        assert!(validate_hostname("example.com").is_ok());
    }

    #[test]
    fn hostname_multi_label_valid() {
        assert!(validate_hostname("mail.sub.example.com").is_ok());
    }

    #[test]
    fn hostname_trailing_dot_valid() {
        assert!(validate_hostname("example.com.").is_ok());
    }

    #[test]
    fn hostname_single_label_valid() {
        assert!(validate_hostname("localhost").is_ok());
    }

    #[test]
    fn hostname_with_underscore_valid() {
        assert!(validate_hostname("_dmarc.example.com").is_ok());
        assert!(validate_hostname("_sip._tcp.example.com").is_ok());
    }

    #[test]
    fn hostname_empty_rejected() {
        assert!(validate_hostname("").is_err());
    }

    #[test]
    fn hostname_empty_label_rejected() {
        assert!(validate_hostname("example..com").is_err());
        assert!(validate_hostname(".example.com").is_err());
    }

    #[test]
    fn hostname_label_too_long_rejected() {
        let long_label = "a".repeat(64);
        assert!(validate_hostname(&format!("{long_label}.com")).is_err());
    }

    #[test]
    fn hostname_total_too_long_rejected() {
        let very_long = (0..5)
            .map(|_| "x".repeat(63))
            .collect::<Vec<_>>()
            .join(".");
        assert!(validate_hostname(&very_long).is_err());
    }

    #[test]
    fn hostname_leading_hyphen_in_label_rejected() {
        assert!(validate_hostname("-bad.example.com").is_err());
    }

    #[test]
    fn hostname_trailing_hyphen_in_label_rejected() {
        assert!(validate_hostname("bad-.example.com").is_err());
    }

    #[test]
    fn hostname_special_chars_rejected() {
        assert!(validate_hostname("exam!ple.com").is_err());
        assert!(validate_hostname("example .com").is_err());
    }

    #[test]
    fn hostname_uppercase_allowed() {
        assert!(validate_hostname("Example.COM").is_ok());
    }
}
