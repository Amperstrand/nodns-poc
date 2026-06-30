//! Tripartite verification — fetches records from API, Nostr, and DNS sources and compares them.

use std::collections::HashSet;

use crate::dns::query_doh;
use crate::nostr::{query_records_by_domain, query_records_by_pubkey};
use crate::types::{
    dns_type_number_to_string, ApiRecordsResponse, DnsAnswer, DnsRecord, NostrDnsRecord,
    ResolvedRecord, SourceResult, SourceStatus, TripartiteComparison, TripartiteRecords,
    DEFAULT_DNS_TYPES, WILDCARD_REDIRECT_IPS,
};

pub struct TripartiteParams {
    pub pubkey: Option<String>,
    pub domain: Option<String>,
}

impl TripartiteParams {
    pub fn from_pubkey(pubkey: impl Into<String>) -> Self {
        Self {
            pubkey: Some(pubkey.into()),
            domain: None,
        }
    }

    pub fn from_domain(domain: impl Into<String>) -> Self {
        Self {
            pubkey: None,
            domain: Some(domain.into()),
        }
    }
}

pub async fn fetch_api_records(
    params: &TripartiteParams,
    api_base: &str,
    http: &reqwest::Client,
) -> SourceResult<DnsRecord> {
    let qs = if let Some(ref pubkey) = params.pubkey {
        format!("pubkey={}", urlencoding_encode(pubkey))
    } else if let Some(ref domain) = params.domain {
        format!("domain={}", urlencoding_encode(domain))
    } else {
        return SourceResult::new("api", SourceStatus::Unavailable, Vec::new());
    };

    let url = format!("{}/api/records?{}", api_base, qs);

    match http
        .get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(resp) => {
            if !resp.status().is_success() {
                return SourceResult::with_error("api", format!("HTTP {}", resp.status()));
            }
            match resp.json::<ApiRecordsResponse>().await {
                Ok(data) => {
                    SourceResult::new("api", SourceStatus::Ok, data.records.unwrap_or_default())
                }
                Err(_) => SourceResult::with_error("api", "Failed to parse API response"),
            }
        }
        Err(_) => SourceResult::new("api", SourceStatus::Unavailable, Vec::new()),
    }
}

pub async fn fetch_nostr_records(
    params: &TripartiteParams,
    zone: &str,
    relays: &[String],
) -> SourceResult<NostrDnsRecord> {
    let result = if let Some(ref pubkey) = params.pubkey {
        query_records_by_pubkey(pubkey, zone, relays).await
    } else if let Some(ref domain) = params.domain {
        query_records_by_domain(domain, zone, relays).await
    } else {
        return SourceResult::new("nostr", SourceStatus::Ok, Vec::new());
    };

    match result {
        Ok(records) => SourceResult::new("nostr", SourceStatus::Ok, records),
        Err(e) => SourceResult::with_error("nostr", format!("Relay query failed: {e}")),
    }
}

pub async fn fetch_dns_records(
    fqdn: &str,
    types: Option<&[&str]>,
    doh_endpoint: &str,
    http: &reqwest::Client,
) -> SourceResult<DnsAnswer> {
    let types = types.unwrap_or(DEFAULT_DNS_TYPES);
    let mut all_answers = Vec::new();

    for &rt in types {
        if let Ok(resp) = query_doh(fqdn, rt, doh_endpoint, http).await {
            if let Some(answers) = resp.answer {
                for a in answers {
                    let type_str = dns_type_number_to_string(a.type_num);
                    let is_wildcard_a = (a.type_num == 1 || type_str == "A")
                        && WILDCARD_REDIRECT_IPS.contains(&a.data.trim_matches('"'));
                    if is_wildcard_a {
                        continue;
                    }
                    all_answers.push(DnsAnswer {
                        name: a.name,
                        record_type: type_str,
                        ttl: a.ttl,
                        data: a.data,
                    });
                }
            }
        }
    }

    let status = if all_answers.is_empty() {
        SourceStatus::Unavailable
    } else {
        SourceStatus::Ok
    };

    SourceResult::new("dns", status, all_answers)
}

