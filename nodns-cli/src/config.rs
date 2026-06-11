use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct RawConfig {
    nostr: Option<NostrSection>,
    resolve: Option<ResolveSection>,
}

#[derive(Debug, Deserialize)]
struct NostrSection {
    relays: Option<Vec<String>>,
    secret_key: Option<String>,
    zone: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ResolveSection {
    dns_server: Option<String>,
}

#[derive(Clone)]
pub struct Config {
    pub relays: Vec<String>,
    pub secret_key: Option<String>,
    pub zone: String,
    pub dns_server: String,
}

pub fn load(
    path: Option<&str>,
    cli_sec: Option<&str>,
    cli_zone: &str,
    cli_relays: &[String],
) -> Config {
    let config_path = path.map(PathBuf::from).unwrap_or_else(default_config_path);

    let raw = std::fs::read_to_string(&config_path)
        .ok()
        .and_then(|s| toml::from_str::<RawConfig>(&s).ok());

    let (file_relays, file_sec, file_zone, file_dns) = match raw {
        Some(r) => {
            let n = r.nostr.unwrap_or(NostrSection {
                relays: None,
                secret_key: None,
                zone: None,
            });
            let dns = r.resolve.and_then(|r| r.dns_server);
            (n.relays, n.secret_key, n.zone, dns)
        }
        None => (None, None, None, None),
    };

    let is_default_relays = cli_relays.len() == 2
        && cli_relays.contains(&"wss://relay.damus.io".to_string())
        && cli_relays.contains(&"wss://nos.lol".to_string());

    let relays = if !cli_relays.is_empty() && !is_default_relays {
        cli_relays.to_vec()
    } else {
        file_relays.unwrap_or_else(|| cli_relays.to_vec())
    };

    Config {
        relays,
        secret_key: cli_sec
            .map(String::from)
            .or(file_sec),
        zone: if cli_zone != "nodns.shop" {
            cli_zone.to_string()
        } else {
            file_zone.unwrap_or_else(|| cli_zone.to_string())
        },
        dns_server: file_dns.unwrap_or_else(|| "1.1.1.1:53".to_string()),
    }
}

fn default_config_path() -> PathBuf {
    dirs_home().join(".config/nodns/config.toml")
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}
