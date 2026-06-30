//! NIP-13 Proof of Work verification.
//!
//! Counts leading zero bits in a Nostr event ID (hex-encoded SHA-256 hash)
//! to determine the mining difficulty, per [NIP-13](https://github.com/nostr-protocol/nips/blob/master/13.md).

/// Count the number of leading zero bits in a hex-encoded event ID.
///
/// Each hex character represents 4 bits. We scan left-to-right, accumulating
/// 4 bits for every `'0'` and then the trailing zero bits of the first non-zero
/// nibble.
#[must_use]
pub fn count_leading_zero_bits(hex_id: &str) -> u32 {
    let mut count: u32 = 0;
    for ch in hex_id.chars() {
        let nibble = match ch.to_digit(16) {
            Some(n) => n,
            None => break,
        };
        if nibble == 0 {
            count += 4;
        } else {
            count += (nibble as u8).leading_zeros() - 4;
            break;
        }
    }
    count
}

/// Returns `true` if the event ID meets or exceeds the required difficulty.
#[must_use]
pub fn verify_pow(event_id: &str, min_difficulty: u32) -> bool {
    let difficulty = count_leading_zero_bits(event_id);
    difficulty >= min_difficulty
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_zeros() {
        assert_eq!(
            count_leading_zero_bits(
                "0000000000000000000000000000000000000000000000000000000000000000"
            ),
            256
        );
    }

    #[test]
    fn no_leading_zeros() {
        assert_eq!(
            count_leading_zero_bits(
                "f000000000000000000000000000000000000000000000000000000000000000"
            ),
            0
        );
    }

    #[test]
    fn known_vector_21_bits() {
        let id = "000006d8c378af1779d2feebc7603a125d99eca0ccf1085959b307f64e5dd358";
        assert_eq!(count_leading_zero_bits(id), 21);
    }

    #[test]
    fn empty_string() {
        assert_eq!(count_leading_zero_bits(""), 0);
    }

    #[test]
    fn invalid_hex_stops_counting() {
        assert_eq!(count_leading_zero_bits("0000xz"), 16);
    }

    #[test]
    fn verify_pow_above_threshold() {
        let id = "000006d8c378af1779d2feebc7603a125d99eca0ccf1085959b307f64e5dd358";
        assert!(verify_pow(id, 20));
        assert!(verify_pow(id, 21));
    }

    #[test]
    fn verify_pow_below_threshold() {
        let id = "000006d8c378af1779d2feebc7603a125d99eca0ccf1085959b307f64e5dd358";
        assert!(!verify_pow(id, 22));
    }

    #[test]
    fn verify_pow_disabled_when_zero() {
        let id = "f000000000000000000000000000000000000000000000000000000000000000";
        assert!(verify_pow(id, 0));
    }

    #[test]
    fn integration_mined_pow_event_accepted() {
        use nostr_sdk::prelude::*;

        let keys = Keys::generate();
        let target: u32 = 4;

        for nonce in 0u64..50_000 {
            let nonce_str = nonce.to_string();
            let target_str = target.to_string();
            let tags = vec![
                Tag::parse(["nonce", nonce_str.as_str(), target_str.as_str()]).unwrap(),
                Tag::parse(["record", "TXT", "", "pow test"]).unwrap(),
            ];

            let event = EventBuilder::new(Kind::Custom(11111), "")
                .tags(tags)
                .sign_with_keys(&keys)
                .unwrap();

            let id = event.id.to_hex();
            let difficulty = count_leading_zero_bits(&id);

            if difficulty >= target {
                assert!(verify_pow(&id, target));
                assert!(verify_pow(&id, target.saturating_sub(1)));
                assert!(!verify_pow(&id, target + 1) || difficulty > target);
                return;
            }
        }

        panic!("failed to mine PoW at difficulty {target} after 50000 attempts");
    }

    #[test]
    fn integration_random_event_rejected_at_threshold() {
        use nostr_sdk::prelude::*;

        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::Custom(11111), "")
            .tags(vec![Tag::parse(["record", "TXT", "", "no pow"]).unwrap()])
            .sign_with_keys(&keys)
            .unwrap();

        let id = event.id.to_hex();
        let difficulty = count_leading_zero_bits(&id);

        assert!(verify_pow(&id, 0), "threshold 0 should always accept");

        if difficulty < 16 {
            assert!(
                !verify_pow(&id, 16),
                "random event (difficulty {difficulty}) should be rejected at threshold 16"
            );
        }

        assert!(
            !verify_pow(&id, 20),
            "random event should essentially never have 20+ leading zero bits"
        );
    }
}
