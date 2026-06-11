use clap::Args;

use crate::config::Config;
use crate::event;

#[derive(Args)]
pub struct CmdArgs {
    /// Record type (A, AAAA, CNAME, TXT, MX, SRV)
    #[arg(short, long)]
    r#type: String,

    /// Record data (IP address, hostname, text content)
    #[arg(short, long)]
    data: String,

    /// Subdomain name (empty for root)
    #[arg(short, long, default_value = "")]
    name: String,

    /// TTL in seconds
    #[arg(short, long, default_value_t = 3600)]
    ttl: u32,

    /// Zone to publish in
    #[arg(short, long)]
    zone: Option<String>,

    /// Show event without publishing
    #[arg(long)]
    dry_run: bool,
}

pub async fn run(args: CmdArgs, cfg: &Config) -> Result<(), String> {
    let mut cfg = cfg.clone();
    if let Some(zone) = args.zone {
        cfg.zone = zone;
    }

    let tags = event::build_record_tags(&[(
        args.r#type.to_uppercase(),
        args.name,
        args.data,
        args.ttl,
    )]);

    event::publish_event(&cfg, tags, args.dry_run).await?;
    Ok(())
}
