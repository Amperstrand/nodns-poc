//! Payment verification module for Cashu token and Zap receipt validation.
//!
//! Ported 1:1 from `nodns-bot/internal/payment/payment.go` and `cashu.go`.
//!
//! Anti-spam payment model: N sats per NEW DNS record, FREE to update existing
//! records (configurable). Cashu ecash tokens are verified against the mint's
//! checkstate endpoint to ensure proofs are unspent.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use cdk::dhke::hash_to_curve;
use cdk::http_client::HttpClient;
use cdk::nuts::{CheckStateRequest, CheckStateResponse, State, Token};
use thiserror::Error;
use tracing::{error, info, warn};

use crate::config::ZonePaymentConfig;
use crate::store::Store;
use crate::types::{DnsRecord, Payment};
use nodns_connectors::circuit_breaker::MINT_CIRCUITS;

// ---------------------------------------------------------------------------
// Tunables & per-mint HTTP client cache
// ---------------------------------------------------------------------------

/// Hard cap on a single mint `/v1/checkstate` round-trip (connect + send +
/// body + deserialize). Backed by both a [`tokio::time::timeout`] wrapper and
/// a per-client `reqwest` timeout, so a hung mint can never block a verifier
/// task indefinitely. See issue #60.
const CHECKSTATE_TIMEOUT: Duration = Duration::from_secs(10);

/// Cache of reusable CDK [`HttpClient`]s, keyed by normalized mint URL.
///
/// Each [`HttpClient`] wraps a [`reqwest::Client`] (itself `Arc`-based and
/// cheap to clone) and internally pools TCP connections, so reusing one per
/// mint avoids reconnect/TLS overhead on every verification. See issue #61.
///
/// A [`Mutex`] is used (rather than `dashmap`, which is not a dependency)
/// because the critical section is a single `entry().or_insert_with()` — lock
/// contention is negligible. Poison is recovered rather than panicked.
static MINT_CLIENTS: LazyLock<Mutex<HashMap<String, HttpClient>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Returns a shared [`HttpClient`] for `mint_url`, building and caching one on
/// first use. The underlying `reqwest::Client` is configured with a 10s total
/// timeout as defense in depth (the [`tokio::time::timeout`] wrapper is the
/// primary bound).
///
/// Building the client can only fail if the TLS backend fails to initialize
/// (essentially never in practice); the error is propagated rather than
/// panicked.
fn get_mint_client(mint_url: &str) -> Result<HttpClient, PaymentError> {
    let mut cache = MINT_CLIENTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(client) = cache.get(mint_url) {
        return Ok(client.clone());
    }
    let reqwest_client = reqwest::Client::builder()
        .timeout(CHECKSTATE_TIMEOUT)
        .build()
        .map_err(|e| PaymentError::MintCheckFailed(format!("building http client: {e}")))?;
    let client = HttpClient::from_reqwest(reqwest_client);
    cache.insert(mint_url.to_string(), client.clone());
    Ok(client)
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur during payment verification.
#[derive(Debug, Error)]
pub enum PaymentError {
    #[error("failed to decode cashu token: {0}")]
    TokenDecode(String),

    #[error("token mint {token_mint} does not match configured mint {configured_mint}")]
    MintMismatch {
        token_mint: String,
        configured_mint: String,
    },

    #[error("insufficient payment: got {got} sats, need {needed} sats")]
    InsufficientPayment { got: u64, needed: u64 },

    #[error("hash-to-curve failed for proof: {0}")]
    HashToCurve(String),

    #[error("token contains no proofs")]
    NoProofs,

    #[error("proof {y} is {state} (not unspent)")]
    ProofNotUnspent { y: String, state: String },

    #[error("mint checkstate request failed: {0}")]
    MintCheckFailed(String),

    #[error("mint {mint} temporarily unavailable (circuit open)")]
    MintUnavailable { mint: String },

    #[error("checking record existence: {0}")]
    StoreError(String),

    #[error(
        "insufficient payment: verified {verified} sats, need {needed} sats for {count} new record(s)"
    )]
    InsufficientTotal {
        verified: i64,
        needed: i64,
        count: usize,
    },

    #[error("cashu claim (receive) failed: {0}")]
    #[allow(dead_code)]
    ClaimFailed(String),
}

