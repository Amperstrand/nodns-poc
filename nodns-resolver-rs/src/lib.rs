//! NoDNS resolver SDK — resolve DNS records from Nostr events via DNS, Nostr, or tripartite verification.
//!
//! ## Usage
//!
//! ```no_run
//! # async fn example() -> anyhow::Result<()> {
//! use nodns_resolver::{Resolver, ResolveMode};
//!
//! let resolver = Resolver::builder()
//!     .mode(ResolveMode::Tripartite)
//!     .build();
//!
//! let records = resolver.resolve("npub1abc.nodns.shop", None).await?;
//! # Ok(())
//! # }
//! ```

pub mod dns;
pub mod nostr;
pub mod parse;
pub mod types;
pub mod verify;
pub mod zones;

pub use dns::query_all_dns_record_types;
pub use dns::query_doh;
pub use nostr::{
    normalize_pubkey, pubkey_to_npub, query_all_recent_records, query_records_by_domain,
    query_records_by_pubkey,
};
pub use parse::{
    compute_fqdn, deduplicate_records, is_npub_derived_name, parse_records_from_event,
};
pub use types::{
    DnsAnswer, DnsRecord, NostrDnsRecord, ResolutionMode, ResolveMode, ResolveVerifiedResult,
    ResolvedRecord, ReverseResult, SourceResult, SourceStatus, TripartiteComparison,
    TripartiteRecords, ZonePricing, ZoneStatus, ZoneStatusLevel,
};
pub use verify::{
    compare_tripartite, fetch_tripartite_records, to_resolved_records, TripartiteParams,
};
pub use zones::{discover_zones, fetch_dns_txt, parse_zone_txt};

use anyhow::Result;

use crate::dns::query_all_dns_record_types as dns_query_all;
use crate::nostr::normalize_pubkey as normalize_pk;
use crate::nostr::{
    query_records_by_domain as nostr_query_domain, query_records_by_pubkey as nostr_query_pubkey,
};
use crate::types::{DEFAULT_API_BASE, DEFAULT_DOH_ENDPOINT, DEFAULT_RELAYS, DEFAULT_ZONE};
use crate::verify::TripartiteParams as TP;

pub struct Resolver {
    mode: ResolveMode,
    relays: Vec<String>,
    api_base: String,
    zone: String,
    doh_endpoint: String,
    http: reqwest::Client,
}

impl Resolver {
    pub fn builder() -> ResolverBuilder {
        ResolverBuilder::new()
    }

    pub fn mode(&self) -> ResolveMode {
        self.mode
    }

    pub fn relays(&self) -> &[String] {
        &self.relays
    }

    pub fn api_base(&self) -> &str {
        &self.api_base
    }

    pub fn zone(&self) -> &str {
        &self.zone
    }

    pub fn doh_endpoint(&self) -> &str {
        &self.doh_endpoint
    }

    pub async fn resolve(
        &self,
        name: &str,
        record_type: Option<&str>,
    ) -> Result<Vec<ResolvedRecord>> {
        match self.mode {
            ResolveMode::Dns => {
                let types: Option<Vec<&str>> = record_type.map(|t| vec![t]);
                let types_ref = types.as_deref();
                let results = dns_query_all(name, types_ref, &self.doh_endpoint, &self.http).await;
                Ok(results
                    .into_iter()
                    .map(|r| ResolvedRecord {
                        record_type: r.record_type,
                        name: r.name,
                        ttl: r.ttl,
                        data: r.data,
                        source: Some("dns".to_string()),
                        pubkey: None,
                        event_id: None,
                    })
                    .collect())
            }
            ResolveMode::Nostr => {
                let records = nostr_query_domain(name, &self.zone, &self.relays).await?;
                Ok(records
                    .into_iter()
                    .filter(|r| record_type.is_none_or(|t| t == r.record_type))
                    .map(|r| ResolvedRecord {
                        record_type: r.record_type,
                        name: r.fqdn,
                        ttl: r.ttl,
                        data: r.value,
                        source: Some("nostr".to_string()),
                        pubkey: Some(r.pubkey),
                        event_id: Some(r.event_id),
                    })
                    .collect())
            }
            ResolveMode::Tripartite => {
                let result = self.resolve_verified_internal(name, record_type).await?;
                Ok(result.records)
            }
        }
    }

