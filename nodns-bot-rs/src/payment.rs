//! Payment verification module for Cashu token and Zap receipt validation.
//!
//! Ported 1:1 from `nodns-bot/internal/payment/payment.go` and `cashu.go`.
//!
//! Anti-spam payment model: N sats per NEW DNS record, FREE to update existing
//! records (configurable). Cashu ecash tokens are verified against the mint's
//! checkstate endpoint to ensure proofs are unspent.

use std::str::FromStr;

use cdk::dhke::hash_to_curve;
use cdk::http_client::HttpClient;
use cdk::nuts::{CheckStateRequest, CheckStateResponse, State, Token};
use thiserror::Error;
use tracing::{error, info, warn};

use crate::store::Store;
use crate::types::{DnsRecord, Payment};
use crate::config::ZonePaymentConfig;

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
    mint_url: String,
    required_sats: i64,
    update_free: bool,
    create_price: u64,
    update_price: u64,
    #[allow(dead_code)]
    delete_price: u64,
    #[allow(dead_code)]
    npub_names_free: bool,
    mint_filter: Option<String>,
}

#[allow(dead_code)]
impl Verifier {
    #[allow(dead_code)]
    pub fn new(mint_url: &str, required_sats: i64, update_free: bool) -> Self {
        Self {
            mint_url: mint_url.to_string(),
            required_sats,
            update_free,
            create_price: required_sats as u64,
            update_price: if update_free { 0 } else { required_sats as u64 },
            delete_price: 0,
            npub_names_free: true,
            mint_filter: None,
        }
    }

    pub fn from_zone_config(config: &ZonePaymentConfig) -> Self {
        Self {
            mint_url: config.mint_url.trim_end_matches('/').to_string(),
            required_sats: config.create_price as i64,
            update_free: config.update_price == 0,
            create_price: config.create_price,
            update_price: config.update_price,
            delete_price: config.delete_price,
            npub_names_free: config.npub_names_free,
            mint_filter: if config.mint_filter.is_empty() {
                None
            } else {
                Some(config.mint_filter.clone())
            },
        }
    }

    pub fn create_price(&self) -> u64 {
        self.create_price
    }

    pub fn update_price(&self) -> u64 {
        self.update_price
    }

    pub fn delete_price(&self) -> u64 {
        self.delete_price
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
        let token = Token::from_str(token_string)
            .map_err(|e| PaymentError::TokenDecode(e.to_string()))?;

        // 2. Check mint URL matches
        let token_mint = token
            .mint_url()
            .map_err(|e| PaymentError::TokenDecode(e.to_string()))?;
        let token_mint_str = token_mint.to_string().trim_end_matches('/').to_string();
        let configured_mint = self.mint_url.trim_end_matches('/').to_string();
        if token_mint_str != configured_mint {
            return Err(PaymentError::MintMismatch {
                token_mint: token_mint_str,
                configured_mint,
            });
        }

        if let Some(filter) = &self.mint_filter {
            if !token_mint_str.contains(filter) {
                return Err(PaymentError::MintMismatch {
                    token_mint: token_mint_str,
                    configured_mint: format!("(filter: must contain '{}')", filter),
                });
            }
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
        let url = format!("{}/v1/checkstate", self.mint_url.trim_end_matches('/'));
        let request_body = CheckStateRequest { ys };
        let client = HttpClient::new();
        let response: CheckStateResponse = client
            .post(&url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| {
                error!(
                    mint = %self.mint_url,
                    error = %e,
                    "mint checkstate request failed"
                );
                PaymentError::MintCheckFailed(e.to_string())
            })?
            .json()
            .await
            .map_err(|e| {
                PaymentError::MintCheckFailed(format!("deserializing checkstate: {e}"))
            })?;

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
            mint = %self.mint_url,
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
    let verifier = match verifier {
        None => return Ok(()),
        Some(v) => v,
    };

    let mut total_required: u64 = 0;
    for rec in records {
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
        };
        let v = Verifier::from_zone_config(&cfg);
        assert_eq!(v.mint_url, "https://testnut.cashu.space");
    }

    #[test]
    fn from_zone_config_sets_mint_filter() {
        let cfg = ZonePaymentConfig {
            enabled: true,
            create_price: 2,
            ..ZonePaymentConfig::default()
        };
        let v = Verifier::from_zone_config(&cfg);
        assert_eq!(v.mint_filter.as_deref(), Some("testnut"));
    }

    #[test]
    fn from_zone_config_empty_filter_is_none() {
        let cfg = ZonePaymentConfig {
            enabled: true,
            mint_filter: String::new(),
            ..ZonePaymentConfig::default()
        };
        let v = Verifier::from_zone_config(&cfg);
        assert!(v.mint_filter.is_none());
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
        assert_eq!(v.delete_price(), 1);
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
    fn new_backward_compat_sets_create_price_from_required_sats() {
        let v = Verifier::new("https://testnut.cashu.space", 250, true);
        assert_eq!(v.required_sats, 250);
        assert_eq!(v.create_price(), 250);
        assert_eq!(v.update_price(), 0);
        assert!(v.update_free);
        assert!(v.mint_filter.is_none());
    }

    #[test]
    fn new_backward_compat_update_not_free() {
        let v = Verifier::new("https://testnut.cashu.space", 250, false);
        assert!(!v.update_free);
        assert_eq!(v.update_price(), 250);
        assert!(v.should_require_payment(true));
    }

    #[test]
    fn should_require_payment_disabled_when_zero() {
        let v = Verifier::new("https://testnut.cashu.space", 0, false);
        assert!(!v.should_require_payment(false));
        assert!(!v.should_require_payment(true));
    }

    #[test]
    fn truncate_y_short_and_long() {
        assert_eq!(truncate_y("abc"), "abc");
        assert_eq!(truncate_y("a_very_long_string_here"), "a_very_long_...");
    }
}
