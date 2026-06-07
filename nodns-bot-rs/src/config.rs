//! TOML configuration loading with multi-zone support and backward compatibility.
//!
//! Ported 1:1 from `nodns-bot/internal/config/config.go`.

use std::fmt;
use std::path::Path;
use std::time::Duration;

use serde::Deserialize;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("reading config file: {0}")]
    Read(#[from] std::io::Error),

    #[error("parsing config: {0}")]
    Parse(#[from] toml::de::Error),

    #[error("invalid config: {0}")]
    Validation(String),
}

// ---------------------------------------------------------------------------
// Top-level config
// ---------------------------------------------------------------------------

/// Root configuration file structure.
///
/// Maps directly from TOML:
/// ```toml
/// [server]
/// [nostr]
/// [dns]
/// [[dns.zones]]
/// [policy]
/// [store]
/// [payment]
/// [registrar_keys]
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,

    #[serde(default)]
    pub nostr: NostrConfig,

    #[serde(default)]
    pub dns: DNSConfig,

    #[serde(default)]
    pub policy: PolicyConfig,

    #[serde(default)]
    pub store: StoreConfig,

    #[serde(default)]
    pub registrar_keys: std::collections::HashMap<String, String>,

    #[serde(default)]
    pub payment: PaymentConfig,

    #[serde(default)]
    pub dnssec_derivation: DnssecDerivationConfig,

    #[serde(default)]
    pub acme: AcmeConfig,
}

// ---------------------------------------------------------------------------
// Sub-config structs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub bind: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:9090".to_string(),
        }
    }
}

/// Duration in seconds deserialized from TOML (Go uses `time.Duration` nanos,
/// but the TOML representation is a human-readable string like `"1s"` or a
/// bare integer of seconds). We accept both.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(try_from = "toml::Value")]
pub struct HumaneDuration(pub Duration);

impl TryFrom<toml::Value> for HumaneDuration {
    type Error = String;

    fn try_from(value: toml::Value) -> Result<Self, Self::Error> {
        match value {
            toml::Value::Integer(secs) => Ok(HumaneDuration(Duration::from_secs(secs as u64))),
            toml::Value::String(s) => parse_duration::parse_human(&s)
                .map(HumaneDuration)
                .map_err(|e| format!("invalid duration '{s}': {e}")),
            other => Err(format!("expected integer or string for duration, got {other:?}")),
        }
    }
}

/// Minimal human-readable duration parser (supports `1s`, `5m`, `1h`, plain integer).
mod parse_duration {
    use std::time::Duration;