// ---------------------------------------------------------------------------
// Verifier
// ---------------------------------------------------------------------------

/// Validates Cashu tokens for anti-spam payment verification.
///
/// Mirrors the Go `Verifier` struct from `cashu.go`. Each field maps 1:1:
/// - `mint_url`      → the Cashu mint URL payments must be from
/// - `required_sats` → sats required per new DNS record (0 = disabled)
/// - `update_free`   → if true, updates to existing records are free
pub struct Verifier {
    mint_allowlist: Vec<String>,
    mint_denylist: Vec<String>,
    required_sats: i64,
    update_free: bool,
    create_price: u64,
    update_price: u64,
    npub_names_free: bool,
}

fn normalize_mint(url: &str) -> String {
    url.trim_end_matches('/').to_lowercase()
}

impl Verifier {
    pub fn from_zone_config(config: &ZonePaymentConfig) -> Self {
        let allowlist = if !config.mint_allowlist.is_empty() {
            config
                .mint_allowlist
                .iter()
                .map(|m| normalize_mint(m))
                .collect()
        } else if !config.mint_url.is_empty() {
            vec![normalize_mint(&config.mint_url)]
        } else {
            vec![]
        };

        let denylist = config
            .mint_denylist
            .iter()
            .map(|m| normalize_mint(m))
            .collect();

        Self {
            mint_allowlist: allowlist,
            mint_denylist: denylist,
            required_sats: config.create_price as i64,
            update_free: config.update_price == 0,
            create_price: config.create_price,
            update_price: config.update_price,
            npub_names_free: config.npub_names_free,
        }
    }

    pub fn new(mint_url: &str, _mint_filter: &str, required_sats: i64) -> Self {
        Self {
            mint_allowlist: if mint_url.is_empty() {
                vec![]
            } else {
                vec![normalize_mint(mint_url)]
            },
            mint_denylist: vec![],
            required_sats,
            update_free: true,
            create_price: required_sats as u64,
            update_price: 0,
            npub_names_free: true,
        }
    }

    pub fn create_price(&self) -> u64 {
        self.create_price
    }

    pub fn update_price(&self) -> u64 {
        self.update_price
    }

    /// Determines if payment is needed for this operation.
    ///
    /// Returns `false` when:
    /// - `required_sats == 0` (payment system disabled)
    /// - `is_update == true` AND `update_free == true`
    pub fn should_require_payment(&self, is_update: bool) -> bool {
        if self.required_sats == 0 {
            return false;
        }
        if is_update && self.update_free {
            return false;
        }
        true
    }

