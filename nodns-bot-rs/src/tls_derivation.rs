//! Deterministic TLS private key derivation from Nostr nsec using HKDF.
//!
//! Uses HKDF-SHA512 (RFC 5869) with domain separator "nodns-tls-v2":
//!   prk  = HKDF-Extract(salt="nodns-tls-v2", ikm=nsec_bytes)
//!   key  = HKDF-Expand(prk, info=subdomain_bytes, len=32)
//!
//! Different from DNSSEC SLIP-10 derivation ("Nist256p1 seed" HMAC)
//! to ensure independence between the two schemes.

use hkdf::Hkdf;
use p256::SecretKey;
use sha2::Sha512;

const TLS_DOMAIN_SEPARATOR: &[u8] = b"nodns-tls-v2";

pub fn derive_tls_key(nsec_bytes: &[u8], subdomain: &str) -> Result<SecretKey, String> {
    if nsec_bytes.len() != 32 {
        return Err("nsec must be exactly 32 bytes".into());
    }

    let hkdf = Hkdf::<Sha512>::new(Some(TLS_DOMAIN_SEPARATOR), nsec_bytes);

    let mut okm = [0u8; 32];
    hkdf.expand(subdomain.as_bytes(), &mut okm)
        .map_err(|e| format!("HKDF-Expand failed: {}", e))?;

    SecretKey::from_bytes((&okm).into())
        .map_err(|e| format!("invalid P-256 key derived: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_derivation() {
        let nsec = [42u8; 32];
        let key1 = derive_tls_key(&nsec, "blog").unwrap();
        let key2 = derive_tls_key(&nsec, "blog").unwrap();
        assert_eq!(key1.to_bytes().as_slice(), key2.to_bytes().as_slice());
    }

    #[test]
    fn different_subdomains_different_keys() {
        let nsec = [42u8; 32];
        let key1 = derive_tls_key(&nsec, "blog").unwrap();
        let key2 = derive_tls_key(&nsec, "www").unwrap();
        assert_ne!(key1.to_bytes().as_slice(), key2.to_bytes().as_slice());
    }

    #[test]
    fn rejects_wrong_length() {
        assert!(derive_tls_key(&[1u8; 16], "blog").is_err());
        assert!(derive_tls_key(&[1u8; 33], "blog").is_err());
    }

    #[test]
    fn empty_subdomain_works() {
        let nsec = [42u8; 32];
        assert!(derive_tls_key(&nsec, "").is_ok());
    }

    #[test]
    fn different_nsec_different_key() {
        let key1 = derive_tls_key(&[1u8; 32], "blog").unwrap();
        let key2 = derive_tls_key(&[2u8; 32], "blog").unwrap();
        assert_ne!(key1.to_bytes().as_slice(), key2.to_bytes().as_slice());
    }

    #[test]
    fn independence_from_dnssec_derivation() {
        use hmac::{Hmac, Mac};
        type HmacSha512 = Hmac<sha2::Sha512>;

        let nsec = [42u8; 32];

        let tls_key = derive_tls_key(&nsec, "").unwrap();

        let mut mac = HmacSha512::new_from_slice(b"Nist256p1 seed").unwrap();
        mac.update(&nsec);
        let dnssec_result = mac.finalize().into_bytes();
        let dnssec_key = p256::SecretKey::from_bytes((&dnssec_result[..32]).into()).unwrap();

        assert_ne!(
            tls_key.to_bytes().as_slice(),
            dnssec_key.to_bytes().as_slice(),
            "TLS and DNSSEC derived keys must be independent"
        );
    }
}
