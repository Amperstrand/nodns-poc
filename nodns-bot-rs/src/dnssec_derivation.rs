//! SLIP-0010 P-256 DNSSEC key derivation from Nostr nsec.
//!
//! Derives a P-256 (`NIST256p1`) private key from a secp256k1 private key
//! using the SLIP-0010 master key derivation formula:
//!
//!   I = HMAC-SHA512(key="Nist256p1 seed", `data=nsec_bytes`)
//!   `private_key` = parse256(I[0:32])
//!
//! Output is PKCS#8 PEM importable by Knot DNS `keymgr import-pem`.

use hmac::{Hmac, Mac};
use p256::elliptic_curve::pkcs8::EncodePrivateKey;
use p256::SecretKey;
use sha2::Sha512;
use thiserror::Error;

type HmacSha512 = Hmac<Sha512>;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum DerivationError {
    #[error("derived private key is zero or >= P-256 curve order")]
    InvalidKey,

    #[error("PKCS#8 export failed: {0}")]
    Pkcs8Export(#[from] pkcs8::Error),
}

// ---------------------------------------------------------------------------
// Derived key container
// ---------------------------------------------------------------------------

pub struct DnssecKey {
    private_key_bytes: [u8; 32],
}

impl DnssecKey {
    pub fn to_pkcs8_pem(&self) -> Result<String, DerivationError> {
        let secret = SecretKey::from_bytes((&self.private_key_bytes).into())
            .map_err(|_| DerivationError::InvalidKey)?;
        let pem = secret.to_pkcs8_pem(pkcs8::LineEnding::LF)?;
        Ok(pem.to_string())
    }

    pub fn private_key_bytes(&self) -> &[u8; 32] {
        &self.private_key_bytes
    }
}

// ---------------------------------------------------------------------------
// Derivation
// ---------------------------------------------------------------------------

pub fn derive_dnssec_key(seed: &[u8]) -> Result<DnssecKey, DerivationError> {
    let mut mac = HmacSha512::new_from_slice(b"Nist256p1 seed").expect("HMAC key length is valid");
    mac.update(seed);
    let result = mac.finalize().into_bytes();

    let mut private_key_bytes = [0u8; 32];
    private_key_bytes.copy_from_slice(&result[..32]);

    if private_key_bytes.iter().all(|&b| b == 0) {
        return Err(DerivationError::InvalidKey);
    }

    Ok(DnssecKey { private_key_bytes })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn hex_decode(hex: &str) -> Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect()
    }

    fn bytes_to_hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn slip10_p256_test_vector() {
        let seed = hex_decode("000102030405060708090a0b0c0d0e0f");
        let expected_key = "612091aaa12e22dd2abef664f8a01a82cae99ad7441b7ef8110424915c268bc2";

        let dnssec_key = derive_dnssec_key(&seed).unwrap();

        assert_eq!(bytes_to_hex(dnssec_key.private_key_bytes()), expected_key);
    }

    #[test]
    fn to_pkcs8_pem_format() {
        let seed = hex_decode("000102030405060708090a0b0c0d0e0f");
        let dnssec_key = derive_dnssec_key(&seed).unwrap();
        let pem = dnssec_key.to_pkcs8_pem().unwrap();

        assert!(pem.starts_with("-----BEGIN PRIVATE KEY-----"));
        assert!(pem.contains("-----END PRIVATE KEY-----"));
    }

    #[test]
    fn deterministic_derivation() {
        let seed = hex_decode("000102030405060708090a0b0c0d0e0f");
        let key1 = derive_dnssec_key(&seed).unwrap();
        let key2 = derive_dnssec_key(&seed).unwrap();

        assert_eq!(key1.private_key_bytes(), key2.private_key_bytes());
    }
}