    pub async fn resolve_verified(
        &self,
        name: &str,
        record_type: Option<&str>,
    ) -> Result<ResolveVerifiedResult> {
        self.resolve_verified_internal(name, record_type).await
    }

    async fn resolve_verified_internal(
        &self,
        name: &str,
        record_type: Option<&str>,
    ) -> Result<ResolveVerifiedResult> {
        let detected_zone = self
            .extract_zone_from_fqdn(name)
            .unwrap_or_else(|| self.zone.clone());

        let params = if name.starts_with("npub1") {
            TP::from_pubkey(normalize_pk(name))
        } else {
            TP::from_domain(name)
        };

        let sources = fetch_tripartite_records(
            &params,
            &detected_zone,
            &self.relays,
            &self.api_base,
            &self.doh_endpoint,
            &self.http,
        )
        .await;

        let comparison = compare_tripartite(&sources);
        let mut records = to_resolved_records(&sources, &comparison);

        if let Some(rt) = record_type {
            records.retain(|r| r.record_type == rt);
        }

        Ok(ResolveVerifiedResult {
            records,
            verified: comparison.is_match,
            sources,
            comparison,
        })
    }

    pub async fn reverse(&self, npub: &str) -> Result<ReverseResult> {
        let hex_pubkey = normalize_pk(npub);
        let records = nostr_query_pubkey(&hex_pubkey, &self.zone, &self.relays).await?;
        let mut names: Vec<String> = records
            .iter()
            .map(|r| r.fqdn.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        names.sort();
        Ok(ReverseResult { names, records })
    }

    pub async fn discover_zones(&self) -> Result<Vec<ZoneStatus>> {
        Ok(discover_zones(&self.relays, &self.doh_endpoint, &self.http).await)
    }

    pub async fn get_nostr_records(
        &self,
        pubkey: Option<&str>,
        domain: Option<&str>,
    ) -> Result<Vec<NostrDnsRecord>> {
        if let Some(pubkey) = pubkey {
            let hex = if pubkey.starts_with("npub1") {
                normalize_pk(pubkey)
            } else {
                pubkey.to_string()
            };
            return nostr_query_pubkey(&hex, &self.zone, &self.relays).await;
        }
        if let Some(domain) = domain {
            return nostr_query_domain(domain, &self.zone, &self.relays).await;
        }
        Ok(Vec::new())
    }

    fn extract_zone_from_fqdn(&self, fqdn: &str) -> Option<String> {
        let lower = fqdn.to_lowercase();
        if lower.ends_with(&format!(".{}", self.zone)) {
            Some(self.zone.clone())
        } else {
            None
        }
    }
}

pub struct ResolverBuilder {
    mode: ResolveMode,
    relays: Vec<String>,
    api_base: String,
    zone: String,
    doh_endpoint: String,
}

impl ResolverBuilder {
    pub fn new() -> Self {
        Self {
            mode: ResolveMode::Dns,
            relays: DEFAULT_RELAYS.iter().map(|s| s.to_string()).collect(),
            api_base: DEFAULT_API_BASE.to_string(),
            zone: DEFAULT_ZONE.to_string(),
            doh_endpoint: DEFAULT_DOH_ENDPOINT.to_string(),
        }
    }

    pub fn mode(mut self, mode: ResolveMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn relays(mut self, relays: Vec<String>) -> Self {
        self.relays = relays;
        self
    }

    pub fn api_base(mut self, api_base: impl Into<String>) -> Self {
        self.api_base = api_base.into();
        self
    }

    pub fn zone(mut self, zone: impl Into<String>) -> Self {
        self.zone = zone.into();
        self
    }

    pub fn doh_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.doh_endpoint = endpoint.into();
        self
    }

    pub fn build(self) -> Resolver {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Resolver {
            mode: self.mode,
            relays: self.relays,
            api_base: self.api_base,
            zone: self.zone,
            doh_endpoint: self.doh_endpoint,
            http,
        }
    }
}

impl Default for ResolverBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for Resolver {
    fn default() -> Self {
        Self::builder().build()
    }
}
