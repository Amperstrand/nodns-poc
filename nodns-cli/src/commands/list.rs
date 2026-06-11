use clap::Args;
use nostr_sdk::nips::nip19::FromBech32;
use nostr_sdk::PublicKey;

use crate::config::Config;
use crate::event;

#[derive(Args)]
pub struct CmdArgs {
    /// List records for this npub (no --sec needed)
    #[arg(long)]
    npub: Option<String>,

    /// Zone to query
    #[arg(short, long)]
    zone: Option<String>,
}

pub async fn run(args: CmdArgs, cfg: &Config) -> Result<(), String> {
    let pubkey = match args.npub {
        Some(npub) => {
            if npub.starts_with("npub1") {
                let pk = PublicKey::from_bech32(&npub).map_err(|e| format!("invalid npub: {e}"))?;
                Some(pk)
            } else {
                let pk = PublicKey::from_hex(&npub).map_err(|e| format!("invalid pubkey: {e}"))?;
                Some(pk)
            }
        }
        None => None,
    };

    let events = event::fetch_events(cfg, pubkey).await?;

    if events.is_empty() {
        eprintln!("No records found.");
        return Ok(());
    }

    println!("{:<20} {:<6} {:<8} DATA", "NAME", "TYPE", "TTL");
    println!("{}", "-".repeat(60));

    for ev in &events {
        for tag in ev.tags.iter() {
            let vec = tag.clone().to_vec();
            if vec.is_empty() {
                continue;
            }
            if vec[0] == "record" && vec.len() >= 5 {
                let name = if vec[2].is_empty() { "@" } else { &vec[2] };
                let ttl = vec.get(3).map(|s| s.as_str()).unwrap_or("3600");
                let data = vec.get(4).map(|s| s.as_str()).unwrap_or("");
                println!("{:<20} {:<6} {:<8} {}", name, vec[1], ttl, data);
            }
        }
    }

    Ok(())
}
