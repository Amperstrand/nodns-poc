//! Shared types and constants for the nodns resolver SDK.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub const RECORD_KIND: u64 = 11111;
pub const ZONE_HANDLER_KIND: u64 = 31990;

pub const DEFAULT_ZONE: &str = "nodns.shop";
pub const DEFAULT_API_BASE: &str = "https://nodns.shop";
pub const DEFAULT_DOH_ENDPOINT: &str = "https://cloudflare-dns.com/dns-query";

pub const VALID_RECORD_TYPES: &[&str] = &["A", "AAAA", "CNAME", "TXT", "MX"];

pub const DEFAULT_DNS_TYPES: &[&str] = &["A", "AAAA", "TXT", "CNAME", "MX"];

pub const WILDCARD_REDIRECT_IPS: &[&str] = &["46.224.104.12"];

pub const DEFAULT_RELAYS: &[&str] = &["wss://relay.cashu.email"];

#[must_use]
pub fn dns_type_number_to_string(num: u16) -> String {
    match num {
        1 => "A",
        28 => "AAAA",
        5 => "CNAME",
        16 => "TXT",
        15 => "MX",
        2 => "NS",
        6 => "SOA",
        _ => return num.to_string(),
    }
    .to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct DohAnswer {
    pub name: String,
    #[serde(rename = "type")]
    pub type_num: u16,
    #[serde(rename = "TTL")]
    pub ttl: u32,
    pub data: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DohResponse {
    #[serde(rename = "Status")]
    pub status: u16,
    #[serde(rename = "Answer")]
    pub answer: Option<Vec<DohAnswer>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DnsAnswer {
    #[serde(rename = "type")]
    pub record_type: String,
    pub name: String,
    pub ttl: u32,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DnsRecord {
    pub npub: String,
    pub name: String,
    pub fqdn: String,
    #[serde(rename = "type")]
    pub record_type: String,
    pub ttl: u32,
    pub rdata: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NostrDnsRecord {
    #[serde(rename = "type")]
    pub record_type: String,
    pub name: String,
    pub value: String,
    pub ttl: u32,
    pub fqdn: String,
    pub pubkey: String,
    #[serde(rename = "eventId")]
    pub event_id: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedRecord {
    #[serde(rename = "type")]
    pub record_type: String,
    pub name: String,
    pub ttl: u32,
    pub data: String,
    pub source: Option<String>,
    pub pubkey: Option<String>,
    #[serde(rename = "eventId")]
    pub event_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceStatus {
    Loading,
    Ok,
    Error,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceResult<T> {
    pub source: String,
    pub status: SourceStatus,
    pub records: Vec<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T> SourceResult<T> {
    pub fn new(source: &str, status: SourceStatus, records: Vec<T>) -> Self {
        Self {
            source: source.to_string(),
            status,
            records,
            error: None,
        }
    }

    pub fn with_error(source: &str, error: impl Into<String>) -> Self {
        Self {
            source: source.to_string(),
            status: SourceStatus::Error,
            records: Vec::new(),
            error: Some(error.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TripartiteRecords {
    pub api: SourceResult<DnsRecord>,
    pub nostr: SourceResult<NostrDnsRecord>,
    pub dns: SourceResult<DnsAnswer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TripartiteComparison {
    pub is_match: bool,
    pub api_count: usize,
    pub nostr_count: usize,
    pub dns_count: usize,
    pub only_in_api: Vec<String>,
    pub only_in_nostr: Vec<String>,
    pub only_in_dns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveVerifiedResult {
    pub records: Vec<ResolvedRecord>,
    pub verified: bool,
    pub sources: TripartiteRecords,
    pub comparison: TripartiteComparison,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReverseResult {
    pub names: Vec<String>,
    pub records: Vec<NostrDnsRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ZoneStatusLevel {
    Testing,
    Preview,
    Production,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZonePricing {
    pub create: u64,
    pub update: u64,
    pub delete: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneStatus {
    pub zone: String,
    pub pubkey: String,
    pub status: ZoneStatusLevel,
    pub testnet: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dnskey_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dnskey_alg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pricing: Option<ZonePricing>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web: Option<String>,
    pub verified: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_error: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResolveMode {
    #[default]
    Dns,
    Nostr,
    Tripartite,
}

pub type ResolutionMode = ResolveMode;

pub type ZoneTxtFields = HashMap<String, String>;

#[derive(Debug, Deserialize)]
pub(crate) struct ApiRecordsResponse {
    pub records: Option<Vec<DnsRecord>>,
}
