//! Nostr event tag parser for DNS record events.
//!
//! Port of `nodns-bot/internal/nostr/parser.go`. Handles kind 11111 events,
//! extracting DNS records, delegations, registrar keys, and payment proofs
//! from Nostr event tags.

use std::collections::HashSet;
use std::net::IpAddr;
use std::str::FromStr;

use nostr_sdk::prelude::*;
use thiserror::Error;

use crate::types::{
    ClaimRequest, Delegation, DeleteRequest, DnsRecord, Payment, ParsedEvent, RegistrarKey, RenewalRequest, DEFAULT_TTL, KIND_DNS_RECORD,
};

#[derive(Error, Debug)]
pub enum ParserError {
    #[error("nil event")]
    NilEvent,
    #[error("expected kind {expected}, got {got}")]
    WrongKind { expected: u64, got: u64 },
    #[error("tag {index}: {message}")]
    TagError { index: usize, message: String },
    #[error("{0}")]
    Validation(String),
    #[error("no recognized tags found (need record, delete, delegation, or registrar)")]
    NoRecognizedTags,
    #[error("content must be empty string")]
    ContentNotEmpty,
    #[error("no record tags found")]
    NoRecordTags,
    #[error("CNAME records cannot coexist with other record types at the same name")]
    CannotCoexistWithCname,
}

/// Private/reserved IP networks that should be blocked.
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

/// Check if an IP address is in a private/reserved range.
fn is_private_ip(ip: IpAddr) -> bool {
    PRIVATE_NETWORKS
        .iter()
        .filter_map(|cidr| ipnet::IpNet::from_str(cidr).ok())
        .any(|net| net.contains(&ip))
}

/// Parse all tags and classify the event.
///
/// Content is allowed to be non-empty (it can be a description string).
pub fn classify_event(
    event: &Event,
    allowed_types: &[String],
    block_private_ip: bool,
    max_txt_length: usize,
) -> Result<ParsedEvent, ParserError> {
    if event.kind.as_u16() != KIND_DNS_RECORD as u16 {
        return Err(ParserError::WrongKind {
            expected: KIND_DNS_RECORD,
            got: event.kind.as_u16() as u64,
        });
    }

    let mut result = ParsedEvent {
        records: Vec::new(),
        deletes: Vec::new(),
        delegation: None,
        registrar: None,
        payments: Vec::new(),
        claim: None,
        renewal: None,
        sig: hex::encode(event.sig.serialize()),
        raw_tags: event.tags.iter().map(|t| t.as_slice().to_vec()).collect(),
    };

    for (i, tag) in event.tags.iter().enumerate() {
        let slice = tag.as_slice();
        if slice.is_empty() {
            continue;
        }
        match slice[0].as_str() {
            "record" => {
                let rec = parse_record_tag(slice, allowed_types, block_private_ip, max_txt_length)
                    .map_err(|e| ParserError::TagError {
                    index: i,
                    message: e.to_string(),
                })?;
                result.records.push(rec);
            }
            "delegation" => {
                if result.delegation.is_some() {
                    return Err(ParserError::TagError {
                        index: i,
                        message: "duplicate delegation tag".to_string(),
                    });
                }
                let d = parse_delegation_tag(slice).map_err(|e| ParserError::TagError {
                    index: i,
                    message: e.to_string(),
                })?;
                result.delegation = Some(d);
            }
            "registrar" => {
                if result.registrar.is_some() {
                    return Err(ParserError::TagError {
                        index: i,
                        message: "duplicate registrar tag".to_string(),
                    });
                }
                let r = parse_registrar_tag(slice).map_err(|e| ParserError::TagError {
                    index: i,
                    message: e.to_string(),
                })?;
                result.registrar = Some(r);
            }
            "claim" => {
                if result.claim.is_some() {
                    return Err(ParserError::TagError {
                        index: i,
                        message: "duplicate claim tag".to_string(),
                    });
                }
                let c = parse_claim_tag(slice).map_err(|e| ParserError::TagError {
                    index: i,
                    message: e.to_string(),
                })?;
                result.claim = Some(c);
            }
            "renewal" => {
                if result.renewal.is_some() {
                    return Err(ParserError::TagError {
                        index: i,
                        message: "duplicate renewal tag".to_string(),
                    });
                }
                let r = parse_renewal_tag(slice).map_err(|e| ParserError::TagError {
                    index: i,
                    message: e.to_string(),
                })?;
                result.renewal = Some(r);
            }
            "delete" => {
                let del = parse_delete_tag(slice).map_err(|e| ParserError::TagError {
                    index: i,
                    message: e.to_string(),
                })?;
                result.deletes.push(del);
            }
            _ => {}
        }
    }

    let payments = parse_payment_tags(&event.tags).map_err(|e| ParserError::TagError {
        index: 0,
        message: e.to_string(),
    })?;
    result.payments = payments;

    if result.records.is_empty()
        && result.deletes.is_empty()
        && result.delegation.is_none()
        && result.registrar.is_none()
        && result.claim.is_none()
        && result.renewal.is_none()
    {
        return Err(ParserError::NoRecognizedTags);
    }

    // RFC 1912: CNAME cannot coexist with other record types at the same name.
    // We check per-name groups since each name forms its own RRset.
    {
        let mut names_with_cname: HashSet<&str> = HashSet::new();
        let mut names_with_others: HashSet<&str> = HashSet::new();
        for rec in &result.records {
            if rec.record_type == "CNAME" {
                names_with_cname.insert(&rec.name);
            } else {
                names_with_others.insert(&rec.name);
            }
        }
        for name in &names_with_cname {
            if names_with_others.contains(name) {
                return Err(ParserError::CannotCoexistWithCname);
            }
        }
    }

    Ok(result)
}

/// Backward-compatible entry point that enforces content-must-be-empty.
/// New callers should use `classify_event`.
pub fn parse_event(
    event: &Event,
    allowed_types: &[String],
    block_private_ip: bool,
    max_txt_length: usize,
) -> Result<Vec<DnsRecord>, ParserError> {
    if !event.content.is_empty() {
        return Err(ParserError::ContentNotEmpty);
    }
    let parsed = classify_event(event, allowed_types, block_private_ip, max_txt_length)?;
    if parsed.records.is_empty() {
        return Err(ParserError::NoRecordTags);
    }
    Ok(parsed.records)
}

