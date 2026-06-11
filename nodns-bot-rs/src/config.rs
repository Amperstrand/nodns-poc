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
    pub registrar: RegistrarConfig,

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
#[allow(dead_code)]
pub struct HumaneDuration(pub Duration);

impl TryFrom<toml::Value> for HumaneDuration {
    type Error = String;

    fn try_from(value: toml::Value) -> Result<Self, Self::Error> {
        match value {
            toml::Value::Integer(secs) => {
                if secs <= 0 {
                    return Err(format!(
                        "duration must be a positive integer, got {secs}"
                    ));
                }
                Ok(HumaneDuration(Duration::from_secs(secs as u64)))
            }
            toml::Value::String(s) => humantime::parse_duration(&s)
                .map(HumaneDuration)
                .map_err(|e| format!("invalid duration '{s}': {e}")),
            other => Err(format!("expected integer or string for duration, got {other:?}")),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct NostrConfig {
    #[serde(default)]
    pub relays: Vec<String>,

    /// Single-zone shorthand (used for backward compat).
    #[serde(default)]
    pub zone: String,
}


/// Per-zone payment configuration.
///
/// Each zone can have its own pricing, mint URL, and mint filter.
/// If a zone has `enabled = false` (the default), the global `[payment]`
/// section is used as fallback via `apply_defaults()`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ZonePaymentConfig {
    pub enabled: bool,
    pub create_price: u64,
    pub update_price: u64,
    pub delete_price: u64,
    pub npub_names_free: bool,
    pub mint_url: String,
    pub mint_filter: String,
}

impl Default for ZonePaymentConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            create_price: 2,
            update_price: 0,
            delete_price: 0,
            npub_names_free: true,
            mint_url: "https://testnut.cashu.space".to_string(),
            mint_filter: "testnut".to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ZoneLeaseConfig {
    pub grace_period_days: u32,
    pub max_lease_days: u32,
    pub operator_lease_expires: Option<String>,
}

impl Default for ZoneLeaseConfig {
    fn default() -> Self {
        Self {
            grace_period_days: 30,
            max_lease_days: 365,
            operator_lease_expires: None,
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
    #[serde(default)]
    pub payment: ZonePaymentConfig,
    #[serde(default)]
    pub lease: ZoneLeaseConfig,
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
            payment: ZonePaymentConfig::default(),
            lease: ZoneLeaseConfig::default(),
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

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RegistrarConfig {
    pub nsec_hex: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AcmeConfig {
    pub enabled: bool,
    /// "staging" (default) or "production". Used to resolve directory_url if not explicitly set.
    pub environment: String,
    /// ACME directory URL. If empty, resolved from `environment` during apply_defaults().
    /// If explicitly set in config, overrides the environment-based URL.
    pub directory_url: String,
    /// Default contact email for ACME account. Can be empty (will use cert@nodns.shop).
    pub contact_email: String,
    pub challenge_ttl: u32,
    /// Default CA: "letsencrypt-staging" (default), "zerossl", "letsencrypt-production"
    pub ca: String,
    /// ZeroSSL EAB Key ID (required for ZeroSSL)
    pub zerossl_eab_kid: String,
    /// ZeroSSL EAB HMAC key (base64-encoded, required for ZeroSSL)
    pub zerossl_eab_hmac_key: String,
    /// Hex-encoded 32-byte key for encrypting ACME private keys at rest.
    /// If empty, a random key is generated at startup (keys unreadable after restart).
    pub encryption_key: Option<String>,
}

impl Default for AcmeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            environment: "staging".to_string(),
            directory_url: String::new(),
            contact_email: String::new(),
            challenge_ttl: 300,
            ca: "letsencrypt-staging".to_string(),
            zerossl_eab_kid: String::new(),
            zerossl_eab_hmac_key: String::new(),
            encryption_key: None,
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
                payment: ZonePaymentConfig::default(),
                lease: ZoneLeaseConfig::default(),
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

        // Propagate global [payment] to zones that don't have zone-level payment enabled.
        if self.payment.enabled {
            for z in &mut self.dns.zones {
                if !z.payment.enabled {
                    z.payment.enabled = true;
                    z.payment.create_price = if self.payment.required_sats > 0 {
                        self.payment.required_sats as u64
                    } else {
                        250
                    };
                    z.payment.update_price = if self.payment.update_free { 0 } else { self.payment.required_sats as u64 };
                    z.payment.delete_price = 0;
                    z.payment.npub_names_free = true;
                    z.payment.mint_url = if self.payment.cashu_mint_url.is_empty() {
                        "https://testnut.cashu.space".to_string()
                    } else {
                        self.payment.cashu_mint_url.clone()
                    };
                    z.payment.mint_filter = String::new();
                }
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

        // ACME: resolve directory_url from environment if not explicitly set
        if self.acme.environment.is_empty() {
            self.acme.environment = "staging".to_string();
        }
        if self.acme.directory_url.is_empty() {
            self.acme.directory_url = match self.acme.environment.as_str() {
                "production" => "https://acme-v02.api.letsencrypt.org/directory".to_string(),
                _ => "https://acme-staging-v02.api.letsencrypt.org/directory".to_string(),
            };
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

    #[test]
    fn duration_negative_integer_rejected() {
        let val = toml::Value::Integer(-1);
        let result = HumaneDuration::try_from(val);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("positive"));
    }

    #[test]
    fn duration_zero_integer_rejected() {
        let val = toml::Value::Integer(0);
        let result = HumaneDuration::try_from(val);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("positive"));
    }

    #[test]
    fn duration_positive_integer_accepted() {
        let val = toml::Value::Integer(60);
        let dur = HumaneDuration::try_from(val).unwrap();
        assert_eq!(dur.0, Duration::from_secs(60));
    }

    #[test]
    fn duration_negative_string_rejected() {
        let val = toml::Value::String("-1s".to_string());
        let result = HumaneDuration::try_from(val);
        assert!(result.is_err(), "negative duration string should be rejected");
    }

    #[test]
    fn duration_valid_string_accepted() {
        let val = toml::Value::String("5m".to_string());
        let dur = HumaneDuration::try_from(val).unwrap();
        assert_eq!(dur.0, Duration::from_secs(300));
    }

    #[test]
    fn zone_payment_config_defaults() {
        let cfg = ZonePaymentConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.create_price, 2);
        assert_eq!(cfg.update_price, 0);
        assert_eq!(cfg.delete_price, 0);
        assert!(cfg.npub_names_free);
        assert_eq!(cfg.mint_url, "https://testnut.cashu.space");
        assert_eq!(cfg.mint_filter, "testnut");
    }

    #[test]
    fn per_zone_payment_config_parsing() {
        let toml = r#"
[nostr]
relays = ["wss://relay.example.com"]
zone = "nodns.shop"

[[dns.zones]]
knot_address = "127.0.0.1:5353"
zone = "nodns.shop"
tsig_key_name = "key1."
tsig_key_secret = "secret1"

[dns.zones.payment]
enabled = true
create_price = 5
update_price = 1
delete_price = 0
npub_names_free = false
mint_url = "https://mint.example.com"
mint_filter = "mint.example"
"#;
        let mut cfg: Config = toml::from_str(toml).unwrap();
        cfg.apply_defaults();
        cfg.validate().unwrap();

        assert_eq!(cfg.dns.zones.len(), 1);
        let zp = &cfg.dns.zones[0].payment;
        assert!(zp.enabled);
        assert_eq!(zp.create_price, 5);
        assert_eq!(zp.update_price, 1);
        assert_eq!(zp.delete_price, 0);
        assert!(!zp.npub_names_free);
        assert_eq!(zp.mint_url, "https://mint.example.com");
        assert_eq!(zp.mint_filter, "mint.example");
    }

    #[test]
    fn global_payment_backward_compat_propagates_to_zones() {
        let toml = r#"
[nostr]
relays = ["wss://relay.example.com"]
zone = "nodns.shop"

[[dns.zones]]
knot_address = "127.0.0.1:5353"
zone = "nodns.shop"
tsig_key_name = "key1."
tsig_key_secret = "secret1"

[payment]
enabled = true
required_sats = 500
update_free = false
cashu_mint_url = "https://mint.example.com"
"#;
        let mut cfg: Config = toml::from_str(toml).unwrap();
        cfg.apply_defaults();
        cfg.validate().unwrap();

        assert_eq!(cfg.dns.zones.len(), 1);
        let zp = &cfg.dns.zones[0].payment;
        assert!(zp.enabled);
        assert_eq!(zp.create_price, 500);
        assert_eq!(zp.update_price, 500);
        assert_eq!(zp.delete_price, 0);
        assert!(zp.npub_names_free);
        assert_eq!(zp.mint_url, "https://mint.example.com");
        assert!(zp.mint_filter.is_empty());
    }

    #[test]
    fn zone_payment_overrides_global() {
        let toml = r#"
[nostr]
relays = ["wss://relay.example.com"]
zone = "nodns.shop"

[[dns.zones]]
knot_address = "127.0.0.1:5353"
zone = "nodns.shop"
tsig_key_name = "key1."
tsig_key_secret = "secret1"

[dns.zones.payment]
enabled = true
create_price = 10
update_price = 0
delete_price = 0
npub_names_free = true
mint_url = "https://testnut.cashu.space"
mint_filter = "testnut"

[payment]
enabled = true
required_sats = 500
update_free = true
cashu_mint_url = "https://other-mint.example.com"
"#;
        let mut cfg: Config = toml::from_str(toml).unwrap();
        cfg.apply_defaults();
        cfg.validate().unwrap();

        let zp = &cfg.dns.zones[0].payment;
        assert_eq!(zp.create_price, 10);
        assert_eq!(zp.mint_url, "https://testnut.cashu.space");
        assert_eq!(zp.mint_filter, "testnut");
    }

    #[test]
    fn zone_lease_config_defaults() {
        let cfg = ZoneLeaseConfig::default();
        assert_eq!(cfg.grace_period_days, 30);
        assert_eq!(cfg.max_lease_days, 365);
        assert!(cfg.operator_lease_expires.is_none());
    }

    #[test]
    fn zone_config_lease_defaults() {
        let cfg = ZoneConfig::default();
        assert_eq!(cfg.lease.grace_period_days, 30);
        assert_eq!(cfg.lease.max_lease_days, 365);
    }

    #[test]
    fn zone_lease_config_custom_values() {
        let toml = r#"
[nostr]
relays = ["wss://relay.example.com"]
zone = "nodns.shop"

[[dns.zones]]
knot_address = "127.0.0.1:5353"
zone = "nodns.shop"
tsig_key_name = "key1."
tsig_key_secret = "secret1"

[dns.zones.lease]
grace_period_days = 90
max_lease_days = 730
operator_lease_expires = "2027-06-08"
"#;
        let mut cfg: Config = toml::from_str(toml).unwrap();
        cfg.apply_defaults();
        cfg.validate().unwrap();

        let lease = &cfg.dns.zones[0].lease;
        assert_eq!(lease.grace_period_days, 90);
        assert_eq!(lease.max_lease_days, 730);
        assert_eq!(lease.operator_lease_expires.as_deref(), Some("2027-06-08"));
    }

    #[test]
    fn zone_lease_backward_compat_zone_gets_defaults() {
        let toml = r#"
[nostr]
relays = ["wss://relay.example.com"]
zone = "nodns.shop"

[dns]
knot_address = "127.0.0.1:5353"
zone = "nodns.shop"
tsig_key_name = "key1."
tsig_key_secret = "secret1"
"#;
        let mut cfg: Config = toml::from_str(toml).unwrap();
        cfg.apply_defaults();
        cfg.validate().unwrap();

        let lease = &cfg.dns.zones[0].lease;
        assert_eq!(lease.grace_period_days, 30);
        assert_eq!(lease.max_lease_days, 365);
    }
}