pub async fn fetch_tripartite_records(
    params: &TripartiteParams,
    zone: &str,
    relays: &[String],
    api_base: &str,
    doh_endpoint: &str,
    http: &reqwest::Client,
) -> TripartiteRecords {
    let fqdn = params.domain.clone().unwrap_or_default();

    let (api_result, nostr_result, dns_result) = tokio::join!(
        fetch_api_records(params, api_base, http),
        fetch_nostr_records(params, zone, relays),
        async {
            if fqdn.is_empty() {
                SourceResult::new("dns", SourceStatus::Unavailable, Vec::new())
            } else {
                fetch_dns_records(&fqdn, None, doh_endpoint, http).await
            }
        }
    );

    TripartiteRecords {
        api: api_result,
        nostr: nostr_result,
        dns: dns_result,
    }
}

pub fn compare_tripartite(records: &TripartiteRecords) -> TripartiteComparison {
    let to_key = |t: &str, d: &str| format!("{t}:{d}");

    let api_keys: HashSet<String> = records
        .api
        .records
        .iter()
        .map(|r| to_key(&r.record_type, &r.rdata))
        .collect();
    let nostr_keys: HashSet<String> = records
        .nostr
        .records
        .iter()
        .map(|r| to_key(&r.record_type, &r.value))
        .collect();
    let dns_keys: HashSet<String> = records
        .dns
        .records
        .iter()
        .map(|r| to_key(&r.record_type, &r.data))
        .collect();

    let all_keys: HashSet<String> = api_keys
        .iter()
        .chain(nostr_keys.iter())
        .chain(dns_keys.iter())
        .cloned()
        .collect();

    let mut only_in_api = Vec::new();
    let mut only_in_nostr = Vec::new();
    let mut only_in_dns = Vec::new();

    for key in &all_keys {
        let in_api = api_keys.contains(key);
        let in_nostr = nostr_keys.contains(key);
        let in_dns = dns_keys.contains(key);

        if in_api && !in_nostr && !in_dns {
            only_in_api.push(key.clone());
        }
        if in_nostr && !in_api && !in_dns {
            only_in_nostr.push(key.clone());
        }
        if in_dns && !in_api && !in_nostr {
            only_in_dns.push(key.clone());
        }
    }

    let is_match = only_in_api.is_empty() && only_in_nostr.is_empty() && only_in_dns.is_empty();

    TripartiteComparison {
        is_match,
        api_count: records.api.records.len(),
        nostr_count: records.nostr.records.len(),
        dns_count: records.dns.records.len(),
        only_in_api,
        only_in_nostr,
        only_in_dns,
    }
}

pub fn to_resolved_records(
    sources: &TripartiteRecords,
    comparison: &TripartiteComparison,
) -> Vec<ResolvedRecord> {
    if comparison.is_match {
        let from_dns: Vec<ResolvedRecord> = sources
            .dns
            .records
            .iter()
            .map(|r| ResolvedRecord {
                record_type: r.record_type.clone(),
                name: r.name.clone(),
                ttl: r.ttl,
                data: r.data.clone(),
                source: Some("dns".to_string()),
                pubkey: None,
                event_id: None,
            })
            .collect();

        if !from_dns.is_empty() {
            return from_dns;
        }

        return sources
            .api
            .records
            .iter()
            .map(|r| ResolvedRecord {
                record_type: r.record_type.clone(),
                name: r.fqdn.clone(),
                ttl: r.ttl,
                data: r.rdata.clone(),
                source: Some("dns".to_string()),
                pubkey: None,
                event_id: None,
            })
            .collect();
    }

    let agreed_keys: HashSet<String> = sources
        .api
        .records
        .iter()
        .filter_map(|r| {
            let key = format!("{}:{}", r.record_type, r.rdata);
            let in_nostr = sources
                .nostr
                .records
                .iter()
                .any(|n| format!("{}:{}", n.record_type, n.value) == key);
            let in_dns = sources
                .dns
                .records
                .iter()
                .any(|d| format!("{}:{}", d.record_type, d.data) == key);
            if in_nostr || in_dns {
                Some(key)
            } else {
                None
            }
        })
        .collect();

    sources
        .api
        .records
        .iter()
        .filter(|r| agreed_keys.contains(&format!("{}:{}", r.record_type, r.rdata)))
        .map(|r| ResolvedRecord {
            record_type: r.record_type.clone(),
            name: r.fqdn.clone(),
            ttl: r.ttl,
            data: r.rdata.clone(),
            source: Some("dns".to_string()),
            pubkey: None,
            event_id: None,
        })
        .collect()
}

fn urlencoding_encode(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            ' ' => "+".to_string(),
            c if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~' => {
                c.to_string()
            }
            c => format!("%{:02X}", c as u8),
        })
        .collect()
}