    /// Validates a Cashu token against the required amount.
    ///
    /// Verification steps (mirrors Go `VerifyPayment`):
    /// 1. Token can be decoded (valid format)
    /// 2. Token mint matches configured mint URL
    /// 3. Total token amount >= required amount
    /// 4. Compute Y values (hash-to-curve of each proof secret)
    /// 5. Call mint's `/v1/checkstate` with Y values
    /// 6. Verify all proofs are unspent
    pub async fn verify_payment(
        &self,
        token_string: &str,
        required_amount: i64,
    ) -> Result<u64, PaymentError> {
        // 1. Decode token
        let token =
            Token::from_str(token_string).map_err(|e| PaymentError::TokenDecode(e.to_string()))?;

        // 2. Check mint policy (allowlist / denylist / permissive)
        let token_mint = token
            .mint_url()
            .map_err(|e| PaymentError::TokenDecode(e.to_string()))?;
        let token_mint_str = normalize_mint(&token_mint.to_string());

        if self.mint_denylist.contains(&token_mint_str) {
            return Err(PaymentError::MintMismatch {
                token_mint: token_mint_str,
                configured_mint: "denied by denylist".to_string(),
            });
        }

        if !self.mint_allowlist.is_empty() && !self.mint_allowlist.contains(&token_mint_str) {
            return Err(PaymentError::MintMismatch {
                token_mint: token_mint_str,
                configured_mint: format!("allowlist: {:?}", self.mint_allowlist),
            });
        }

        // 3. Check amount
        let token_amount: u64 = token
            .value()
            .map_err(|e| PaymentError::TokenDecode(e.to_string()))?
            .into();
        if (token_amount as i64) < required_amount {
            return Err(PaymentError::InsufficientPayment {
                got: token_amount,
                needed: required_amount as u64,
            });
        }

        // 4. Compute Y values (hash-to-curve of each proof secret)
        let secrets = token.token_secrets();
        if secrets.is_empty() {
            return Err(PaymentError::NoProofs);
        }

        let ys = secrets
            .iter()
            .map(|s| hash_to_curve(&s.to_bytes()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| PaymentError::HashToCurve(e.to_string()))?;

        // 5. Call mint checkstate endpoint (POST /v1/checkstate)
        // Guard: skip mints the circuit breaker has tripped (issue #62).
        if !MINT_CIRCUITS.is_available(&token_mint_str) {
            warn!(mint = %token_mint_str, "circuit open, skipping checkstate");
            return Err(PaymentError::MintUnavailable {
                mint: token_mint_str.clone(),
            });
        }

        let url = format!("{}/v1/checkstate", token_mint_str);
        let request_body = CheckStateRequest { ys };
        let client = get_mint_client(&token_mint_str)?;

        // Bound the full round-trip (connect + send + body + deserialize)
        // with tokio::time::timeout (issue #60). The cached reqwest client
        // also carries its own 10s timeout as defense in depth.
        let response: CheckStateResponse = match tokio::time::timeout(CHECKSTATE_TIMEOUT, async {
            let raw = client.post(&url).json(&request_body).send().await?;
            raw.json::<CheckStateResponse>().await
        })
        .await
        {
            Ok(Ok(response)) => {
                MINT_CIRCUITS.record_success(&token_mint_str);
                response
            }
            Ok(Err(e)) => {
                error!(
                    mint = %token_mint_str,
                    error = %e,
                    "mint checkstate request failed"
                );
                MINT_CIRCUITS.record_failure(&token_mint_str);
                return Err(PaymentError::MintCheckFailed(e.to_string()));
            }
            Err(_elapsed) => {
                warn!(
                    mint = %token_mint_str,
                    timeout_secs = CHECKSTATE_TIMEOUT.as_secs(),
                    "mint checkstate timed out"
                );
                MINT_CIRCUITS.record_failure(&token_mint_str);
                return Err(PaymentError::MintCheckFailed(format!(
                    "checkstate timed out after {}s",
                    CHECKSTATE_TIMEOUT.as_secs()
                )));
            }
        };

        // 6. Verify all proofs are unspent
        for proof_state in &response.states {
            if proof_state.state != State::Unspent {
                return Err(PaymentError::ProofNotUnspent {
                    y: truncate_y(&proof_state.y.to_string()),
                    state: format!("{:?}", proof_state.state),
                });
            }
        }

        info!(
            amount = token_amount,
            proofs = secrets.len(),
            mint = %token_mint_str,
            "cashu token verified"
        );

        Ok(token_amount)
    }
}

// ---------------------------------------------------------------------------
// check_event_payment — standalone verification function
// ---------------------------------------------------------------------------

/// Verifies payment requirements for a DNS update event.
///
/// For each record in the event:
/// - If it's a new record (not in DB), payment is required
/// - If it's an update (already exists), payment may be free (configurable)
///
/// Returns `Ok(())` if all payment requirements are met, error otherwise.
/// If `verifier` is `None`, payment verification is skipped entirely.
pub async fn check_event_payment(
    payments: &[Payment],
    npub: &str,
    records: &[DnsRecord],
    zone: &str,
    store: &Store,
    verifier: Option<&Verifier>,
) -> Result<(), PaymentError> {
    let Some(verifier) = verifier else {
        return Ok(());
    };

    let mut total_required: u64 = 0;
    for rec in records {
        if (rec.name.is_empty() || rec.name == "@") && verifier.npub_names_free {
            continue;
        }
        let exists = store
            .has_record(npub, &rec.record_type, &rec.name, zone)
            .map_err(|e| PaymentError::StoreError(e.to_string()))?;
        if !verifier.should_require_payment(exists) {
            continue;
        }
        total_required += if exists {
            verifier.update_price()
        } else {
            verifier.create_price()
        };
    }

    if total_required == 0 {
        return Ok(());
    }

    let total_required_i64 = total_required as i64;

    // Look for Cashu payments
    let mut total_verified: i64 = 0;
    for p in payments {
        if p.method != "cashu" {
            continue;
        }
        let remaining = total_required_i64 - total_verified;
        match verifier.verify_payment(&p.token, remaining).await {
            Ok(verified_amount) => {
                total_verified += verified_amount as i64;
            }
            Err(e) => {
                warn!(
                    error = %e,
                    mint = %p.mint_url,
                    "cashu token verification failed"
                );
                continue;
            }
        }
        if total_verified >= total_required_i64 {
            return Ok(());
        }
    }

    Err(PaymentError::InsufficientTotal {
        verified: total_verified,
        needed: total_required_i64,
        count: records.len(),
    })
}

// ---------------------------------------------------------------------------
// Cashu claim (hold) — best-effort, called after EPP success
// ---------------------------------------------------------------------------

pub async fn claim_payment(token: &str, mint_url: &str) -> Result<u64, PaymentError> {
    let preview = if token.len() > 40 {
        &token[..40]
    } else {
        token
    };
    tracing::info!(
        mint_url = %mint_url,
        token_preview = %preview,
        "Cashu payment held for reconciliation (pilot Option A: bot holds, operator settles)"
    );
    Ok(0)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Shortens a Y hash for log/error messages (matches Go `truncateY`).
fn truncate_y(y: &str) -> String {
    if y.len() > 12 {
        format!("{}...", &y[..12])
    } else {
        y.to_string()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ZonePaymentConfig;

    #[test]
    fn from_zone_config_trims_trailing_slash() {
        let cfg = ZonePaymentConfig {
            enabled: true,
            create_price: 2,
            update_price: 0,
            delete_price: 0,
            npub_names_free: true,
            mint_url: "https://testnut.cashu.space/".to_string(),
            mint_filter: "testnut".to_string(),
            mint_allowlist: vec![],
            mint_denylist: vec![],
        };
        let v = Verifier::from_zone_config(&cfg);
        assert_eq!(v.mint_allowlist, vec!["https://testnut.cashu.space"]);
    }

    #[test]
    fn from_zone_config_permissive_when_both_empty() {
        let cfg = ZonePaymentConfig {
            enabled: true,
            mint_url: String::new(),
            mint_filter: String::new(),
            mint_allowlist: vec![],
            mint_denylist: vec![],
            ..ZonePaymentConfig::default()
        };
        let v = Verifier::from_zone_config(&cfg);
        assert!(v.mint_allowlist.is_empty());
        assert!(v.mint_denylist.is_empty());
    }

    #[test]
    fn from_zone_config_allowlist_overrides_mint_url() {
        let cfg = ZonePaymentConfig {
            enabled: true,
            mint_url: "https://testnut.cashu.space".to_string(),
            mint_allowlist: vec![
                "https://minibits.cash".to_string(),
                "https://kashu.me".to_string(),
            ],
            ..ZonePaymentConfig::default()
        };
        let v = Verifier::from_zone_config(&cfg);
        assert_eq!(v.mint_allowlist.len(), 2);
        assert!(v
            .mint_allowlist
            .contains(&"https://minibits.cash".to_string()));
        assert!(v.mint_allowlist.contains(&"https://kashu.me".to_string()));
    }

    #[test]
    fn from_zone_config_denylist_populated() {
        let cfg = ZonePaymentConfig {
            enabled: true,
            mint_denylist: vec!["https://evil-mint.example.com".to_string()],
            ..ZonePaymentConfig::default()
        };
        let v = Verifier::from_zone_config(&cfg);
        assert_eq!(v.mint_denylist, vec!["https://evil-mint.example.com"]);
    }

    #[test]
    fn from_zone_config_per_zone_pricing() {
        let cfg = ZonePaymentConfig {
            enabled: true,
            create_price: 5,
            update_price: 3,
            delete_price: 1,
            npub_names_free: false,
            ..ZonePaymentConfig::default()
        };
        let v = Verifier::from_zone_config(&cfg);
        assert_eq!(v.create_price(), 5);
        assert_eq!(v.update_price(), 3);
    }

    #[test]
    fn from_zone_config_update_free_when_price_zero() {
        let cfg = ZonePaymentConfig {
            enabled: true,
            update_price: 0,
            ..ZonePaymentConfig::default()
        };
        let v = Verifier::from_zone_config(&cfg);
        assert!(v.update_free);
        assert!(!v.should_require_payment(true));
    }

    #[test]
    fn from_zone_config_update_not_free_when_price_set() {
        let cfg = ZonePaymentConfig {
            enabled: true,
            create_price: 250,
            update_price: 250,
            ..ZonePaymentConfig::default()
        };
        let v = Verifier::from_zone_config(&cfg);
        assert!(!v.update_free);
        assert_eq!(v.update_price(), 250);
        assert!(v.should_require_payment(true));
    }

    #[test]
    fn should_require_payment_disabled_when_zero() {
        let cfg = ZonePaymentConfig {
            enabled: true,
            create_price: 0,
            ..ZonePaymentConfig::default()
        };
        let v = Verifier::from_zone_config(&cfg);
        assert!(!v.should_require_payment(false));
        assert!(!v.should_require_payment(true));
    }

    #[test]
    fn truncate_y_short_and_long() {
        assert_eq!(truncate_y("abc"), "abc");
        assert_eq!(truncate_y("a_very_long_string_here"), "a_very_long_...");
    }

    #[test]
    fn should_require_payment_matrix() {
        let disabled_cfg = ZonePaymentConfig {
            enabled: true,
            create_price: 0,
            ..ZonePaymentConfig::default()
        };
        let v_disabled = Verifier::from_zone_config(&disabled_cfg);
        assert!(!v_disabled.should_require_payment(false));
        assert!(!v_disabled.should_require_payment(true));

        let free_update_cfg = ZonePaymentConfig {
            enabled: true,
            create_price: 100,
            update_price: 0,
            ..ZonePaymentConfig::default()
        };
        let v_free = Verifier::from_zone_config(&free_update_cfg);
        assert!(v_free.should_require_payment(false));
        assert!(!v_free.should_require_payment(true));

        let paid_update_cfg = ZonePaymentConfig {
            enabled: true,
            create_price: 100,
            update_price: 50,
            ..ZonePaymentConfig::default()
        };
        let v_paid = Verifier::from_zone_config(&paid_update_cfg);
        assert!(v_paid.should_require_payment(false));
        assert!(v_paid.should_require_payment(true));
    }

    #[test]
    fn create_price_returns_config_value() {
        let cfg = ZonePaymentConfig {
            enabled: true,
            create_price: 42,
            ..ZonePaymentConfig::default()
        };
        let v = Verifier::from_zone_config(&cfg);
        assert_eq!(v.create_price(), 42);
    }

    #[test]
    fn update_price_returns_config_value() {
        let cfg = ZonePaymentConfig {
            enabled: true,
            update_price: 17,
            ..ZonePaymentConfig::default()
        };
        let v = Verifier::from_zone_config(&cfg);
        assert_eq!(v.update_price(), 17);
    }
}
