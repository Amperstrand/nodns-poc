use clap::Args;
use nostr_sdk::nips::nip19::FromBech32;
use nostr_sdk::PublicKey;
use serde::Deserialize;

use crate::config::Config;
use crate::event;

const KIND_DNS: u16 = 11111;
const DEFAULT_API_BASE: &str = "https://nodns.shop";

#[derive(Args)]
pub struct CmdArgs {
    /// Domain name to resolve (e.g. npub1...nodns.shop)
    domain: String,

    /// Record type (A, AAAA, CNAME, TXT, MX, SRV, ANY)
    #[arg(short = 't', long, default_value = "ANY")]
    r#type: String,

    /// DNS server to query (host:port)
    #[arg(short, long)]
    server: Option<String>,

    /// Bot API base URL (default: https://nodns.shop)
    #[arg(long)]
    api_base: Option<String>,

    /// Query Nostr relays directly for raw events
    #[arg(long)]
    nostr: bool,

    /// Only query DNS, skip API and Nostr
    #[arg(long)]
    dns_only: bool,
}

#[derive(Deserialize)]
struct ApiRecordsResponse {
    records: Vec<ApiRecord>,
    count: usize,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct ApiRecord {
    npub: String,
    name: String,
    fqdn: String,
    #[serde(rename = "type")]
    record_type: String,
    ttl: u32,
    rdata: String,
    created_at: u64,
}

pub async fn run(args: CmdArgs, cfg: &Config) -> Result<(), String> {
    let filter_type = args.r#type.to_uppercase();

    if args.dns_only {
        println!(";; DNS ANSWER:");
        match resolve_dns(&args.domain, &filter_type, args.server.as_deref()).await {
            Ok(records) => {
                if records.is_empty() {
                    println!("  (no DNS records found)");
                }
                for record in records {
                    println!("  {record}");
                }
            }
            Err(e) => eprintln!("  (dns query failed: {e})"),
        }
        return Ok(());
    }

    if args.nostr {
        if let Some(npub_str) = extract_npub_from_domain(&args.domain) {
            println!(";; NOSTR EVENTS (kind {KIND_DNS}):");
            match resolve_nostr(cfg, &npub_str, &filter_type).await {
                Ok(events) => {
                    if events.is_empty() {
                        println!("  (no events found for {npub_str})");
                    }
                    for ev in &events {
                        for tag in ev.tags.iter() {
                            let vec = tag.clone().to_vec();
                            if vec.len() >= 5 && vec[0] == "record"
                                && (filter_type == "ANY" || vec[1].eq_ignore_ascii_case(&filter_type)) {
                                    let name = if vec[2].is_empty() { "@" } else { &vec[2] };
                                    println!("  {name}\t{}\t{}\t{}", vec[1], vec[3], vec[4]);
                                }
                        }
                    }
                }
                Err(e) => eprintln!("  (nostr query failed: {e})"),
            }
            println!();
        } else {
            eprintln!("Cannot extract npub from domain. Provide <npub>.nodns.shop domain.");
        }
        return Ok(());
    }

    let api_base = args.api_base.as_deref().unwrap_or(DEFAULT_API_BASE);
    let npub_str = extract_npub_from_domain(&args.domain);

    let api_result = if let Some(ref npub) = npub_str {
        resolve_api_by_npub(api_base, npub, &filter_type).await
    } else {
        resolve_api(api_base, &args.domain, &filter_type).await
    };

    println!(";; API RECORDS ({api_base}):");
    match api_result {
        Ok(resp) => {
            if resp.records.is_empty() {
                println!("  (no records found for {})", args.domain);
            }
            for r in &resp.records {
                if filter_type != "ANY" && !r.record_type.eq_ignore_ascii_case(&filter_type) {
                    continue;
                }
                let name = if r.name.is_empty() { "@" } else { &r.name };
                println!("  {name}\t{}\t{}\t{}\t{}", r.record_type, r.ttl, r.rdata, r.fqdn);
            }
            println!("  ;; {} record(s)", resp.count);
        }
        Err(e) => {
            eprintln!("  (api query failed: {e})");
            eprintln!("  ;; falling back to DNS...");
            match resolve_dns(&args.domain, &filter_type, args.server.as_deref()).await {
                Ok(records) => {
                    if records.is_empty() {
                        println!("  (no DNS records found)");
                    }
                    for record in records {
                        println!("  {record}");
                    }
                }
                Err(e2) => eprintln!("  (dns query also failed: {e2})"),
            }
        }
    }

    Ok(())
}

fn extract_npub_from_domain(domain: &str) -> Option<String> {
    let parts: Vec<&str> = domain.split('.').collect();
    if parts.len() >= 3 && parts[0].starts_with("npub1") {
        Some(parts[0].to_string())
    } else {
        None
    }
}

async fn resolve_api(
    api_base: &str,
    domain: &str,
    _filter_type: &str,
) -> Result<ApiRecordsResponse, String> {
    let url = format!("{api_base}/api/records?domain={domain}");
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("{e}"))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    resp.json::<ApiRecordsResponse>()
        .await
        .map_err(|e| format!("parse error: {e}"))
}

async fn resolve_api_by_npub(
    api_base: &str,
    npub: &str,
    _filter_type: &str,
) -> Result<ApiRecordsResponse, String> {
    let url = format!("{api_base}/api/records/by-npub/{npub}");
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("{e}"))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    resp.json::<ApiRecordsResponse>()
        .await
        .map_err(|e| format!("parse error: {e}"))
}

async fn resolve_nostr(
    cfg: &Config,
    npub: &str,
    filter_type: &str,
) -> Result<Vec<nostr_sdk::Event>, String> {
    let pk = if npub.starts_with("npub1") {
        PublicKey::from_bech32(npub).map_err(|e| format!("invalid npub: {e}"))?
    } else {
        PublicKey::from_hex(npub).map_err(|e| format!("invalid pubkey: {e}"))?
    };

    let _ = filter_type;
    event::fetch_events(cfg, Some(pk)).await
}

async fn resolve_dns(
    domain: &str,
    record_type_str: &str,
    server: Option<&str>,
) -> Result<Vec<String>, String> {
    use hickory_proto::rr::RecordType;
    use std::str::FromStr;

    let record_type = match record_type_str {
        "A" => RecordType::A,
        "AAAA" => RecordType::AAAA,
        "CNAME" => RecordType::CNAME,
        "TXT" => RecordType::TXT,
        "MX" => RecordType::MX,
        "SRV" => RecordType::SRV,
        "ANY" => RecordType::ANY,
        other => return Err(format!("unsupported record type: {other}")),
    };

    let name = hickory_proto::rr::Name::from_str(domain)
        .map_err(|e| format!("invalid domain: {e}"))?;

    let resolver = if let Some(_srv) = server {
        hickory_resolver::TokioResolver::builder_tokio()
            .map_err(|e| format!("resolver init failed: {e}"))?
            .build()
    } else {
        hickory_resolver::TokioResolver::builder_tokio()
            .map_err(|e| format!("resolver init failed: {e}"))?
            .build()
    };

    let response = resolver
        .lookup(name, record_type)
        .await
        .map_err(|e| format!("query failed: {e}"))?;

    Ok(response.records().iter().map(|r| format!("{r}")).collect())
}
