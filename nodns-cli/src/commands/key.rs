use clap::Subcommand;

use nostr_sdk::nips::nip19::{FromBech32, ToBech32};
use nostr_sdk::{Keys, SecretKey};

#[derive(Subcommand)]
pub enum KeyCommand {
    /// Generate a new key pair
    Generate,

    /// Show the public key (npub) for the configured secret key
    Public,

    /// Derive npub from an nsec or hex private key
    Derive {
        /// Secret key (nsec or hex)
        key: String,
    },
}

pub fn run(cmd: KeyCommand) -> Result<(), String> {
    match cmd {
        KeyCommand::Generate => {
            let keys = Keys::generate();
            let secret = keys.secret_key();
            let nsec = secret.to_bech32().map_err(|e| format!("nsec: {e}"))?;
            let npub = keys.public_key().to_bech32().map_err(|e| format!("npub: {e}"))?;
            println!("nsec: {nsec}");
            println!("npub: {npub}");
            eprintln!("\nSave the nsec to your config or set NODNS_SECRET_KEY=nsec1...");
        }
        KeyCommand::Public => {
            eprintln!("error: no secret key configured. Use --sec or NODNS_SECRET_KEY");
            std::process::exit(1);
        }
        KeyCommand::Derive { key } => {
            let keys = if key.starts_with("nsec1") {
                let sk = SecretKey::from_bech32(&key).map_err(|e| format!("invalid nsec: {e}"))?;
                Keys::new(sk)
            } else {
                let sk = SecretKey::from_hex(&key).map_err(|e| format!("invalid hex: {e}"))?;
                Keys::new(sk)
            };
            let npub = keys.public_key().to_bech32().map_err(|e| format!("npub: {e}"))?;
            println!("{npub}");
        }
    }
    Ok(())
}
