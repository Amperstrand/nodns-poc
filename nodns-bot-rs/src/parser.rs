//! Nostr event tag parser for DNS record events.
//!
//! Port of `nodns-bot/internal/nostr/parser.go`. Handles kind 11111 events,
//! extracting DNS records, delegations, registrar keys, and payment proofs
//! from Nostr event tags.

use std::collections::HashSet;

use nostr_sdk::prelude::*;
use thiserror::Error;

use nodns_protocol::{self, ValidationPolicy};

use crate::types::{
    is_dns_kind, ClaimRequest, Delegation, DeleteRequest, DnsRecord, ParsedEvent, Payment,
    RegistrarKey, RenewalRequest,
};

#[cfg(test)]
use nodns_protocol::is_private_ip;
#[cfg(test)]
use std::net::IpAddr;

#[derive(Error, Debug)]
pub enum ParserError {
    #[error("expected kind 11111 or 31111, got {got}")]
    WrongKind { got: u64 },
    #[error("tag {index}: {message}")]
    TagError { index: usize, message: String },
    #[error("{0}")]
    Validation(String),
    #[error("no recognized tags found (need record, delete, delegation, or registrar)")]
    NoRecognizedTags,
    #[error("CNAME records cannot coexist with other record types at the same name")]
    CannotCoexistWithCname,
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
    let kind = u64::from(event.kind.as_u16());
    if !is_dns_kind(kind) {
        return Err(ParserError::WrongKind { got: kind });
    }

    let d_tag = event
        .tags
        .iter()
        .find(|t| {
            let s = t.as_slice();
            !s.is_empty() && s[0] == "d"
        })
        .and_then(|t| t.as_slice().get(1).cloned());

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
        d_tag,
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
            "unsupported delete type: {rtype}"
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
/// - `VALID_UNTIL` is a valid unix timestamp
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
    let new_valid_until: i64 = tag[3].parse().map_err(|e| {
        ParserError::Validation(format!("invalid new_valid_until {:?}: {}", tag[3], e))
    })?;

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
                let amount: i64 = slice[3].parse().map_err(|e| {
                    ParserError::Validation(format!("invalid cashu amount {:?}: {}", slice[3], e))
                })?;
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
                let amount: i64 = slice[2].parse().map_err(|e| {
                    ParserError::Validation(format!("invalid zap amount {:?}: {}", slice[2], e))
                })?;
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

    let normalized = normalize_legacy_tag(tag);

    let policy = ValidationPolicy {
        allowed_types: allowed_types.to_vec(),
        block_private_ip,
        max_txt_length,
    };

    let rec = nodns_protocol::parse_record(&normalized, &policy)
        .map_err(|e| ParserError::Validation(e.to_string()))?;

    Ok(DnsRecord {
        record_type: rec.rtype,
        name: rec.name,
        ttl: rec.ttl,
        rdata: rec.rdata,
    })
}