/// Parse a delegation tag.
///
/// Format: `["delegation", DOMAIN, NPUB, VALID_FROM, VALID_UNTIL, RENEW_BY]`
pub fn parse_delegation_tag(tag: &[String]) -> Result<Delegation, ParserError> {
    if tag.len() < 6 {
        return Err(ParserError::Validation(format!(
            "delegation tag must have 6 elements, got {}",
            tag.len()
        )));
    }
    if tag[0] != "delegation" {
        return Err(ParserError::Validation(
            "first element must be 'delegation'".to_string(),
        ));
    }
    if tag[1].is_empty() {
        return Err(ParserError::Validation(
            "delegation domain cannot be empty".to_string(),
        ));
    }
    if tag[2].is_empty() {
        return Err(ParserError::Validation(
            "delegation npub cannot be empty".to_string(),
        ));
    }

    let valid_from = tag[3]
        .parse::<i64>()
        .map_err(|e| ParserError::Validation(format!("invalid valid_from {:?}: {}", tag[3], e)))?;
    let valid_until = tag[4]
        .parse::<i64>()
        .map_err(|e| ParserError::Validation(format!("invalid valid_until {:?}: {}", tag[4], e)))?;
    let renew_by = tag[5]
        .parse::<i64>()
        .map_err(|e| ParserError::Validation(format!("invalid renew_by {:?}: {}", tag[5], e)))?;

    Ok(Delegation {
        domain: tag[1].clone(),
        npub: tag[2].clone(),
        valid_from,
        valid_until,
        renew_by,
    })
}

/// Parse a registrar key publication tag.
///
/// Format: `["registrar", ZONE, PUBKEY_HEX]`
pub fn parse_registrar_tag(tag: &[String]) -> Result<RegistrarKey, ParserError> {
    if tag.len() < 3 {
        return Err(ParserError::Validation(format!(
            "registrar tag must have 3 elements, got {}",
            tag.len()
        )));
    }
    if tag[0] != "registrar" {
        return Err(ParserError::Validation(
            "first element must be 'registrar'".to_string(),
        ));
    }
    if tag[1].is_empty() {
        return Err(ParserError::Validation(
            "registrar zone cannot be empty".to_string(),
        ));
    }
    if tag[2].is_empty() {
        return Err(ParserError::Validation(
            "registrar pubkey hex cannot be empty".to_string(),
        ));
    }
    Ok(RegistrarKey {
        zone: tag[1].clone(),
        pubkey_hex: tag[2].clone(),
    })
}

pub fn parse_delete_tag(tag: &[String]) -> Result<DeleteRequest, ParserError> {
    if tag.len() < 3 {
        return Err(ParserError::Validation(format!(
            "delete tag must have 3 elements, got {}",
            tag.len()
        )));
    }
    if tag[0] != "delete" {
        return Err(ParserError::Validation(
            "first element must be 'delete'".to_string(),
        ));
    }
    let rtype = tag[1].to_uppercase();
    if rtype.is_empty() {
        return Err(ParserError::Validation(
            "delete record type cannot be empty".to_string(),
        ));
    }
    let known = ["A", "AAAA", "CNAME", "TXT", "MX", "SRV", "NS", "PTR"];
    if !known.contains(&rtype.as_str()) {
        return Err(ParserError::Validation(format!(
            "unsupported delete type: {}",
            rtype
        )));
    }
    let name = if tag[2].is_empty() {
        "@".to_string()
    } else {
        tag[2].clone()
    };
    Ok(DeleteRequest {
        record_type: rtype,
        name,
    })
}

/// Parse a claim tag: `["claim", NAME, ZONE, VALID_UNTIL]`
///
/// Validates:
/// - NAME is a valid DNS label (alphanumeric + hyphen, 1-63 chars, no leading/trailing hyphens)
/// - ZONE is non-empty
/// - VALID_UNTIL is a valid unix timestamp
pub fn parse_claim_tag(tag: &[String]) -> Result<ClaimRequest, ParserError> {
    if tag.len() < 4 {
        return Err(ParserError::Validation(format!(
            "claim tag must have 4 elements, got {}",
            tag.len()
        )));
    }
    if tag[0] != "claim" {
        return Err(ParserError::Validation(
            "first element must be 'claim'".to_string(),
        ));
    }
    let name = &tag[1];
    if name.is_empty() {
        return Err(ParserError::Validation(
            "claim name cannot be empty".to_string(),
        ));
    }
    validate_dns_label(name)?;
    if tag[2].is_empty() {
        return Err(ParserError::Validation(
            "claim zone cannot be empty".to_string(),
        ));
    }
    let valid_until: i64 = tag[3]
        .parse()
        .map_err(|e| ParserError::Validation(format!("invalid valid_until {:?}: {}", tag[3], e)))?;

    Ok(ClaimRequest {
        name: name.clone(),
        zone: tag[2].clone(),
        valid_until,
    })
}

/// Parse a renewal tag: `["renewal", NAME, ZONE, NEW_VALID_UNTIL]`
pub fn parse_renewal_tag(tag: &[String]) -> Result<RenewalRequest, ParserError> {
    if tag.len() < 4 {
        return Err(ParserError::Validation(format!(
            "renewal tag must have 4 elements, got {}",
            tag.len()
        )));
    }
    if tag[0] != "renewal" {
        return Err(ParserError::Validation(
            "first element must be 'renewal'".to_string(),
        ));
    }
    let name = &tag[1];
    if name.is_empty() {
        return Err(ParserError::Validation(
            "renewal name cannot be empty".to_string(),
        ));
    }
    validate_dns_label(name)?;
    if tag[2].is_empty() {
        return Err(ParserError::Validation(
            "renewal zone cannot be empty".to_string(),
        ));
    }
    let new_valid_until: i64 = tag[3]
        .parse()
        .map_err(|e| ParserError::Validation(format!("invalid new_valid_until {:?}: {}", tag[3], e)))?;

    Ok(RenewalRequest {
        name: name.clone(),
        zone: tag[2].clone(),
        new_valid_until,
    })
}

