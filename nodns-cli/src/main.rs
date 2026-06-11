//! NoDNS CLI — manage DNS records published via Nostr events.

mod commands;
mod config;
mod event;

use clap::Parser;

#[derive(Parser)]
#[command(name = "nodns", version, about = "Manage DNS records published via Nostr events")]
struct Cli {
    #[command(subcommand)]
    command: commands::Command,

    /// Path to config file
    #[arg(long, global = true, env = "NODNS_CONFIG")]
    config: Option<String>,

    /// Secret key (nsec, hex, or ncryptsec)
    #[arg(long, global = true, env = "NODNS_SECRET_KEY")]
    sec: Option<String>,

    /// Default zone
    #[arg(long, global = true, default_value = "nodns.shop")]
    zone: String,

    /// Relays to publish to
    #[arg(long, global = true, default_values_t = vec![
        "wss://relay.damus.io".to_string(),
        "wss://nos.lol".to_string(),
    ])]
    relay: Vec<String>,

    /// Show relay connection logs and debug output
    #[arg(long, global = true)]
    verbose: bool,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if cli.verbose {
        tracing_subscriber::fmt().init();
    } else {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::WARN)
            .init();
    }

    let cli = Cli::parse();

    let cfg = config::load(cli.config.as_deref(), cli.sec.as_deref(), &cli.zone, &cli.relay);

    if let Err(e) = commands::run(cli.command, &cfg).await {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
