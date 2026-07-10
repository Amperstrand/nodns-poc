//! TOML configuration loading with multi-zone support and backward compatibility.
//!
//! Ported 1:1 from `nodns-bot/internal/config/config.go`.

use std::fmt;
use std::path::Path;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OperatingMode {
    #[default]
    Combined,
    Registrar,
    Sync,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct BotConfig {
    #[serde(default)]
    pub mode: OperatingMode,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,

    #[serde(default)]
    pub bot: BotConfig,

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

    #[serde(default)]
    pub dns_update: DnsUpdateConfig,

    #[serde(default)]
    pub resolver: ResolverConfig,

    #[serde(default)]
    pub epp: nodns_epp::EppConfig,
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

fn default_true() -> bool {
    true
}

fn default_backend() -> String {
    "ddns".to_string()
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
    #[serde(default = "default_true")]
    pub store_proofs: bool,
    #[serde(default)]
    pub payment: ZonePaymentConfig,
    #[serde(default)]
    pub lease: ZoneLeaseConfig,
    #[serde(default = "default_backend")]
    pub backend: String,
    #[serde(default)]
    pub cloudflare_api_token: Option<String>,
    #[serde(default)]
    pub cloudflare_zone_id: Option<String>,
    #[serde(default)]
    pub dns_cache_events: bool,
    #[serde(default)]
    pub min_pow: u32,
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
            store_proofs: true,
            payment: ZonePaymentConfig::default(),
            lease: ZoneLeaseConfig::default(),
            backend: "ddns".to_string(),
            cloudflare_api_token: None,
            cloudflare_zone_id: None,
            dns_cache_events: false,
            min_pow: 0,
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
    #[serde(default)]
    pub test_mode: bool,
    pub min_pow: u32,
    #[serde(default)]
    pub min_pob_sats: u64,
    #[serde(default = "default_pob_notary_url")]
    pub pob_notary_url: String,
}

fn default_pob_notary_url() -> String {
    "https://notary.electrum.org/n/api".to_string()
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
            test_mode: false,
            min_pow: 0,
            min_pob_sats: 0,
            pob_notary_url: default_pob_notary_url(),
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
pub struct DnsUpdateConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub listen: String,
    #[serde(default)]
    pub tsig_key_name: String,
    #[serde(default)]
    pub tsig_key_secret: String,
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
    /// "staging" (default) or "production". Used to resolve `directory_url` if not explicitly set.
    pub environment: String,
    /// ACME directory URL. If empty, resolved from `environment` during `apply_defaults()`.
    /// If explicitly set in config, overrides the environment-based URL.
    pub directory_url: String,
    /// Default contact email for ACME account. Can be empty (will use cert@nodns.shop).
    pub contact_email: String,
    pub challenge_ttl: u32,
    /// Default CA: "letsencrypt-staging" (default), "zerossl", "letsencrypt-production"
    pub ca: String,
    /// `ZeroSSL` EAB Key ID (required for `ZeroSSL`)
    pub zerossl_eab_kid: String,
    /// `ZeroSSL` EAB HMAC key (base64-encoded, required for `ZeroSSL`)
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

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ResolverConfig {
    pub enabled: bool,
    pub price_sats: i64,
    pub mint_url: String,
    pub mint_filter: String,
    pub duration_days: u32,
    pub daily_query_limit: i64,
}

impl Default for ResolverConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            price_sats: 10,
            mint_url: "https://testnut.cashu.space".to_string(),
            mint_filter: "testnut".to_string(),
            duration_days: 30,
            daily_query_limit: 10000,
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
                store_proofs: true,
                payment: ZonePaymentConfig::default(),
                lease: ZoneLeaseConfig::default(),
                backend: "ddns".to_string(),
                cloudflare_api_token: None,
                cloudflare_zone_id: None,
                dns_cache_events: false,
                min_pow: 0,
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
                z.tsig_algorithm.clone_from(&self.dns.tsig_algorithm);
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
                    z.payment.update_price = if self.payment.update_free {
                        0
                    } else {
                        self.payment.required_sats as u64
                    };
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

        // ACME: warn if encryption_key is missing while ACME is enabled.
        // Without a persistent key the bot generates a random one at startup,
        // making previously-encrypted ACME private keys unreadable after restart.
        if self.acme.enabled {
            let key_missing = self
                .acme
                .encryption_key
                .as_deref()
                .map(str::is_empty)
                .unwrap_or(true);
            if key_missing {
                tracing::warn!(
                    "acme.encryption_key is not set while ACME is enabled — \
                     encrypted private keys will be unreadable after restart. \
                     Set [acme] encryption_key in config.toml to a hex-encoded \
                     32-byte key to persist keys across restarts."
                );
            }
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
            let is_cloudflare = z.backend == "cloudflare";
            if !is_cloudflare {
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
            } else {
                if z.cloudflare_api_token.as_deref().unwrap_or("").is_empty() {
                    return Err(ConfigError::Validation(format!(
                        "dns.zones[{i}].cloudflare_api_token is required when backend = \"cloudflare\""
                    )));
                }
                if z.cloudflare_zone_id.as_deref().unwrap_or("").is_empty() {
                    return Err(ConfigError::Validation(format!(
                        "dns.zones[{i}].cloudflare_zone_id is required when backend = \"cloudflare\""
                    )));
                }
            }
        }

        if self.acme.enabled
            && self.acme.environment == "production"
            && self
                .acme
                .encryption_key
                .as_deref()
                .map(str::is_empty)
                .unwrap_or(true)
        {
            return Err(ConfigError::Validation(
                "acme.encryption_key is required when acme.enabled = true and acme.environment = \"production\". \
                 Without a persistent key, ACME private keys are unreadable after restart. \
                 Generate one with: openssl rand -hex 32"
                    .into(),
            ));
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
knot_address = "127.0.0.1:53"
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
knot_address = "127.0.0.1:53"
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
        assert_eq!(cfg.dns.zones[0].knot_address, "127.0.0.1:53");
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
knot_address = "127.0.0.1:53"
zone = "nodns.shop"
tsig_key_name = "key1."
tsig_key_secret = "secret1"

[[dns.zones]]
knot_address = "127.0.0.1:5354"
zone = "test.shop"
tsig_key_name = "key2."
tsig_key_secret = "secret2"
default_ttl = 7200
"#;
        let mut cfg: Config = toml::from_str(toml).unwrap();
        cfg.apply_defaults();
        cfg.validate().unwrap();

        assert_eq!(cfg.dns.zones.len(), 2);
        assert_eq!(cfg.dns.zones[0].zone, "nodns.shop");
        assert_eq!(cfg.dns.zones[1].zone, "test.shop");
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
knot_address = "127.0.0.1:53"
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
knot_address = "127.0.0.1:53"
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
knot_address = "127.0.0.1:53"
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
knot_address = "127.0.0.1:53"
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
knot_address = "127.0.0.1:53"
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

    #[test]
    fn acme_config_defaults() {
        let cfg = AcmeConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.environment, "staging");
        assert_eq!(cfg.ca, "letsencrypt-staging");
        assert_eq!(cfg.directory_url, "");
        assert_eq!(cfg.contact_email, "");
        assert_eq!(cfg.challenge_ttl, 300);
        assert!(cfg.encryption_key.is_none());
    }

    #[test]
    fn acme_apply_defaults_staging() {
        let toml = r#"
[acme]
environment = "staging"
"#;
        let mut cfg: Config = toml::from_str(toml).unwrap();
        cfg.apply_defaults();
        assert_eq!(
            cfg.acme.directory_url,
            "https://acme-staging-v02.api.letsencrypt.org/directory"
        );
    }

    #[test]
    fn acme_apply_defaults_production() {
        let toml = r#"
[acme]
environment = "production"
"#;
        let mut cfg: Config = toml::from_str(toml).unwrap();
        cfg.apply_defaults();
        assert_eq!(
            cfg.acme.directory_url,
            "https://acme-v02.api.letsencrypt.org/directory"
        );
    }

    #[test]
    fn acme_apply_defaults_keeps_explicit_url() {
        let toml = r#"
[acme]
environment = "staging"
directory_url = "https://custom-acme.example.com/directory"
"#;
        let mut cfg: Config = toml::from_str(toml).unwrap();
        cfg.apply_defaults();
        assert_eq!(
            cfg.acme.directory_url,
            "https://custom-acme.example.com/directory"
        );
    }

    #[test]
    fn acme_apply_defaults_empty_environment_defaults_to_staging() {
        let toml = r#"
[acme]
"#;
        let mut cfg: Config = toml::from_str(toml).unwrap();
        cfg.apply_defaults();
        assert_eq!(cfg.acme.environment, "staging");
        assert_eq!(
            cfg.acme.directory_url,
            "https://acme-staging-v02.api.letsencrypt.org/directory"
        );
    }

    #[test]
    fn policy_config_defaults() {
        let cfg = PolicyConfig::default();
        assert_eq!(cfg.max_records, 20);
        assert_eq!(cfg.rate_limit, 5);
        assert!(!cfg.block_private_ip);
        assert_eq!(cfg.max_txt_length, 512);
        assert!(cfg.allowed_types.contains(&"A".to_string()));
        assert!(cfg.allowed_types.contains(&"AAAA".to_string()));
        assert!(cfg.allowed_types.contains(&"CNAME".to_string()));
        assert!(cfg.allowed_types.contains(&"TXT".to_string()));
        assert!(cfg.allowed_types.contains(&"MX".to_string()));
        assert_eq!(cfg.allowed_types.len(), 5);
    }

    #[test]
    fn store_config_defaults() {
        let cfg = StoreConfig::default();
        assert_eq!(cfg.path, "records.db");
    }

    #[test]
    fn validation_fails_with_empty_nostr_zone() {
        let toml = r#"
[nostr]
relays = ["wss://relay.example.com"]
"#;
        let mut cfg: Config = toml::from_str(toml).unwrap();
        cfg.apply_defaults();
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("nostr.zone"));
    }

    #[test]
    fn epp_config_defaults_when_section_absent() {
        let toml = r#"
[nostr]
relays = ["wss://relay.example.com"]
zone = "nodns.shop"

[[dns.zones]]
knot_address = "127.0.0.1:53"
zone = "nodns.shop"
tsig_key_name = "key1."
tsig_key_secret = "secret1"
"#;
        let mut cfg: Config = toml::from_str(toml).unwrap();
        cfg.apply_defaults();
        assert_eq!(cfg.epp.host, "registry.ola.cv");
        assert_eq!(cfg.epp.port, 700);
        assert_eq!(cfg.epp.pool_size, 8);
        assert_eq!(cfg.epp.timeout_secs, 30);
        assert_eq!(cfg.epp.password_env_var, "CV_EPP_PASSWORD");
    }

    #[test]
    fn epp_config_custom_values() {
        let toml = r#"
[nostr]
relays = ["wss://relay.example.com"]
zone = "nodns.shop"

[[dns.zones]]
knot_address = "127.0.0.1:53"
zone = "nodns.shop"
tsig_key_name = "key1."
tsig_key_secret = "secret1"

[epp]
host = "custom.registry.cv"
port = 8443
username = "bridge"
pool_size = 4
timeout_secs = 60
"#;
        let mut cfg: Config = toml::from_str(toml).unwrap();
        cfg.apply_defaults();
        assert_eq!(cfg.epp.host, "custom.registry.cv");
        assert_eq!(cfg.epp.port, 8443);
        assert_eq!(cfg.epp.username, "bridge");
        assert_eq!(cfg.epp.pool_size, 4);
        assert_eq!(cfg.epp.timeout_secs, 60);
    }

    #[test]
    fn zone_backend_defaults_to_ddns() {
        let cfg = ZoneConfig::default();
        assert_eq!(cfg.backend, "ddns");
        assert!(!cfg.dns_cache_events);
        assert!(cfg.cloudflare_api_token.is_none());
        assert!(cfg.cloudflare_zone_id.is_none());
    }

    #[test]
    fn cloudflare_backend_config_parsing() {
        let toml = r#"
[nostr]
relays = ["wss://relay.example.com"]
zone = "dns4sats.xyz"

[[dns.zones]]
zone = "dns4sats.xyz"
backend = "cloudflare"
cloudflare_api_token = "secret-token"
cloudflare_zone_id = "abc123zoneid"
dns_cache_events = true
"#;
        let mut cfg: Config = toml::from_str(toml).unwrap();
        cfg.apply_defaults();
        cfg.validate().unwrap();

        let z = &cfg.dns.zones[0];
        assert_eq!(z.backend, "cloudflare");
        assert_eq!(z.cloudflare_api_token.as_deref(), Some("secret-token"));
        assert_eq!(z.cloudflare_zone_id.as_deref(), Some("abc123zoneid"));
        assert!(z.dns_cache_events);
    }

    #[test]
    fn cloudflare_backend_validation_missing_token() {
        let toml = r#"
[nostr]
relays = ["wss://relay.example.com"]
zone = "dns4sats.xyz"

[[dns.zones]]
zone = "dns4sats.xyz"
backend = "cloudflare"
cloudflare_zone_id = "abc123"
"#;
        let mut cfg: Config = toml::from_str(toml).unwrap();
        cfg.apply_defaults();
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("cloudflare_api_token"));
    }

    #[test]
    fn cloudflare_backend_validation_missing_zone_id() {
        let toml = r#"
[nostr]
relays = ["wss://relay.example.com"]
zone = "dns4sats.xyz"

[[dns.zones]]
zone = "dns4sats.xyz"
backend = "cloudflare"
cloudflare_api_token = "token"
"#;
        let mut cfg: Config = toml::from_str(toml).unwrap();
        cfg.apply_defaults();
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("cloudflare_zone_id"));
    }

    #[test]
    fn cloudflare_backend_skips_ddns_validation() {
        let toml = r#"
[nostr]
relays = ["wss://relay.example.com"]
zone = "dns4sats.xyz"

[[dns.zones]]
zone = "dns4sats.xyz"
backend = "cloudflare"
cloudflare_api_token = "token"
cloudflare_zone_id = "zoneid"
"#;
        let mut cfg: Config = toml::from_str(toml).unwrap();
        cfg.apply_defaults();
        cfg.validate().unwrap();
    }

    #[test]
    fn dns_cache_events_defaults_false() {
        let toml = r#"
[nostr]
relays = ["wss://relay.example.com"]
zone = "nodns.shop"

[[dns.zones]]
knot_address = "127.0.0.1:53"
zone = "nodns.shop"
tsig_key_name = "key1."
tsig_key_secret = "secret1"
"#;
        let mut cfg: Config = toml::from_str(toml).unwrap();
        cfg.apply_defaults();
        cfg.validate().unwrap();
        assert!(!cfg.dns.zones[0].dns_cache_events);
    }

    #[test]
    fn mixed_backend_multi_zone_config() {
        let toml = r#"
[nostr]
relays = ["wss://relay.example.com"]
zone = "multi"

[[dns.zones]]
knot_address = "127.0.0.1:53"
zone = "nodns.shop"
tsig_key_name = "key1."
tsig_key_secret = "secret1"

[[dns.zones]]
zone = "dns4sats.xyz"
backend = "cloudflare"
cloudflare_api_token = "token"
cloudflare_zone_id = "zoneid"
dns_cache_events = true
"#;
        let mut cfg: Config = toml::from_str(toml).unwrap();
        cfg.apply_defaults();
        cfg.validate().unwrap();

        assert_eq!(cfg.dns.zones.len(), 2);
        assert_eq!(cfg.dns.zones[0].backend, "ddns");
        assert_eq!(cfg.dns.zones[1].backend, "cloudflare");
        assert!(cfg.dns.zones[1].dns_cache_events);
    }
}