fn normalize_legacy_tag(tag: &[String]) -> Vec<String> {
    if tag.len() == 11 && tag[0] == "record" {
        let rdata_parts: Vec<&str> = (3..=9)
            .filter(|&i| !tag[i].is_empty())
            .map(|i| tag[i].as_str())
            .collect();
        vec![
            "record".to_string(),
            tag[1].clone(),
            tag[2].clone(),
            tag[10].clone(),
            rdata_parts.join(" "),
        ]
    } else {
        tag.to_vec()
    }
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
                    "DNS label must be lowercase, found uppercase: '{ch}'"
                )));
            }
            return Err(ParserError::Validation(format!(
                "DNS label contains invalid character: '{ch}'"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
fn validate_record(
    rec: &DnsRecord,
    block_private_ip: bool,
    max_txt_length: usize,
) -> Result<(), ParserError> {
    let proto_rec = nodns_protocol::Record {
        rtype: rec.record_type.clone(),
        name: rec.name.clone(),
        ttl: rec.ttl,
        rdata: rec.rdata.clone(),
    };
    let policy = ValidationPolicy {
        allowed_types: vec![],
        block_private_ip,
        max_txt_length,
    };
    nodns_protocol::validate_record(&proto_rec, &policy)
        .map_err(|e| ParserError::Validation(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::DEFAULT_TTL;

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
            "alice.test.shop".to_string(),
            "npub1abc".to_string(),
            "1000".to_string(),
            "2000".to_string(),
            "1500".to_string(),
        ];
        let d = parse_delegation_tag(&tag).unwrap();
        assert_eq!(d.domain, "alice.test.shop");
        assert_eq!(d.npub, "npub1abc");
        assert_eq!(d.valid_from, 1000);
        assert_eq!(d.valid_until, 2000);
        assert_eq!(d.renew_by, 1500);
    }

    #[test]
    fn test_parse_delegation_tag_too_short() {
        let tag: Vec<String> = vec!["delegation".to_string(), "alice.test.shop".to_string()];
        let err = parse_delegation_tag(&tag).unwrap_err();
        assert!(err.to_string().contains("must have 6 elements"));
    }

    #[test]
    fn test_parse_registrar_tag_valid() {
        let tag: Vec<String> = vec![
            "registrar".to_string(),
            "test.shop".to_string(),
            "abcdef123456".to_string(),
        ];
        let r = parse_registrar_tag(&tag).unwrap();
        assert_eq!(r.zone, "test.shop");
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
        assert!(err.to_string().contains("must have 5 elements"));
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
        let tags = Tags::from_list(vec![t1, t2]);
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
        let tags = Tags::from_list(vec![t1, t2]);
        let payments = parse_payment_tags(&tags).unwrap();
        assert_eq!(payments.len(), 1);
        assert_eq!(payments[0].method, "zap");
        assert_eq!(payments[0].token, "receipt_event_id");
        assert_eq!(payments[0].mint_url, "");
        assert_eq!(payments[0].amount, 500);
    }

    #[test]
    fn test_parse_delete_tag_valid() {
        let tag: Vec<String> = vec!["delete".to_string(), "A".to_string(), "".to_string()];
        let del = parse_delete_tag(&tag).unwrap();
        assert_eq!(del.record_type, "A");
        assert_eq!(del.name, "@");
    }

    #[test]
    fn test_parse_delete_tag_with_subdomain() {
        let tag: Vec<String> = vec!["delete".to_string(), "TXT".to_string(), "www".to_string()];
        let del = parse_delete_tag(&tag).unwrap();
        assert_eq!(del.record_type, "TXT");
        assert_eq!(del.name, "www");
    }

    #[test]
    fn test_parse_delete_tag_too_short() {
        let tag: Vec<String> = vec!["delete".to_string(), "A".to_string()];
        let err = parse_delete_tag(&tag).unwrap_err();
        assert!(err.to_string().contains("must have 3 elements"));
    }

    #[test]
    fn test_parse_delete_tag_unknown_type() {
        let tag: Vec<String> = vec!["delete".to_string(), "UNKNOWN".to_string(), "".to_string()];
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
            .tags(vec![Tag::parse([
                "record",
                "CNAME",
                "@",
                "3600",
                "target.example.com",
            ])
            .unwrap()])
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
            .tags(vec![Tag::parse([
                "claim",
                "alice",
                "nodns.shop",
                "1780704000",
            ])
            .unwrap()])
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
            .tags(vec![Tag::parse([
                "renewal",
                "alice",
                "nodns.shop",
                "1780704000",
            ])
            .unwrap()])
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

    #[test]
    fn test_parse_record_tag_aaaa_blocks_private_ipv6() {
        let tag_fc00: Vec<String> = vec![
            "record".to_string(),
            "AAAA".to_string(),
            "@".to_string(),
            "3600".to_string(),
            "fc00::1".to_string(),
        ];
        let err = parse_record_tag(&tag_fc00, &[], true, 0).unwrap_err();
        assert!(err.to_string().contains("private IP address blocked"));

        let tag_loopback: Vec<String> = vec![
            "record".to_string(),
            "AAAA".to_string(),
            "@".to_string(),
            "3600".to_string(),
            "::1".to_string(),
        ];
        let err = parse_record_tag(&tag_loopback, &[], true, 0).unwrap_err();
        assert!(err.to_string().contains("private IP address blocked"));
    }

    #[test]
    fn test_parse_record_tag_aaaa_allows_public_ipv6() {
        let tag: Vec<String> = vec![
            "record".to_string(),
            "AAAA".to_string(),
            "@".to_string(),
            "3600".to_string(),
            "2001:db8::1".to_string(),
        ];
        let rec = parse_record_tag(&tag, &[], true, 0).unwrap();
        assert_eq!(rec.record_type, "AAAA");
        assert_eq!(rec.rdata, "2001:db8::1");
    }

    #[test]
    fn test_parse_record_tag_invalid_a_ip_format() {
        let tag: Vec<String> = vec![
            "record".to_string(),
            "A".to_string(),
            "@".to_string(),
            "3600".to_string(),
            "not.an.ip".to_string(),
        ];
        let err = parse_record_tag(&tag, &[], false, 0).unwrap_err();
        assert!(err.to_string().contains("invalid IPv4 address"));
    }

    #[test]
    fn test_parse_record_tag_cname_invalid_domain() {
        let tag: Vec<String> = vec![
            "record".to_string(),
            "CNAME".to_string(),
            "@".to_string(),
            "3600".to_string(),
            "invalid..double..dot".to_string(),
        ];
        let err = parse_record_tag(&tag, &[], false, 0).unwrap_err();
        assert!(err.to_string().contains("empty label") || err.to_string().contains("domain"));
    }

    #[test]
    fn test_parse_record_tag_empty_rdata_a_rejected() {
        let tag: Vec<String> = vec![
            "record".to_string(),
            "A".to_string(),
            "@".to_string(),
            "3600".to_string(),
            "".to_string(),
        ];
        let err = parse_record_tag(&tag, &[], false, 0).unwrap_err();
        assert!(err.to_string().contains("A record requires rdata"));
    }

    #[test]
    fn test_parse_record_tag_empty_rdata_txt_ok() {
        let tag: Vec<String> = vec![
            "record".to_string(),
            "TXT".to_string(),
            "@".to_string(),
            "3600".to_string(),
            "".to_string(),
        ];
        let rec = parse_record_tag(&tag, &[], false, 0).unwrap();
        assert_eq!(rec.record_type, "TXT");
        assert!(rec.rdata.is_empty());
    }

    #[test]
    fn test_validate_record_ns_valid() {
        let rec = DnsRecord {
            record_type: "NS".to_string(),
            name: "@".to_string(),
            ttl: 3600,
            rdata: "ns1.example.com".to_string(),
        };
        assert!(validate_record(&rec, false, 0).is_ok());
    }

    #[test]
    fn test_classify_event_multiple_a_records() {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::Custom(11111), "")
            .tags(vec![
                Tag::parse(["record", "A", "@", "3600", "1.2.3.4"]).unwrap(),
                Tag::parse(["record", "A", "www", "3600", "5.6.7.8"]).unwrap(),
            ])
            .sign_with_keys(&keys)
            .unwrap();
        let result = classify_event(&event, &[], false, 0).unwrap();
        assert_eq!(result.records.len(), 2);
        assert_eq!(result.records[0].name, "@");
        assert_eq!(result.records[1].name, "www");
    }

    #[test]
    fn test_classify_event_accepts_kind_31111() {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::Custom(31111), "")
            .tags(vec![
                Tag::parse(["d", "A:@.nodns.shop"]).unwrap(),
                Tag::parse(["record", "A", "@", "3600", "1.2.3.4"]).unwrap(),
            ])
            .sign_with_keys(&keys)
            .unwrap();
        let result = classify_event(&event, &[], false, 0).unwrap();
        assert_eq!(result.records.len(), 1);
        assert_eq!(result.records[0].record_type, "A");
        assert_eq!(result.records[0].rdata, "1.2.3.4");
    }

    #[test]
    fn test_classify_event_31111_extracts_d_tag() {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::Custom(31111), "")
            .tags(vec![
                Tag::parse(["d", "TXT:hello.nodns.shop"]).unwrap(),
                Tag::parse(["record", "TXT", "hello", "3600", "world"]).unwrap(),
            ])
            .sign_with_keys(&keys)
            .unwrap();
        let result = classify_event(&event, &[], false, 0).unwrap();
        assert_eq!(result.d_tag.as_deref(), Some("TXT:hello.nodns.shop"));
    }

    #[test]
    fn test_classify_event_11111_d_tag_none_when_absent() {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::Custom(11111), "")
            .tags(vec![
                Tag::parse(["record", "A", "@", "3600", "1.2.3.4"]).unwrap()
            ])
            .sign_with_keys(&keys)
            .unwrap();
        let result = classify_event(&event, &[], false, 0).unwrap();
        assert!(result.d_tag.is_none());
    }

    #[test]
    fn test_classify_event_rejects_unrelated_kind() {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::Custom(22222), "")
            .tags(vec![
                Tag::parse(["record", "A", "@", "3600", "1.2.3.4"]).unwrap()
            ])
            .sign_with_keys(&keys)
            .unwrap();
        let err = classify_event(&event, &[], false, 0).unwrap_err();
        assert!(err.to_string().contains("expected kind 11111 or 31111"));
        assert!(err.to_string().contains("22222"));
    }
}
