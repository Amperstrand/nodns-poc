//! DNS-over-HTTPS (DoH) lookup functions.

use std::time::Duration;

use anyhow::Result;
use url::Url;

use crate::types::{dns_type_number_to_string, DnsAnswer, DohResponse, DEFAULT_DNS_TYPES};

const DOH_TIMEOUT_SECS: u64 = 10;

pub async fn query_doh(
    fqdn: &str,
    record_type: &str,
    doh_endpoint: &str,
    http: &reqwest::Client,
) -> Result<DohResponse> {
    let mut url = Url::parse(doh_endpoint)?;
    url.query_pairs_mut()
        .append_pair("name", fqdn)
        .append_pair("type", record_type);

    let resp = http
        .get(url)
        .header("Accept", "application/dns-json")
        .timeout(Duration::from_secs(DOH_TIMEOUT_SECS))
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("DNS query failed (HTTP {}).", resp.status());
    }

    let doh: DohResponse = resp.json().await?;
    Ok(doh)
}

fn strip_txt_quotes(data: &str) -> String {
    data.trim_start_matches('"')
        .trim_end_matches('"')
        .replace("\" \"", "")
        .replace("\"", "")
}

pub async fn query_all_dns_record_types(
    fqdn: &str,
    types: Option<&[&str]>,
    doh_endpoint: &str,
    http: &reqwest::Client,
) -> Vec<DnsAnswer> {
    let types = types.unwrap_or(DEFAULT_DNS_TYPES);
    let mut results = Vec::new();

    for &rt in types {
        if let Ok(resp) = query_doh(fqdn, rt, doh_endpoint, http).await {
            if let Some(answers) = resp.answer {
                for a in answers {
                    let type_str = dns_type_number_to_string(a.type_num);
                    if type_str != rt {
                        continue;
                    }
                    let data = if rt == "TXT" {
                        strip_txt_quotes(&a.data)
                    } else {
                        a.data.clone()
                    };
                    results.push(DnsAnswer {
                        name: a.name,
                        record_type: type_str,
                        ttl: a.ttl,
                        data,
                    });
                }
            }
        }
    }

    results
}

pub async fn query_all_dns_record_types_default(
    fqdn: &str,
    doh_endpoint: &str,
    http: &reqwest::Client,
) -> Vec<DnsAnswer> {
    query_all_dns_record_types(fqdn, None, doh_endpoint, http).await
}
