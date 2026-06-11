use clap::Args;
use nostr_sdk::nips::nip19::FromBech32;
use nostr_sdk::PublicKey;

use crate::config::Config;
use crate::event;

const KIND_DNS: u16 = 11111;

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

    /// Only query Nostr relays, skip DNS
    #[arg(long)]
    nostr_only: bool,

    /// Only query DNS, skip Nostr relays
    #[arg(long)]
    dns_only: bool,
}

pub async fn run(args: CmdArgs, cfg: &Config) -> Result<(), String> {
    let filter_type = args.r#type.to_uppercase();

    let do_nostr = !args.dns_only;
    let do_dns = !args.nostr_only;

    if do_nostr {
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
        } else if do_nostr && !do_dns {
            eprintln!("Cannot extract npub from domain. Use --npub or provide <npub>.nodns.shop domain.");
        }
    }

    if do_dns {
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
        // TODO: custom DNS server resolver
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
