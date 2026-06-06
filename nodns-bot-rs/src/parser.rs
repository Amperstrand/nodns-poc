//! Nostr event tag parser for DNS record events.
//!
//! Port of `nodns-bot/internal/nostr/parser.go`. Handles kind 11111 events,
//! extracting DNS records, delegations, registrar keys, and payment proofs
//! from Nostr event tags.

use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use nostr_sdk::prelude::*;
use thiserror::Error;

use crate::types::{
    Delegation, DnsRecord, Payment, ParsedEvent, RegistrarKey, DEFAULT_TTL, KIND_DNS_RECORD,
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
    #[error("no recognized tags found (need record, delegation, or registrar)")]
    NoRecognizedTags,
    #[error("content must be empty string")]
    ContentNotEmpty,
    #[error("no record tags found")]
    NoRecordTags,
}

/// Private/reserved IP networks that should be blocked.
const PRIVATE_IPV4_NETS: &[(&str, u8)] = &[
    ("10.0.0.0", 8),
    ("172.16.0.0", 12),
    ("192.168.0.0", 16),
    ("127.0.0.0", 8),
    ("169.254.0.0", 16),
    ("0.0.0.0", 8),
    ("100.64.0.0", 10),
];

const PRIVATE_IPV6_NETS: &[(&str, u8)] = &[
    ("fc00::", 7),  // Unique local addresses
    ("fe80::", 10), // Link-local addresses
    ("::1", 128),   // Loopback
];

/// Check if an IPv4 address is in a given CIDR network.
fn ipv4_in_network(ip: Ipv4Addr, network_addr: &str, prefix_len: u8) -> bool {
    let net_ip: Ipv4Addr = network_addr.parse().expect("valid IPv4");
    if prefix_len == 0 {
        return true;
    }
    let mask = if prefix_len >= 32 {
        u32::MAX
    } else {
        !((1u32 << (32 - prefix_len)) - 1)
    };
    (u32::from(ip) & mask) == (u32::from(net_ip) & mask)
}

/// Check if an IPv6 address is in a given CIDR network.
fn ipv6_in_network(ip: Ipv6Addr, network_addr: &str, prefix_len: u8) -> bool {
    let net_ip: Ipv6Addr = network_addr.parse().expect("valid IPv6");
    if prefix_len == 0 {
        return true;
    }
    let mask = if prefix_len >= 128 {
        u128::MAX
    } else {
        !((1u128 << (128 - prefix_len)) - 1)
    };
    (u128::from(ip) & mask) == (u128::from(net_ip) & mask)
}

/// Check if an IP address is in a private/reserved range.
fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            for &(addr, prefix) in PRIVATE_IPV4_NETS {
                if ipv4_in_network(v4, addr, prefix) {
                    return true;
                }
            }
            false
        }
        IpAddr::V6(v6) => {
            for &(addr, prefix) in PRIVATE_IPV6_NETS {
                if ipv6_in_network(v6, addr, prefix) {
                    return true;
                }
            }
            false
        }
    }
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
        delegation: None,
        registrar: None,
        payments: Vec::new(),
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
            _ => {}
        }
    }

    let payments = parse_payment_tags(&event.tags).map_err(|e| ParserError::TagError {
        index: 0,
        message: e.to_string(),
    })?;
    result.payments = payments;

    if result.records.is_empty()
        && result.delegation.is_none()
        && result.registrar.is_none()
    {
        return Err(ParserError::NoRecognizedTags);
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
            let ip: IpAddr = rec
                .rdata
                .parse()
                .map_err(|_| {
                    ParserError::Validation(format!("invalid IPv4 address: {}", rec.rdata))
                })?;
            if !ip.is_ipv4() {
                return Err(ParserError::Validation(format!(
                    "invalid IPv4 address: {}",
                    rec.rdata
                )));
            }
            if block_private_ip && is_private_ip(ip) {
                return Err(ParserError::Validation(format!(
                    "private IP address blocked: {}",
                    rec.rdata
                )));
            }
        }
        "AAAA" => {
            let ip: IpAddr = rec
                .rdata
                .parse()
                .map_err(|_| {
                    ParserError::Validation(format!("invalid IPv6 address: {}", rec.rdata))
                })?;
            if block_private_ip && is_private_ip(ip) {
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
        }
        "TXT" => {
            // Any content allowed, length already checked above
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
}