/// Parse all cashu and zap payment tags from the event.
pub fn parse_payment_tags(tags: &Tags) -> Result<Vec<Payment>, ParserError> {
    let mut payments = Vec::new();
    for tag in tags.iter() {
        let slice = tag.as_slice();
        if slice.len() < 3 {
            continue;
        }
        match slice[0].as_str() {
            "cashu" => {
                if slice.len() < 4 {
                    return Err(ParserError::Validation(format!(
                        "cashu tag must have 4 elements, got {}",
                        slice.len()
                    )));
                }
                let amount: i64 = slice[3]
                    .parse()
                    .map_err(|e| ParserError::Validation(format!("invalid cashu amount {:?}: {}", slice[3], e)))?;
                payments.push(Payment {
                    method: "cashu".to_string(),
                    token: slice[1].clone(),
                    mint_url: slice[2].clone(),
                    amount,
                });
            }
            "zap" => {
                if slice.len() < 3 {
                    return Err(ParserError::Validation(format!(
                        "zap tag must have at least 3 elements, got {}",
                        slice.len()
                    )));
                }
                let amount: i64 = slice[2]
                    .parse()
                    .map_err(|e| ParserError::Validation(format!("invalid zap amount {:?}: {}", slice[2], e)))?;
                payments.push(Payment {
                    method: "zap".to_string(),
                    token: slice[1].clone(),
                    mint_url: String::new(),
                    amount,
                });
            }
            _ => {}
        }
    }
    Ok(payments)
}

/// Parse a single record tag in either 5-element or 11-element format.
pub fn parse_record_tag(
    tag: &[String],
    allowed_types: &[String],
    block_private_ip: bool,
    max_txt_length: usize,
) -> Result<DnsRecord, ParserError> {
    if tag.is_empty() || tag[0] != "record" {
        return Err(ParserError::Validation(
            "first element must be 'record'".to_string(),
        ));
    }

    let allowed_set: HashSet<String> = allowed_types.iter().map(|t| t.to_uppercase()).collect();

    match tag.len() {
        5 => parse_new_format(tag, &allowed_set, block_private_ip, max_txt_length),
        11 => parse_legacy_format(tag, &allowed_set, block_private_ip, max_txt_length),
        _ => Err(ParserError::Validation(format!(
            "record tag must have 5 or 11 elements, got {}",
            tag.len()
        ))),
    }
}

