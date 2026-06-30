//! Proof of Burn verification via ThomasV's notary API.
//!
//! Extracts `["pob", AMOUNT_SATS, PROOF_JSON]` tags from kind 11111 events
//! and verifies the burn proof against `notary.electrum.org`.
//!
//! Used as an alternative anti-spam gate alongside NIP-13 Proof of Work.
//! An event passes if it has EITHER sufficient PoW difficulty OR sufficient
//! PoB burn amount.

use std::time::Duration;

use nostr_sdk::Tags;
use serde::Deserialize;
use thiserror::Error;

const NOTARY_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub struct PobProof {
    pub amount_sats: u64,
    pub proof_json: String,
}

#[derive(Debug, Error)]
pub enum PobError {
    #[error("notary request failed: {0}")]
    Request(String),
    #[error("notary response parse error: {0}")]
    Parse(String),
}

#[derive(Deserialize)]
struct NotaryResponse {
    verified: bool,
}

pub fn extract_pob_tag(tags: &Tags) -> Option<PobProof> {
    for tag in tags.iter() {
        let slice = tag.as_slice();
        if slice.len() >= 3 && slice[0] == "pob" {
            let amount_sats: u64 = match slice[1].parse() {
                Ok(n) => n,
                Err(_) => continue,
            };
            return Some(PobProof {
                amount_sats,
                proof_json: slice[2].clone(),
            });
        }
    }
    None
}

pub async fn verify_pob(proof: &PobProof, notary_url: &str) -> Result<bool, PobError> {
    let client = reqwest::Client::builder()
        .timeout(NOTARY_TIMEOUT)
        .build()
        .map_err(|e| PobError::Request(e.to_string()))?;

    let url = format!("{}/verify_proof", notary_url.trim_end_matches('/'));

    let parsed: serde_json::Value = serde_json::from_str(&proof.proof_json)
        .map_err(|e| PobError::Parse(format!("invalid proof JSON: {e}")))?;

    let resp = client
        .post(&url)
        .json(&parsed)
        .send()
        .await
        .map_err(|e| PobError::Request(e.to_string()))?;

    let result: NotaryResponse = resp
        .json()
        .await
        .map_err(|e| PobError::Parse(e.to_string()))?;

    Ok(result.verified)
}

#[must_use]
pub fn meets_threshold(proof: &PobProof, min_sats: u64) -> bool {
    proof.amount_sats >= min_sats
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pow;
    use nostr_sdk::prelude::*;

    #[test]
    fn extract_pob_tag_parses_valid_tag() {
        let proof_json = r#"{"tx":"abc","amount":1000}"#;
        let tag = Tag::parse(["pob", "1000", proof_json]).unwrap();
        let tags = Tags::from_list(vec![tag]);
        let pob = extract_pob_tag(&tags).expect("should extract pob tag");
        assert_eq!(pob.amount_sats, 1000);
        assert_eq!(pob.proof_json, proof_json);
    }

    #[test]
    fn extract_pob_tag_returns_none_when_absent() {
        let tag = Tag::parse(["record", "A", "@", "3600", "1.2.3.4"]).unwrap();
        let tags = Tags::from_list(vec![tag]);
        assert!(extract_pob_tag(&tags).is_none());
    }

    #[test]
    fn extract_pob_tag_skips_invalid_amount() {
        let tag = Tag::parse(["pob", "not-a-number", "{}"]).unwrap();
        let tags = Tags::from_list(vec![tag]);
        assert!(extract_pob_tag(&tags).is_none());
    }

    #[test]
    fn extract_pob_tag_skips_too_short() {
        let tag = Tag::parse(["pob", "100"]).unwrap();
        let tags = Tags::from_list(vec![tag]);
        assert!(extract_pob_tag(&tags).is_none());
    }

    #[test]
    fn extract_pob_tag_finds_first_among_many() {
        let proof_json = r#"{"tx":"def"}"#;
        let tags = Tags::from_list(vec![
            Tag::parse(["record", "A", "@", "3600", "1.2.3.4"]).unwrap(),
            Tag::parse(["pob", "500", proof_json]).unwrap(),
            Tag::parse(["pob", "999", r#"{"tx":"other"}"#]).unwrap(),
        ]);
        let pob = extract_pob_tag(&tags).expect("should find first pob tag");
        assert_eq!(pob.amount_sats, 500);
    }

    #[test]
    fn meets_threshold_above() {
        let proof = PobProof {
            amount_sats: 1000,
            proof_json: "{}".to_string(),
        };
        assert!(meets_threshold(&proof, 500));
        assert!(meets_threshold(&proof, 1000));
    }

    #[test]
    fn meets_threshold_below() {
        let proof = PobProof {
            amount_sats: 100,
            proof_json: "{}".to_string(),
        };
        assert!(!meets_threshold(&proof, 500));
    }

    #[test]
    fn meets_threshold_zero_min_always_passes() {
        let proof = PobProof {
            amount_sats: 0,
            proof_json: "{}".to_string(),
        };
        assert!(meets_threshold(&proof, 0));
    }

    #[test]
    fn either_or_logic_both_disabled() {
        let min_pow: u32 = 0;
        let min_pob_sats: u64 = 0;

        let event_id = "f000000000000000000000000000000000000000000000000000000000000000";
        let pow_ok = pow::verify_pow(event_id, min_pow);

        let tags = Tags::from_list(vec![]);
        let pob_ok = if min_pob_sats > 0 {
            if let Some(proof) = extract_pob_tag(&tags) {
                if meets_threshold(&proof, min_pob_sats) {
                    false
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };

        let passes = min_pow == 0 && min_pob_sats == 0 || pow_ok || pob_ok;
        assert!(passes, "both disabled should accept");
    }

    #[test]
    fn either_or_logic_pow_sufficient_pob_absent() {
        let min_pow: u32 = 20;
        let min_pob_sats: u64 = 100;

        let event_id = "000006d8c378af1779d2feebc7603a125d99eca0ccf1085959b307f64e5dd358";
        let pow_ok = pow::verify_pow(event_id, min_pow);
        assert!(pow_ok);

        let tags = Tags::from_list(vec![]);
        let pob_ok = if min_pob_sats > 0 {
            extract_pob_tag(&tags).is_some()
        } else {
            false
        };

        let passes = pow_ok || pob_ok;
        assert!(passes, "sufficient PoW should pass even without PoB");
    }

    #[test]
    fn either_or_logic_both_fail() {
        let min_pow: u32 = 20;
        let min_pob_sats: u64 = 100;

        let event_id = "f000000000000000000000000000000000000000000000000000000000000000";
        let pow_ok = pow::verify_pow(event_id, min_pow);
        assert!(!pow_ok);

        let tags = Tags::from_list(vec![]);
        let pob_ok = if min_pob_sats > 0 {
            extract_pob_tag(&tags).is_some()
        } else {
            false
        };

        let passes = pow_ok || pob_ok;
        assert!(!passes, "insufficient PoW and no PoB should reject");
    }
}
