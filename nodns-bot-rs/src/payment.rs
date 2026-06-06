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
}

impl Verifier {
    /// Create a new Cashu payment verifier.
    pub fn new(mint_url: &str, required_sats: i64, update_free: bool) -> Self {
        Self {
            mint_url: mint_url.to_string(),
            required_sats,
            update_free,
        }
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
    ) -> Result<(), PaymentError> {
        // 1. Decode token
        let token = Token::from_str(token_string)
            .map_err(|e| PaymentError::TokenDecode(e.to_string()))?;

        // 2. Check mint URL matches
        let token_mint = token
            .mint_url()
            .map_err(|e| PaymentError::TokenDecode(e.to_string()))?;
        if token_mint.to_string() != self.mint_url {
            return Err(PaymentError::MintMismatch {
                token_mint: token_mint.to_string(),
                configured_mint: self.mint_url.clone(),
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

        Ok(())
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

    // Count records that need payment
    let mut new_record_count = 0usize;
    for rec in records {
        let exists = store
            .has_record(npub, &rec.record_type, &rec.name, zone)
            .map_err(|e| PaymentError::StoreError(e.to_string()))?;
        if !verifier.should_require_payment(exists) {
            continue;
        }
        new_record_count += 1;
    }

    if new_record_count == 0 {
        return Ok(());
    }

    let total_required = (new_record_count as i64) * verifier.required_sats;

    // Look for Cashu payments
    let mut total_verified: i64 = 0;
    for p in payments {
        if p.method != "cashu" {
            continue;
        }
        let remaining = total_required - total_verified;
        if let Err(e) = verifier.verify_payment(&p.token, remaining).await {
            warn!(
                error = %e,
                mint = %p.mint_url,
                "cashu token verification failed"
            );
            continue;
        }
        // Token already validated inside verify_payment; trust the declared amount
        total_verified += p.amount;
        if total_verified >= total_required {
            return Ok(());
        }
    }

    Err(PaymentError::InsufficientTotal {
        verified: total_verified,
        needed: total_required,
        count: new_record_count,
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