/// Handle 5-element format: `["record", "TYPE", "name", "TTL", "rdata"]`.
fn parse_new_format(
    tag: &[String],
    allowed_types: &HashSet<String>,
    block_private_ip: bool,
    max_txt_length: usize,
) -> Result<DnsRecord, ParserError> {
    let rtype = tag[1].to_uppercase();
    if rtype.is_empty() {
        return Err(ParserError::Validation(
            "record type cannot be empty".to_string(),
        ));
    }
    if !allowed_types.is_empty() && !allowed_types.contains(&rtype) {
        return Err(ParserError::Validation(format!(
            "record type {:?} not allowed",
            rtype
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
            .map_err(|e| ParserError::Validation(format!("invalid TTL {:?}: {}", tag[3], e)))?;
        ttl = parsed;
    }
    if ttl == 0 {
        ttl = DEFAULT_TTL;
    }

    let rec = DnsRecord {
        record_type: rtype,
        name,
        rdata,
        ttl,
    };

    validate_record(&rec, block_private_ip, max_txt_length)?;

    Ok(rec)
}

/// Handle 11-element legacy format:
/// `["record", "TYPE", "name", "pos1", "pos2", "pos3", "pos4", "pos5", "pos6", "pos7", "ttl"]`
fn parse_legacy_format(
    tag: &[String],
    allowed_types: &HashSet<String>,
    block_private_ip: bool,
    max_txt_length: usize,
) -> Result<DnsRecord, ParserError> {
    let rtype = tag[1].to_uppercase();
    if rtype.is_empty() {
        return Err(ParserError::Validation(
            "record type cannot be empty".to_string(),
        ));
    }
    if !allowed_types.is_empty() && !allowed_types.contains(&rtype) {
        return Err(ParserError::Validation(format!(
            "record type {:?} not allowed",
            rtype
        )));
    }

    let name = if tag[2].is_empty() {
        "@".to_string()
    } else {
        tag[2].clone()
    };

    validate_dns_label(&name)?;

    // Reconstruct rdata from positions 3-9 (indices 3..=9) by joining non-empty values
    let rdata_parts: Vec<&str> = (3..=9).filter(|&i| !tag[i].is_empty()).map(|i| tag[i].as_str()).collect();
    let rdata = rdata_parts.join(" ");

    // TTL from position 10 (index 10)
    let mut ttl = DEFAULT_TTL;
    if !tag[10].is_empty() {
        let parsed: u32 = tag[10]
            .parse()
            .map_err(|e| ParserError::Validation(format!("invalid TTL {:?}: {}", tag[10], e)))?;
        ttl = parsed;
    }
    if ttl == 0 {
        ttl = DEFAULT_TTL;
    }

    let rec = DnsRecord {
        record_type: rtype,
        name,
        rdata,
        ttl,
    };

    validate_record(&rec, block_private_ip, max_txt_length)?;

    Ok(rec)
}

/// Validate a DNS label (the `name` field that becomes a subdomain label).
///
/// Returns Ok for "@" (apex) and empty strings. Otherwise enforces:
/// - max 63 characters
/// - only lowercase alphanumeric and hyphens
/// - cannot start or end with a hyphen
pub fn validate_dns_label(name: &str) -> Result<(), ParserError> {
    if name == "@" || name.is_empty() {
        return Ok(());
    }
    if name.len() > 63 {
        return Err(ParserError::Validation(format!(
            "DNS label too long: {} characters (max 63)",
            name.len()
        )));
    }
    if name.starts_with('-') {
        return Err(ParserError::Validation(
            "DNS label cannot start with a hyphen".to_string(),
        ));
    }
    if name.ends_with('-') {
        return Err(ParserError::Validation(
            "DNS label cannot end with a hyphen".to_string(),
        ));
    }
    for ch in name.chars() {
        if !ch.is_ascii_lowercase() && !ch.is_ascii_digit() && ch != '-' && ch != '_' {
            if ch.is_ascii_uppercase() {
                return Err(ParserError::Validation(format!(
                    "DNS label must be lowercase, found uppercase: '{}'",
                    ch
                )));
            }
            return Err(ParserError::Validation(format!(
                "DNS label contains invalid character: '{}'",
                ch
            )));
        }
    }
    Ok(())
}

/// Perform type-specific validation on a parsed record.
fn validate_record(
    rec: &DnsRecord,
    block_private_ip: bool,
    max_txt_length: usize,
) -> Result<(), ParserError> {
    if rec.rdata.is_empty() && rec.record_type != "TXT" {
        return Err(ParserError::Validation(format!(
            "{} record requires rdata",
            rec.record_type
        )));
    }

    if rec.record_type == "TXT" && max_txt_length > 0 && rec.rdata.len() > max_txt_length {
        return Err(ParserError::Validation(format!(
            "TXT record exceeds max length {}: got {}",
            max_txt_length,
            rec.rdata.len()
        )));
    }

    let fields: Vec<&str> = rec.rdata.split_whitespace().collect();

    match rec.record_type.as_str() {
        "A" => {
            let a: hickory_proto::rr::rdata::A = rec
                .rdata
                .parse()
                .map_err(|_| {
                    ParserError::Validation(format!("invalid IPv4 address: {}", rec.rdata))
                })?;
            if block_private_ip && is_private_ip(IpAddr::from(*a)) {
                return Err(ParserError::Validation(format!(
                    "private IP address blocked: {}",
                    rec.rdata
                )));
            }
        }
        "AAAA" => {
            let aaaa: hickory_proto::rr::rdata::AAAA = rec
                .rdata
                .parse()
                .map_err(|_| {
                    ParserError::Validation(format!("invalid IPv6 address: {}", rec.rdata))
                })?;
            if block_private_ip && is_private_ip(IpAddr::from(*aaaa)) {
                return Err(ParserError::Validation(format!(
                    "private IP address blocked: {}",
                    rec.rdata
                )));
            }
        }
        "CNAME" | "NS" | "PTR" => {
            if rec.rdata.is_empty() {
                return Err(ParserError::Validation(format!(
                    "{} record requires target domain",
                    rec.record_type
                )));
            }
            rec.rdata
                .parse::<hickory_proto::rr::domain::Name>()
                .map_err(|_| {
                    ParserError::Validation(format!(
                        "invalid {} domain name: {}",
                        rec.record_type, rec.rdata
                    ))
                })?;
        }
        "TXT" => {
            if rec.name == "_dmarc" {
                return Err(ParserError::Validation(
                    "TXT record with name '_dmarc' is reserved (DMARC spoofing protection)"
                        .to_string(),
                ));
            }
            if rec.name.starts_with("_domainkey") {
                return Err(ParserError::Validation(
                    "TXT record with name starting with '_domainkey' is reserved (DKIM spoofing protection)"
                        .to_string(),
                ));
            }
            if rec.name == "@" && rec.rdata.trim().starts_with("v=spf1") {
                return Err(ParserError::Validation(
                    "TXT record at apex with SPF data is reserved (SPF spoofing protection)"
                        .to_string(),
                ));
            }
        }
        "MX" => {
            if fields.len() < 2 {
                return Err(ParserError::Validation(
                    "MX record requires: priority mailserver".to_string(),
                ));
            }
            let _priority: u16 = fields[0].parse().map_err(|_| {
                ParserError::Validation(format!("invalid MX priority: {}", fields[0]))
            })?;
            fields[1]
                .parse::<hickory_proto::rr::domain::Name>()
                .map_err(|_| {
                    ParserError::Validation(format!(
                        "invalid MX exchange domain: {}",
                        fields[1]
                    ))
                })?;
        }
        "SRV" => {
            if fields.len() < 4 {
                return Err(ParserError::Validation(
                    "SRV record requires: priority weight port target".to_string(),
                ));
            }
            for (i, field_name) in ["priority", "weight", "port"].iter().enumerate() {
                let _: u16 = fields[i].parse().map_err(|_| {
                    ParserError::Validation(format!(
                        "invalid SRV {}: {}",
                        field_name, fields[i]
                    ))
                })?;
            }
            fields[3]
                .parse::<hickory_proto::rr::domain::Name>()
                .map_err(|_| {
                    ParserError::Validation(format!(
                        "invalid SRV target domain: {}",
                        fields[3]
                    ))
                })?;
        }
        _ => {
            return Err(ParserError::Validation(format!(
                "unsupported record type: {}",
                rec.record_type
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_private_ip_v4() {
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
    fn test_is_private_ip_v6() {
        assert!(is_private_ip("::1".parse::<IpAddr>().unwrap()));
        assert!(is_private_ip("fc00::1".parse::<IpAddr>().unwrap()));
        assert!(is_private_ip("fe80::1".parse::<IpAddr>().unwrap()));
        assert!(!is_private_ip("2001:db8::1".parse::<IpAddr>().unwrap()));
    }

    #[test]
    fn test_parse_delegation_tag_valid() {
        let tag: Vec<String> = vec![
            "delegation".to_string(),
            "alice.cv".to_string(),
            "npub1abc".to_string(),
            "1000".to_string(),
            "2000".to_string(),
            "1500".to_string(),
        ];
        let d = parse_delegation_tag(&tag).unwrap();
        assert_eq!(d.domain, "alice.cv");
        assert_eq!(d.npub, "npub1abc");
        assert_eq!(d.valid_from, 1000);
        assert_eq!(d.valid_until, 2000);
        assert_eq!(d.renew_by, 1500);
    }

    #[test]
    fn test_parse_delegation_tag_too_short() {
        let tag: Vec<String> = vec![
            "delegation".to_string(),
            "alice.cv".to_string(),
        ];
        let err = parse_delegation_tag(&tag).unwrap_err();
        assert!(err.to_string().contains("must have 6 elements"));
    }

    #[test]
    fn test_parse_registrar_tag_valid() {
        let tag: Vec<String> = vec![
            "registrar".to_string(),
            "cv".to_string(),
            "abcdef123456".to_string(),
        ];
        let r = parse_registrar_tag(&tag).unwrap();
        assert_eq!(r.zone, "cv");
        assert_eq!(r.pubkey_hex, "abcdef123456");
    }

    #[test]
    fn test_parse_record_tag_new_format() {
        let tag: Vec<String> = vec![
            "record".to_string(),
            "A".to_string(),
            "@".to_string(),
            "3600".to_string(),
            "1.2.3.4".to_string(),
        ];
        let rec = parse_record_tag(&tag, &[], false, 0).unwrap();
        assert_eq!(rec.record_type, "A");
        assert_eq!(rec.name, "@");
        assert_eq!(rec.ttl, 3600);
        assert_eq!(rec.rdata, "1.2.3.4");
    }

    #[test]
    fn test_parse_record_tag_blocks_private_ip() {
        let tag: Vec<String> = vec![
            "record".to_string(),
            "A".to_string(),
            "@".to_string(),
            "3600".to_string(),
            "10.0.0.1".to_string(),
        ];
        let err = parse_record_tag(&tag, &[], true, 0).unwrap_err();
        assert!(err.to_string().contains("private IP address blocked"));
    }

    #[test]
    fn test_parse_record_tag_legacy_format() {
        let tag: Vec<String> = vec![
            "record".to_string(),
            "TXT".to_string(),
            "@".to_string(),
            "".to_string(),
            "hello".to_string(),
            "world".to_string(),
            "".to_string(),
            "".to_string(),
            "".to_string(),
            "".to_string(),
            "7200".to_string(),
        ];
        let rec = parse_record_tag(&tag, &[], false, 0).unwrap();
        assert_eq!(rec.record_type, "TXT");
        assert_eq!(rec.rdata, "hello world");
        assert_eq!(rec.ttl, 7200);
    }

    #[test]
    fn test_parse_record_tag_wrong_length() {
        let tag: Vec<String> = vec!["record".to_string(), "A".to_string()];
        let err = parse_record_tag(&tag, &[], false, 0).unwrap_err();
        assert!(err.to_string().contains("must have 5 or 11 elements"));
    }

    #[test]
    fn test_validate_record_mx() {
        let rec = DnsRecord {
            record_type: "MX".to_string(),
            name: "@".to_string(),
            ttl: 3600,
            rdata: "10 mail.example.com".to_string(),
        };
        assert!(validate_record(&rec, false, 0).is_ok());

        let bad_rec = DnsRecord {
            record_type: "MX".to_string(),
            name: "@".to_string(),
            ttl: 3600,
            rdata: "notanumber mail.example.com".to_string(),
        };
        assert!(validate_record(&bad_rec, false, 0).is_err());
    }

    #[test]
    fn test_validate_record_srv() {
        let rec = DnsRecord {
            record_type: "SRV".to_string(),
            name: "@".to_string(),
            ttl: 3600,
            rdata: "10 20 443 server.example.com".to_string(),
        };
        assert!(validate_record(&rec, false, 0).is_ok());

        let bad_rec = DnsRecord {
            record_type: "SRV".to_string(),
            name: "@".to_string(),
            ttl: 3600,
            rdata: "10 20".to_string(),
        };
        assert!(validate_record(&bad_rec, false, 0).is_err());
    }

    #[test]
    fn test_parse_record_tag_default_ttl() {
        let tag: Vec<String> = vec![
            "record".to_string(),
            "A".to_string(),
            "@".to_string(),
            "0".to_string(),
            "1.2.3.4".to_string(),
        ];
        let rec = parse_record_tag(&tag, &[], false, 0).unwrap();
        assert_eq!(rec.ttl, DEFAULT_TTL);
    }

    #[test]
    fn test_parse_record_tag_allowed_types() {
        let allowed = vec!["A".to_string(), "AAAA".to_string()];
        let tag: Vec<String> = vec![
            "record".to_string(),
            "CNAME".to_string(),
            "@".to_string(),
            "3600".to_string(),
            "example.com".to_string(),
        ];
        let err = parse_record_tag(&tag, &allowed, false, 0).unwrap_err();
        assert!(err.to_string().contains("not allowed"));
    }

    #[test]
    fn test_parse_record_tag_empty_name_defaults_to_at() {
        let tag: Vec<String> = vec![
            "record".to_string(),
            "A".to_string(),
            "".to_string(),
            "3600".to_string(),
            "1.2.3.4".to_string(),
        ];
        let rec = parse_record_tag(&tag, &[], false, 0).unwrap();
        assert_eq!(rec.name, "@");
    }

    #[test]
    fn test_txt_length_check() {
        let tag: Vec<String> = vec![
            "record".to_string(),
            "TXT".to_string(),
            "@".to_string(),
            "3600".to_string(),
            "a".repeat(100),
        ];
        let err = parse_record_tag(&tag, &[], false, 50).unwrap_err();
        assert!(err.to_string().contains("exceeds max length"));
    }

    #[test]
    fn test_parse_payment_tags_cashu() {
        let t1 = Tag::parse(["cashu", "token123", "https://mint.example.com", "100"]).unwrap();
        let t2 = Tag::parse(["e", "someeventid", ""]).unwrap();
        let tags = Tags::new(vec![t1, t2]);
        let payments = parse_payment_tags(&tags).unwrap();
        assert_eq!(payments.len(), 1);
        assert_eq!(payments[0].method, "cashu");
        assert_eq!(payments[0].token, "token123");
        assert_eq!(payments[0].mint_url, "https://mint.example.com");
        assert_eq!(payments[0].amount, 100);
    }

    #[test]
    fn test_parse_payment_tags_zap() {
        let t1 = Tag::parse(["zap", "receipt_event_id", "500"]).unwrap();
        let t2 = Tag::parse(["e", "someeventid", ""]).unwrap();
        let tags = Tags::new(vec![t1, t2]);
        let payments = parse_payment_tags(&tags).unwrap();
        assert_eq!(payments.len(), 1);
        assert_eq!(payments[0].method, "zap");
        assert_eq!(payments[0].token, "receipt_event_id");
        assert_eq!(payments[0].mint_url, "");
        assert_eq!(payments[0].amount, 500);
    }

    #[test]
    fn test_parse_delete_tag_valid() {
        let tag: Vec<String> = vec![
            "delete".to_string(),
            "A".to_string(),
            "".to_string(),
        ];
        let del = parse_delete_tag(&tag).unwrap();
        assert_eq!(del.record_type, "A");
        assert_eq!(del.name, "@");
    }

    #[test]
    fn test_parse_delete_tag_with_subdomain() {
        let tag: Vec<String> = vec![
            "delete".to_string(),
            "TXT".to_string(),
            "www".to_string(),
        ];
        let del = parse_delete_tag(&tag).unwrap();
        assert_eq!(del.record_type, "TXT");
        assert_eq!(del.name, "www");
    }

    #[test]
    fn test_parse_delete_tag_too_short() {
        let tag: Vec<String> = vec![
            "delete".to_string(),
            "A".to_string(),
        ];
        let err = parse_delete_tag(&tag).unwrap_err();
        assert!(err.to_string().contains("must have 3 elements"));
    }

    #[test]
    fn test_parse_delete_tag_unknown_type() {
        let tag: Vec<String> = vec![
            "delete".to_string(),
            "UNKNOWN".to_string(),
            "".to_string(),
        ];
        let err = parse_delete_tag(&tag).unwrap_err();
        assert!(err.to_string().contains("unsupported delete type"));
    }

    // ── I-036: validate_dns_label tests ──

    #[test]
    fn test_validate_dns_label_apex_ok() {
        assert!(validate_dns_label("@").is_ok());
    }

    #[test]
    fn test_validate_dns_label_empty_ok() {
        assert!(validate_dns_label("").is_ok());
    }

    #[test]
    fn test_validate_dns_label_simple_alnum() {
        assert!(validate_dns_label("www").is_ok());
    }

    #[test]
    fn test_validate_dns_label_with_hyphens() {
        assert!(validate_dns_label("my-sub-domain").is_ok());
    }

    #[test]
    fn test_validate_dns_label_with_digits() {
        assert!(validate_dns_label("sub123").is_ok());
    }

    #[test]
    fn test_validate_dns_label_max_63_chars() {
        let label = "a".repeat(63);
        assert!(validate_dns_label(&label).is_ok());
    }

    #[test]
    fn test_validate_dns_label_too_long() {
        let label = "a".repeat(64);
        let err = validate_dns_label(&label).unwrap_err();
        assert!(err.to_string().contains("too long"));
    }

    #[test]
    fn test_validate_dns_label_starts_with_hyphen() {
        let err = validate_dns_label("-bad").unwrap_err();
        assert!(err.to_string().contains("start with a hyphen"));
    }

    #[test]
    fn test_validate_dns_label_ends_with_hyphen() {
        let err = validate_dns_label("bad-").unwrap_err();
        assert!(err.to_string().contains("end with a hyphen"));
    }

    #[test]
    fn test_validate_dns_label_uppercase_rejected() {
        let err = validate_dns_label("WWW").unwrap_err();
        assert!(err.to_string().contains("uppercase"));
    }

    #[test]
    fn test_validate_dns_label_special_chars_rejected() {
        let err = validate_dns_label("sub.domain").unwrap_err();
        assert!(err.to_string().contains("invalid character"));
    }

    #[test]
    fn test_validate_dns_label_underscore_accepted() {
        assert!(validate_dns_label("_acme-challenge").is_ok());
        assert!(validate_dns_label("_dmarc").is_ok());
    }

    #[test]
    fn test_dns_label_validated_in_new_format() {
        let tag: Vec<String> = vec![
            "record".to_string(),
            "A".to_string(),
            "WWW".to_string(),
            "3600".to_string(),
            "1.2.3.4".to_string(),
        ];
        let err = parse_record_tag(&tag, &[], false, 0).unwrap_err();
        assert!(err.to_string().contains("uppercase"));
    }

    #[test]
    fn test_dns_label_validated_in_legacy_format() {
        let tag: Vec<String> = vec![
            "record".to_string(),
            "A".to_string(),
            "-bad".to_string(),
            "".to_string(),
            "".to_string(),
            "".to_string(),
            "".to_string(),
            "".to_string(),
            "".to_string(),
            "1.2.3.4".to_string(),
            "3600".to_string(),
        ];
        let err = parse_record_tag(&tag, &[], false, 0).unwrap_err();
        assert!(err.to_string().contains("start with a hyphen"));
    }

    // ── I-034: Reserved TXT name protection tests ──

    #[test]
    fn test_txt_dmarc_blocked() {
        let rec = DnsRecord {
            record_type: "TXT".to_string(),
            name: "_dmarc".to_string(),
            ttl: 3600,
            rdata: "v=DMARC1; p=none".to_string(),
        };
        let err = validate_record(&rec, false, 0).unwrap_err();
        assert!(err.to_string().contains("_dmarc") && err.to_string().contains("reserved"));
    }

    #[test]
    fn test_txt_domainkey_blocked() {
        let rec = DnsRecord {
            record_type: "TXT".to_string(),
            name: "_domainkey".to_string(),
            ttl: 3600,
            rdata: "o=-".to_string(),
        };
        let err = validate_record(&rec, false, 0).unwrap_err();
        assert!(err.to_string().contains("_domainkey") && err.to_string().contains("reserved"));
    }

    #[test]
    fn test_txt_domainkey_subdomain_blocked() {
        let rec = DnsRecord {
            record_type: "TXT".to_string(),
            name: "_domainkey.selector".to_string(),
            ttl: 3600,
            rdata: "p=MIGfMA0...".to_string(),
        };
        let err = validate_record(&rec, false, 0).unwrap_err();
        assert!(err.to_string().contains("_domainkey") && err.to_string().contains("reserved"));
    }

    #[test]
    fn test_txt_spf_at_apex_blocked() {
        let rec = DnsRecord {
            record_type: "TXT".to_string(),
            name: "@".to_string(),
            ttl: 3600,
            rdata: "v=spf1 include:_spf.google.com ~all".to_string(),
        };
        let err = validate_record(&rec, false, 0).unwrap_err();
        assert!(err.to_string().contains("SPF") && err.to_string().contains("reserved"));
    }

    #[test]
    fn test_txt_spf_with_leading_whitespace_blocked() {
        let rec = DnsRecord {
            record_type: "TXT".to_string(),
            name: "@".to_string(),
            ttl: 3600,
            rdata: "  v=spf1 include:example.com ~all".to_string(),
        };
        let err = validate_record(&rec, false, 0).unwrap_err();
        assert!(err.to_string().contains("SPF"));
    }

    #[test]
    fn test_txt_non_reserved_ok() {
        let rec = DnsRecord {
            record_type: "TXT".to_string(),
            name: "@".to_string(),
            ttl: 3600,
            rdata: "just a normal txt record".to_string(),
        };
        assert!(validate_record(&rec, false, 0).is_ok());
    }

    #[test]
    fn test_txt_spf_not_at_apex_ok() {
        let rec = DnsRecord {
            record_type: "TXT".to_string(),
            name: "something".to_string(),
            ttl: 3600,
            rdata: "v=spf1 include:example.com ~all".to_string(),
        };
        assert!(validate_record(&rec, false, 0).is_ok());
    }

    // ── I-035: CNAME coexistence tests ──

    #[test]
    fn test_cname_alone_ok() {
        let cname = DnsRecord {
            record_type: "CNAME".to_string(),
            name: "@".to_string(),
            ttl: 3600,
            rdata: "target.example.com".to_string(),
        };
        assert!(validate_record(&cname, false, 0).is_ok());
    }

    #[test]
    fn test_cname_coexist_with_a_same_name() {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::Custom(11111), "")
            .tags(vec![
                Tag::parse(["record", "A", "@", "3600", "1.2.3.4"]).unwrap(),
                Tag::parse(["record", "CNAME", "@", "3600", "target.example.com"]).unwrap(),
            ])
            .sign_with_keys(&keys)
            .unwrap();
        let err = classify_event(&event, &[], false, 0).unwrap_err();
        assert!(matches!(err, ParserError::CannotCoexistWithCname));
    }

    #[test]
    fn test_cname_coexist_with_txt_same_name() {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::Custom(11111), "")
            .tags(vec![
                Tag::parse(["record", "TXT", "@", "3600", "hello"]).unwrap(),
                Tag::parse(["record", "CNAME", "@", "3600", "target.example.com"]).unwrap(),
            ])
            .sign_with_keys(&keys)
            .unwrap();
        let err = classify_event(&event, &[], false, 0).unwrap_err();
        assert!(matches!(err, ParserError::CannotCoexistWithCname));
    }

    #[test]
    fn test_cname_different_name_ok() {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::Custom(11111), "")
            .tags(vec![
                Tag::parse(["record", "A", "@", "3600", "1.2.3.4"]).unwrap(),
                Tag::parse(["record", "CNAME", "www", "3600", "target.example.com"]).unwrap(),
            ])
            .sign_with_keys(&keys)
            .unwrap();
        let result = classify_event(&event, &[], false, 0).unwrap();
        assert_eq!(result.records.len(), 2);
    }

    #[test]
    fn test_cname_only_single_record_ok() {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::Custom(11111), "")
            .tags(vec![
                Tag::parse(["record", "CNAME", "@", "3600", "target.example.com"]).unwrap(),
            ])
            .sign_with_keys(&keys)
            .unwrap();
        let result = classify_event(&event, &[], false, 0).unwrap();
        assert_eq!(result.records.len(), 1);
        assert_eq!(result.records[0].record_type, "CNAME");
    }

    // ── Claim tag parsing tests ──

    #[test]
    fn test_parse_claim_tag_valid() {
        let tag: Vec<String> = vec![
            "claim".to_string(),
            "alice".to_string(),
            "nodns.shop".to_string(),
            "1780704000".to_string(),
        ];
        let claim = parse_claim_tag(&tag).unwrap();
        assert_eq!(claim.name, "alice");
        assert_eq!(claim.zone, "nodns.shop");
        assert_eq!(claim.valid_until, 1780704000);
    }

    #[test]
    fn test_parse_claim_tag_too_short() {
        let tag: Vec<String> = vec![
            "claim".to_string(),
            "alice".to_string(),
            "nodns.shop".to_string(),
        ];
        let err = parse_claim_tag(&tag).unwrap_err();
        assert!(err.to_string().contains("must have 4 elements"));
    }

    #[test]
    fn test_parse_claim_tag_empty_name() {
        let tag: Vec<String> = vec![
            "claim".to_string(),
            "".to_string(),
            "nodns.shop".to_string(),
            "1780704000".to_string(),
        ];
        let err = parse_claim_tag(&tag).unwrap_err();
        assert!(err.to_string().contains("name cannot be empty"));
    }

    #[test]
    fn test_parse_claim_tag_invalid_name_uppercase() {
        let tag: Vec<String> = vec![
            "claim".to_string(),
            "Alice".to_string(),
            "nodns.shop".to_string(),
            "1780704000".to_string(),
        ];
        let err = parse_claim_tag(&tag).unwrap_err();
        assert!(err.to_string().contains("uppercase"));
    }

    #[test]
    fn test_parse_claim_tag_invalid_name_hyphen_start() {
        let tag: Vec<String> = vec![
            "claim".to_string(),
            "-alice".to_string(),
            "nodns.shop".to_string(),
            "1780704000".to_string(),
        ];
        let err = parse_claim_tag(&tag).unwrap_err();
        assert!(err.to_string().contains("start with a hyphen"));
    }

    #[test]
    fn test_parse_claim_tag_invalid_name_special_chars() {
        let tag: Vec<String> = vec![
            "claim".to_string(),
            "alice.bob".to_string(),
            "nodns.shop".to_string(),
            "1780704000".to_string(),
        ];
        let err = parse_claim_tag(&tag).unwrap_err();
        assert!(err.to_string().contains("invalid character"));
    }

    #[test]
    fn test_parse_claim_tag_empty_zone() {
        let tag: Vec<String> = vec![
            "claim".to_string(),
            "alice".to_string(),
            "".to_string(),
            "1780704000".to_string(),
        ];
        let err = parse_claim_tag(&tag).unwrap_err();
        assert!(err.to_string().contains("zone cannot be empty"));
    }

    #[test]
    fn test_parse_claim_tag_invalid_valid_until() {
        let tag: Vec<String> = vec![
            "claim".to_string(),
            "alice".to_string(),
            "nodns.shop".to_string(),
            "not-a-number".to_string(),
        ];
        let err = parse_claim_tag(&tag).unwrap_err();
        assert!(err.to_string().contains("invalid valid_until"));
    }

    #[test]
    fn test_parse_claim_tag_name_with_hyphens() {
        let tag: Vec<String> = vec![
            "claim".to_string(),
            "my-name".to_string(),
            "nodns.shop".to_string(),
            "1780704000".to_string(),
        ];
        let claim = parse_claim_tag(&tag).unwrap();
        assert_eq!(claim.name, "my-name");
    }

    #[test]
    fn test_parse_claim_tag_name_with_digits() {
        let tag: Vec<String> = vec![
            "claim".to_string(),
            "abc123".to_string(),
            "nodns.shop".to_string(),
            "1780704000".to_string(),
        ];
        let claim = parse_claim_tag(&tag).unwrap();
        assert_eq!(claim.name, "abc123");
    }

    #[test]
    fn test_classify_event_with_claim_tag() {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::Custom(11111), "")
            .tags(vec![
                Tag::parse(["claim", "alice", "nodns.shop", "1780704000"]).unwrap(),
            ])
            .sign_with_keys(&keys)
            .unwrap();
        let result = classify_event(&event, &[], false, 0).unwrap();
        assert!(result.claim.is_some());
        assert!(result.records.is_empty());
        let claim = result.claim.unwrap();
        assert_eq!(claim.name, "alice");
        assert_eq!(claim.zone, "nodns.shop");
    }

    #[test]
    fn test_classify_event_rejects_duplicate_claim() {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::Custom(11111), "")
            .tags(vec![
                Tag::parse(["claim", "alice", "nodns.shop", "1780704000"]).unwrap(),
                Tag::parse(["claim", "bob", "nodns.shop", "1780704000"]).unwrap(),
            ])
            .sign_with_keys(&keys)
            .unwrap();
        let err = classify_event(&event, &[], false, 0).unwrap_err();
        assert!(err.to_string().contains("duplicate claim tag"));
    }

    #[test]
    fn test_classify_event_claim_with_record() {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::Custom(11111), "")
            .tags(vec![
                Tag::parse(["claim", "alice", "nodns.shop", "1780704000"]).unwrap(),
                Tag::parse(["record", "A", "@", "3600", "1.2.3.4"]).unwrap(),
            ])
            .sign_with_keys(&keys)
            .unwrap();
        let result = classify_event(&event, &[], false, 0).unwrap();
        assert!(result.claim.is_some());
        assert_eq!(result.records.len(), 1);
    }

    // ── Renewal tag parsing tests ──

    #[test]
    fn test_parse_renewal_tag_valid() {
        let tag: Vec<String> = vec![
            "renewal".to_string(),
            "alice".to_string(),
            "nodns.shop".to_string(),
            "1780704000".to_string(),
        ];
        let renewal = parse_renewal_tag(&tag).unwrap();
        assert_eq!(renewal.name, "alice");
        assert_eq!(renewal.zone, "nodns.shop");
        assert_eq!(renewal.new_valid_until, 1780704000);
    }

    #[test]
    fn test_parse_renewal_tag_too_short() {
        let tag: Vec<String> = vec![
            "renewal".to_string(),
            "alice".to_string(),
            "nodns.shop".to_string(),
        ];
        let err = parse_renewal_tag(&tag).unwrap_err();
        assert!(err.to_string().contains("must have 4 elements"));
    }

    #[test]
    fn test_parse_renewal_tag_empty_name() {
        let tag: Vec<String> = vec![
            "renewal".to_string(),
            "".to_string(),
            "nodns.shop".to_string(),
            "1780704000".to_string(),
        ];
        let err = parse_renewal_tag(&tag).unwrap_err();
        assert!(err.to_string().contains("name cannot be empty"));
    }

    #[test]
    fn test_parse_renewal_tag_empty_zone() {
        let tag: Vec<String> = vec![
            "renewal".to_string(),
            "alice".to_string(),
            "".to_string(),
            "1780704000".to_string(),
        ];
        let err = parse_renewal_tag(&tag).unwrap_err();
        assert!(err.to_string().contains("zone cannot be empty"));
    }

    #[test]
    fn test_parse_renewal_tag_invalid_timestamp() {
        let tag: Vec<String> = vec![
            "renewal".to_string(),
            "alice".to_string(),
            "nodns.shop".to_string(),
            "not-a-number".to_string(),
        ];
        let err = parse_renewal_tag(&tag).unwrap_err();
        assert!(err.to_string().contains("invalid new_valid_until"));
    }

    #[test]
    fn test_parse_renewal_tag_invalid_name_uppercase() {
        let tag: Vec<String> = vec![
            "renewal".to_string(),
            "Alice".to_string(),
            "nodns.shop".to_string(),
            "1780704000".to_string(),
        ];
        let err = parse_renewal_tag(&tag).unwrap_err();
        assert!(err.to_string().contains("uppercase"));
    }

    #[test]
    fn test_parse_renewal_tag_name_with_hyphens() {
        let tag: Vec<String> = vec![
            "renewal".to_string(),
            "my-name".to_string(),
            "nodns.shop".to_string(),
            "1780704000".to_string(),
        ];
        let renewal = parse_renewal_tag(&tag).unwrap();
        assert_eq!(renewal.name, "my-name");
    }

    #[test]
    fn test_classify_event_with_renewal_tag() {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::Custom(11111), "")
            .tags(vec![
                Tag::parse(["renewal", "alice", "nodns.shop", "1780704000"]).unwrap(),
            ])
            .sign_with_keys(&keys)
            .unwrap();
        let result = classify_event(&event, &[], false, 0).unwrap();
        assert!(result.renewal.is_some());
        assert!(result.records.is_empty());
        let renewal = result.renewal.unwrap();
        assert_eq!(renewal.name, "alice");
        assert_eq!(renewal.zone, "nodns.shop");
        assert_eq!(renewal.new_valid_until, 1780704000);
    }

    #[test]
    fn test_classify_event_rejects_duplicate_renewal() {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::Custom(11111), "")
            .tags(vec![
                Tag::parse(["renewal", "alice", "nodns.shop", "1780704000"]).unwrap(),
                Tag::parse(["renewal", "bob", "nodns.shop", "1780704000"]).unwrap(),
            ])
            .sign_with_keys(&keys)
            .unwrap();
        let err = classify_event(&event, &[], false, 0).unwrap_err();
        assert!(err.to_string().contains("duplicate renewal tag"));
    }
}