    pub fn parse_human(s: &str) -> Result<Duration, String> {
        let s = s.trim();
        // Try pure integer seconds
        if let Ok(secs) = s.parse::<u64>() {
            return Ok(Duration::from_secs(secs));
        }
        let bytes = s.as_bytes();
        if bytes.is_empty() {
            return Err("empty duration".into());
        }
        let (num_str, suffix) = match &bytes[bytes.len() - 1] {
            b's' => (&s[..s.len() - 1], 1u64),
            b'm' => (&s[..s.len() - 1], 60),
            b'h' => (&s[..s.len() - 1], 3600),
            _ => return Err(format!("unknown duration suffix in '{s}'")),
        };
        let num: u64 = num_str.trim().parse().map_err(|_| format!("invalid number in '{s}'"))?;
        Ok(Duration::from_secs(num * suffix))
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct NostrConfig {
    #[serde(default)]
    pub relays: Vec<String>,

    /// Single-zone shorthand (used for backward compat).
    #[serde(default)]
    pub zone: String,

    pub reconnect_min: Option<HumaneDuration>,
    pub reconnect_max: Option<HumaneDuration>,
}

impl Default for NostrConfig {
    fn default() -> Self {
        Self {
            relays: Vec::new(),
            zone: String::new(),
            reconnect_min: Some(HumaneDuration(Duration::from_secs(1))),
            reconnect_max: Some(HumaneDuration(Duration::from_secs(60))),
        }
    }
}

/// DNS connection details for a single zone.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ZoneConfig {
    pub knot_address: String,
    pub zone: String,
    pub tsig_key_name: String,
    pub tsig_key_secret: String,
    pub tsig_algorithm: String,
    pub default_ttl: u32,
    pub negative_ttl: u32,
}

impl Default for ZoneConfig {
    fn default() -> Self {
        Self {
            knot_address: String::new(),
            zone: String::new(),
            tsig_key_name: String::new(),
            tsig_key_secret: String::new(),
            tsig_algorithm: "hmac-sha256".to_string(),
            default_ttl: 3600,
            negative_ttl: 60,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DNSConfig {
    #[serde(default)]
    pub zones: Vec<ZoneConfig>,

    // Old flat fields kept for backward compatibility with single-zone configs.
    #[serde(default)]
    pub knot_address: String,
    #[serde(default)]
    pub zone: String,
    #[serde(default)]
    pub tsig_key_name: String,
    #[serde(default)]
    pub tsig_key_secret: String,
    #[serde(default)]
    pub tsig_algorithm: String,
    #[serde(default)]
    pub default_ttl: u32,
    #[serde(default)]
    pub negative_ttl: u32,
}

impl Default for DNSConfig {
    fn default() -> Self {
        Self {
            zones: Vec::new(),
            knot_address: String::new(),
            zone: String::new(),
            tsig_key_name: String::new(),
            tsig_key_secret: String::new(),
            tsig_algorithm: "hmac-sha256".to_string(),
            default_ttl: 3600,
            negative_ttl: 60,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PolicyConfig {
    pub max_records: usize,
    pub rate_limit: usize,
    pub allowed_types: Vec<String>,
    pub block_private_ip: bool,
    pub max_txt_length: usize,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            max_records: 20,
            rate_limit: 5,
            allowed_types: vec![
                "A".into(),
                "AAAA".into(),
                "CNAME".into(),
                "TXT".into(),
                "MX".into(),
            ],
            block_private_ip: false,
            max_txt_length: 512,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct StoreConfig {
    pub path: String,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            path: "records.db".to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PaymentConfig {
    pub enabled: bool,
    pub required_sats: i64,
    pub update_free: bool,
    pub cashu_mint_url: String,
}

impl Default for PaymentConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            required_sats: 250,
            update_free: true,
            cashu_mint_url: String::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct DnssecDerivationConfig {
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AcmeConfig {
    pub enabled: bool,
    pub directory_url: String,
    pub contact_email: String,
    pub challenge_ttl: u32,
}

impl Default for AcmeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            directory_url: "https://acme-staging-v02.api.letsencrypt.org/directory".to_string(),
            contact_email: String::new(),
            challenge_ttl: 300,
        }
    }
}

// ---------------------------------------------------------------------------
// Loading + validation
// ---------------------------------------------------------------------------

impl Config {
    /// Load and validate a config file from disk.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let data = std::fs::read_to_string(path)?;
        let cfg: Config = toml::from_str(&data)?;
        let mut cfg = cfg;
        cfg.apply_defaults();
        cfg.validate()?;
        Ok(cfg)
    }

    /// Fill in zero/empty values with sensible defaults, mirroring Go's
    /// `applyDefaults`.  This also handles the backward-compat path where
    /// old `[dns]` flat fields are promoted into `[[dns.zones]]`.
    pub fn apply_defaults(&mut self) {
        // Server
        if self.server.bind.is_empty() {
            self.server.bind = "127.0.0.1:9090".to_string();
        }

        // Nostr reconnect timing
        if self.nostr.reconnect_min.is_none() {
            self.nostr.reconnect_min = Some(HumaneDuration(Duration::from_secs(1)));
        }
        if self.nostr.reconnect_max.is_none() {
            self.nostr.reconnect_max = Some(HumaneDuration(Duration::from_secs(60)));
        }

        // DNS top-level defaults
        if self.dns.default_ttl == 0 {
            self.dns.default_ttl = 3600;
        }
        if self.dns.negative_ttl == 0 {
            self.dns.negative_ttl = 60;
        }
        if self.dns.tsig_algorithm.is_empty() {
            self.dns.tsig_algorithm = "hmac-sha256".to_string();
        }

        // Backward compat: synthesize a [[dns.zones]] entry from the flat [dns] fields.
        if self.dns.zones.is_empty() && !self.dns.zone.is_empty() {
            self.dns.zones.push(ZoneConfig {
                knot_address: self.dns.knot_address.clone(),
                zone: self.dns.zone.clone(),
                tsig_key_name: self.dns.tsig_key_name.clone(),
                tsig_key_secret: self.dns.tsig_key_secret.clone(),
                tsig_algorithm: self.dns.tsig_algorithm.clone(),
                default_ttl: self.dns.default_ttl,
                negative_ttl: self.dns.negative_ttl,
            });
        }

        // Inherit top-level defaults into each zone that left fields at zero.
        for z in &mut self.dns.zones {
            if z.default_ttl == 0 {
                z.default_ttl = self.dns.default_ttl;
            }
            if z.negative_ttl == 0 {
                z.negative_ttl = self.dns.negative_ttl;
            }
            if z.tsig_algorithm.is_empty() {
                z.tsig_algorithm = self.dns.tsig_algorithm.clone();
            }
        }

        // Policy
        if self.policy.max_records == 0 {
            self.policy.max_records = 20;
        }
        if self.policy.rate_limit == 0 {
            self.policy.rate_limit = 5;
        }
        if self.policy.allowed_types.is_empty() {
            self.policy.allowed_types = vec![
                "A".into(),
                "AAAA".into(),
                "CNAME".into(),
                "TXT".into(),
                "MX".into(),
            ];
        }
        if self.policy.max_txt_length == 0 {
            self.policy.max_txt_length = 512;
        }

        // Store
        if self.store.path.is_empty() {
            self.store.path = "records.db".to_string();
        }

        // Payment
        if !self.payment.enabled {
            if self.payment.required_sats == 0 {
                self.payment.required_sats = 250;
            }
            self.payment.update_free = true;
        }
    }

    /// Validate required fields after defaults are applied.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.nostr.relays.is_empty() {
            return Err(ConfigError::Validation(
                "nostr.relays must contain at least one relay URL".into(),
            ));
        }
        if self.nostr.zone.is_empty() {
            return Err(ConfigError::Validation("nostr.zone is required".into()));
        }
        if self.dns.zones.is_empty() {
            return Err(ConfigError::Validation(
                "at least one dns zone must be configured".into(),
            ));
        }
        for (i, z) in self.dns.zones.iter().enumerate() {
            if z.zone.is_empty() {
                return Err(ConfigError::Validation(format!(
                    "dns.zones[{i}].zone is required"
                )));
            }
            if z.knot_address.is_empty() {
                return Err(ConfigError::Validation(format!(
                    "dns.zones[{i}].knot_address is required"
                )));
            }
            if z.tsig_key_name.is_empty() {
                return Err(ConfigError::Validation(format!(
                    "dns.zones[{i}].tsig_key_name is required"
                )));
            }
            if z.tsig_key_secret.is_empty() {
                return Err(ConfigError::Validation(format!(
                    "dns.zones[{i}].tsig_key_secret is required"
                )));
            }
        }
        Ok(())
    }

    /// Helper: return the configured reconnect_min as a `Duration`.
    pub fn reconnect_min(&self) -> Duration {
        self.nostr
            .reconnect_min
            .map(|d| d.0)
            .unwrap_or(Duration::from_secs(1))
    }

    /// Helper: return the configured reconnect_max as a `Duration`.
    pub fn reconnect_max(&self) -> Duration {
        self.nostr
            .reconnect_max
            .map(|d| d.0)
            .unwrap_or(Duration::from_secs(60))
    }
}

impl fmt::Display for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Config {{ server.bind={}, nostr.relays={:?}, dns.zones={} }}",
            self.server.bind,
            self.nostr.relays,
            self.dns.zones.len()
        )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backward_compat_single_zone_with_custom_ttl() {
        let toml_str = r#"
[server]
bind = "0.0.0.0:8080"

[nostr]
relays = ["wss://relay.example.com"]
zone = "nodns.shop"

[dns]
knot_address = "127.0.0.1:5353"
zone = "nodns.shop"
tsig_key_name = "key1."
tsig_key_secret = "c2VjcmV0"
tsig_algorithm = "hmac-sha256"
default_ttl = 7200

[policy]
max_records = 10

[store]
path = "test.db"
"#;
        let mut cfg: Config = toml::from_str(toml_str).unwrap();
        cfg.apply_defaults();
        cfg.validate().unwrap();

        assert_eq!(cfg.server.bind, "0.0.0.0:8080");
        assert_eq!(cfg.dns.zones.len(), 1);
        assert_eq!(cfg.dns.zones[0].zone, "nodns.shop");
        assert_eq!(cfg.dns.zones[0].default_ttl, 7200);
        assert_eq!(cfg.policy.max_records, 10);
    }

    #[test]
    fn parse_and_validate_backward_compat() {
        let toml = r#"
[server]
bind = "0.0.0.0:8080"

[nostr]
relays = ["wss://relay.example.com"]
zone = "nodns.shop"

[dns]
knot_address = "127.0.0.1:5353"
zone = "nodns.shop"
tsig_key_name = "key1."
tsig_key_secret = "c2VjcmV0"

[store]
path = "test.db"
"#;
        let mut cfg: Config = toml::from_str(toml).unwrap();
        cfg.apply_defaults();
        cfg.validate().unwrap();

        assert_eq!(cfg.server.bind, "0.0.0.0:8080");
        assert_eq!(cfg.dns.zones.len(), 1);
        assert_eq!(cfg.dns.zones[0].zone, "nodns.shop");
        assert_eq!(cfg.dns.zones[0].knot_address, "127.0.0.1:5353");
        assert_eq!(cfg.dns.zones[0].default_ttl, 3600); // inherited top-level default
        assert_eq!(cfg.dns.zones[0].tsig_algorithm, "hmac-sha256");
    }

    #[test]
    fn multi_zone_config() {
        let toml = r#"
[nostr]
relays = ["wss://relay.example.com"]
zone = "nodns.shop"

[[dns.zones]]
knot_address = "127.0.0.1:5353"
zone = "nodns.shop"
tsig_key_name = "key1."
tsig_key_secret = "secret1"

[[dns.zones]]
knot_address = "127.0.0.1:5354"
zone = "cv"
tsig_key_name = "key2."
tsig_key_secret = "secret2"
default_ttl = 7200
"#;
        let mut cfg: Config = toml::from_str(toml).unwrap();
        cfg.apply_defaults();
        cfg.validate().unwrap();

        assert_eq!(cfg.dns.zones.len(), 2);
        assert_eq!(cfg.dns.zones[0].zone, "nodns.shop");
        assert_eq!(cfg.dns.zones[1].zone, "cv");
        assert_eq!(cfg.dns.zones[1].default_ttl, 7200);
        // First zone should inherit default TTL
        assert_eq!(cfg.dns.zones[0].default_ttl, 3600);
    }

    #[test]
    fn validation_fails_without_relays() {
        let toml = r#"
[nostr]
zone = "nodns.shop"
"#;
        let mut cfg: Config = toml::from_str(toml).unwrap();
        cfg.apply_defaults();
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("relay"));
    }
}
