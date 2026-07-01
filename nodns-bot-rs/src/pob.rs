//! Proof of Burn verification via ThomasV's notary design.
//!
//! The notary publishes **kind 30021** events after a burn payment is
//! confirmed on-chain. Each proof commits to a kind 11111 event_id (the DNS
//! record being burned for) and contains a Merkle proof linking the burn to a
//! Bitcoin transaction.
//!
//! The bot subscribes to kind 30021, parses the proof tags, optionally
//! re-verifies with the notary API, and stores the verified burn amount in
//! `pob_proofs`. The PoB gate in `event_processor` then queries the store
//! instead of parsing inline tags — this breaks the circular dependency
//! (inline tags change the event_id they commit to).
//!
//! Used as an alternative anti-spam gate alongside NIP-13 Proof of Work.
//! An event passes if it has EITHER sufficient PoW difficulty OR sufficient
//! PoB burn amount.

use std::time::Duration;

use nostr_sdk::Event;
use serde::Deserialize;
use serde_json::{json, Value};
use thiserror::Error;
use tracing::warn;

const NOTARY_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub struct NotaryProof {
    pub event_id: String,
    pub txid: String,
    pub block_height: u64,
    pub nonce: String,
    pub leaf_value: u64,
    pub merkle_index: u64,
    pub merkle_hashes: Vec<(String, u64)>,
    pub chain: String,
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

pub fn parse_kind_30021_proof(event: &Event) -> Option<NotaryProof> {
    let mut event_id: Option<String> = None;
    let mut txid: Option<String> = None;
    let mut block_height: Option<u64> = None;
    let mut nonce: Option<String> = None;
    let mut leaf_value: Option<u64> = None;
    let mut merkle_index: Option<u64> = None;
    let mut merkle_hashes_csv: Option<String> = None;
    let mut chain: Option<String> = None;

    for tag in event.tags.iter() {
        let slice = tag.as_slice();
        if slice.is_empty() {
            continue;
        }
        match slice[0].as_str() {
            "e" if slice.len() >= 2 => {
                event_id = Some(slice[1].clone());
            }
            "n" if slice.len() >= 7 => {
                txid = Some(slice[1].clone());
                block_height = slice[2].parse().ok();
                nonce = Some(slice[3].clone());
                leaf_value = slice[4].parse().ok();
                merkle_index = slice[5].parse().ok();
                merkle_hashes_csv = Some(slice[6].clone());
            }
            "chain" if slice.len() >= 2 => {
                chain = Some(slice[1].clone());
            }
            _ => {}
        }
    }

    let merkle_hashes = merkle_hashes_csv
        .as_deref()
        .map(parse_merkle_hashes)
        .unwrap_or_default();

    Some(NotaryProof {
        event_id: event_id?,
        txid: txid?,
        block_height: block_height?,
        nonce: nonce?,
        leaf_value: leaf_value?,
        merkle_index: merkle_index?,
        merkle_hashes,
        chain: chain.unwrap_or_default(),
    })
}

fn parse_merkle_hashes(csv: &str) -> Vec<(String, u64)> {
    csv.split(',')
        .filter_map(|pair| {
            let mut parts = pair.split(':');
            let hash = parts.next()?.to_string();
            let val: u64 = parts.next()?.parse().ok()?;
            Some((hash, val))
        })
        .collect()
}

pub fn proof_to_json(proof: &NotaryProof) -> Value {
    let merkle_hashes_obj: Vec<Value> = proof
        .merkle_hashes
        .iter()
        .map(|(h, v)| json!({"hash": h, "value": v}))
        .collect();

    json!({
        "version": 0,
        "chain": proof.chain,
        "merkle_index": proof.merkle_index,
        "merkle_hashes": merkle_hashes_obj,
        "event_id": proof.event_id,
        "nonce": proof.nonce,
        "txid": proof.txid,
        "leaf_value": proof.leaf_value,
        "block_height": proof.block_height
    })
}

pub async fn verify_pob_with_notary(
    proof: &NotaryProof,
    notary_url: &str,
) -> Result<bool, PobError> {
    let client = reqwest::Client::builder()
        .timeout(NOTARY_TIMEOUT)
        .build()
        .map_err(|e| PobError::Request(e.to_string()))?;

    let url = format!("{}/verify_proof", notary_url.trim_end_matches('/'));
    let body = proof_to_json(proof);

    let resp = client
        .post(&url)
        .json(&body)
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
pub fn burn_amount_sats(proof: &NotaryProof) -> u64 {
    proof.leaf_value / 1000
}

#[must_use]
#[allow(dead_code)]
pub fn meets_threshold(proof: &NotaryProof, min_sats: u64) -> bool {
    burn_amount_sats(proof) >= min_sats
}

pub async fn verify_or_warn(proof: &NotaryProof, notary_url: &str) -> bool {
    match verify_pob_with_notary(proof, notary_url).await {
        Ok(verified) => verified,
        Err(e) => {
            warn!(
                event_id = %proof.event_id,
                txid = %proof.txid,
                error = %e,
                "notary unreachable, skipping proof verification"
            );
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::prelude::*;

    fn make_kind_30021_event(tags: Vec<Tag>) -> Event {
        let keys = Keys::generate();
        EventBuilder::new(Kind::Custom(30021), "")
            .tags(tags)
            .sign_with_keys(&keys)
            .unwrap()
    }

    fn n_tag() -> Tag {
        Tag::parse([
            "n",
            "aabbccdd",
            "800000",
            "deadbeef",
            "5000000",
            "3",
            "hash1:100,hash2:200",
        ])
        .unwrap()
    }

    #[test]
    fn parse_kind_30021_proof_valid() {
        let event = make_kind_30021_event(vec![
            Tag::parse(["e", "abc123eventid"]).unwrap(),
            n_tag(),
            Tag::parse(["chain", "000000000019d6689c085ae165831e93"]).unwrap(),
        ]);

        let proof = parse_kind_30021_proof(&event).expect("should parse");
        assert_eq!(proof.event_id, "abc123eventid");
        assert_eq!(proof.txid, "aabbccdd");
        assert_eq!(proof.block_height, 800000);
        assert_eq!(proof.nonce, "deadbeef");
        assert_eq!(proof.leaf_value, 5000000);
        assert_eq!(proof.merkle_index, 3);
        assert_eq!(proof.chain, "000000000019d6689c085ae165831e93");
        assert_eq!(proof.merkle_hashes.len(), 2);
        assert_eq!(proof.merkle_hashes[0], ("hash1".to_string(), 100));
        assert_eq!(proof.merkle_hashes[1], ("hash2".to_string(), 200));
    }

    #[test]
    fn parse_kind_30021_proof_missing_e_tag() {
        let event = make_kind_30021_event(vec![n_tag()]);
        assert!(parse_kind_30021_proof(&event).is_none());
    }

    #[test]
    fn parse_kind_30021_proof_missing_n_tag() {
        let event = make_kind_30021_event(vec![
            Tag::parse(["e", "abc123"]).unwrap(),
            Tag::parse(["chain", "deadbeef"]).unwrap(),
        ]);
        assert!(parse_kind_30021_proof(&event).is_none());
    }

    #[test]
    fn parse_kind_30021_proof_n_tag_too_short() {
        let event = make_kind_30021_event(vec![
            Tag::parse(["e", "abc123"]).unwrap(),
            Tag::parse(["n", "txid", "100"]).unwrap(),
        ]);
        assert!(parse_kind_30021_proof(&event).is_none());
    }

    #[test]
    fn parse_kind_30021_proof_n_tag_non_numeric_fields() {
        let event = make_kind_30021_event(vec![
            Tag::parse(["e", "abc123"]).unwrap(),
            Tag::parse(["n", "txid", "not-a-number", "nonce", "1000", "0", "h:1"]).unwrap(),
        ]);
        assert!(parse_kind_30021_proof(&event).is_none());
    }

    #[test]
    fn parse_kind_30021_proof_no_tags() {
        let event = make_kind_30021_event(vec![]);
        assert!(parse_kind_30021_proof(&event).is_none());
    }

    #[test]
    fn parse_kind_30021_proof_chain_defaults_empty() {
        let event = make_kind_30021_event(vec![Tag::parse(["e", "abc123"]).unwrap(), n_tag()]);
        let proof = parse_kind_30021_proof(&event).expect("should parse");
        assert_eq!(proof.chain, "");
    }

    #[test]
    fn parse_kind_30021_proof_empty_merkle_csv() {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::Custom(30021), "")
            .tags(vec![
                Tag::parse(["e", "abc123"]).unwrap(),
                Tag::parse(["n", "txid", "100", "nonce", "1000", "0", ""]).unwrap(),
            ])
            .sign_with_keys(&keys)
            .unwrap();
        let proof = parse_kind_30021_proof(&event).expect("should parse");
        assert!(proof.merkle_hashes.is_empty());
    }

    #[test]
    fn burn_amount_sats_converts_millisats() {
        let proof = NotaryProof {
            event_id: "evt".to_string(),
            txid: "tx".to_string(),
            block_height: 800000,
            nonce: "n".to_string(),
            leaf_value: 5000000,
            merkle_index: 0,
            merkle_hashes: vec![],
            chain: "btc".to_string(),
        };
        assert_eq!(burn_amount_sats(&proof), 5000);
    }

    #[test]
    fn burn_amount_sats_truncates_remainder() {
        let proof = NotaryProof {
            event_id: "evt".to_string(),
            txid: "tx".to_string(),
            block_height: 0,
            nonce: "n".to_string(),
            leaf_value: 999,
            merkle_index: 0,
            merkle_hashes: vec![],
            chain: "btc".to_string(),
        };
        assert_eq!(burn_amount_sats(&proof), 0);
    }

    #[test]
    fn meets_threshold_above() {
        let proof = NotaryProof {
            event_id: "evt".to_string(),
            txid: "tx".to_string(),
            block_height: 0,
            nonce: "n".to_string(),
            leaf_value: 5000000,
            merkle_index: 0,
            merkle_hashes: vec![],
            chain: "btc".to_string(),
        };
        assert!(meets_threshold(&proof, 5000));
        assert!(meets_threshold(&proof, 4000));
    }

    #[test]
    fn meets_threshold_below() {
        let proof = NotaryProof {
            event_id: "evt".to_string(),
            txid: "tx".to_string(),
            block_height: 0,
            nonce: "n".to_string(),
            leaf_value: 100000,
            merkle_index: 0,
            merkle_hashes: vec![],
            chain: "btc".to_string(),
        };
        assert!(!meets_threshold(&proof, 500));
    }

    #[test]
    fn meets_threshold_zero_min_always_passes() {
        let proof = NotaryProof {
            event_id: "evt".to_string(),
            txid: "tx".to_string(),
            block_height: 0,
            nonce: "n".to_string(),
            leaf_value: 0,
            merkle_index: 0,
            merkle_hashes: vec![],
            chain: "btc".to_string(),
        };
        assert!(meets_threshold(&proof, 0));
    }

    #[test]
    fn proof_to_json_has_expected_fields() {
        let proof = NotaryProof {
            event_id: "evt123".to_string(),
            txid: "txid456".to_string(),
            block_height: 800000,
            nonce: "nonce789".to_string(),
            leaf_value: 5000000,
            merkle_index: 2,
            merkle_hashes: vec![("h1".to_string(), 1), ("h2".to_string(), 2)],
            chain: "btc-chain".to_string(),
        };
        let json = proof_to_json(&proof);
        assert_eq!(json["event_id"], "evt123");
        assert_eq!(json["txid"], "txid456");
        assert_eq!(json["block_height"], 800000);
        assert_eq!(json["nonce"], "nonce789");
        assert_eq!(json["leaf_value"], 5000000);
        assert_eq!(json["merkle_index"], 2);
        assert_eq!(json["chain"], "btc-chain");
        assert_eq!(json["merkle_hashes"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn parse_merkle_hashes_valid() {
        let result = parse_merkle_hashes("aaa:1,bbb:2,ccc:3");
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], ("aaa".to_string(), 1));
        assert_eq!(result[2], ("ccc".to_string(), 3));
    }

    #[test]
    fn parse_merkle_hashes_empty() {
        let result = parse_merkle_hashes("");
        assert!(result.is_empty());
    }

    #[test]
    fn parse_merkle_hashes_skips_malformed() {
        let result = parse_merkle_hashes("good:1,badpair,alsogood:2");
        assert_eq!(result.len(), 2);
    }
}
