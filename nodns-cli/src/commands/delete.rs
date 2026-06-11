use clap::Args;

use crate::config::Config;
use crate::event;

#[derive(Args)]
pub struct CmdArgs {
    /// Record type to delete (A, AAAA, CNAME, TXT, MX, SRV)
    #[arg(short, long)]
    r#type: String,

    /// Subdomain name (empty for root)
    #[arg(short, long, default_value = "")]
    name: String,

    /// Zone
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

    let tags = event::build_delete_tags(&[(args.r#type.to_uppercase(), args.name)]);

    event::publish_event(&cfg, tags, args.dry_run).await?;
    Ok(())
}
