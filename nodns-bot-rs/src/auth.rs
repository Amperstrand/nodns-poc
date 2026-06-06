//! Authority checker for DNS management permissions.
//!
//! Port of `nodns-bot/internal/auth/authority.go`. Determines who can manage
//! which DNS names: npub-based names are always allowed, custom names require
//! an active delegation from the zone's registrar.

use std::collections::HashMap;
use std::sync::Arc;

use nostr_sdk::nips::nip19::ToBech32;
use nostr_sdk::PublicKey;
use thiserror::Error;
use tracing::{debug, info, warn};

use crate::store::{Store, StoreError};
use crate::types::Delegation;

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("encoding pubkey to npub: {0}")]
    NpubEncoding(String),
    #[error("npub name {name} does not match signer npub {npub}")]
    NpubMismatch { name: String, npub: String },
    #[error("domain {fqdn:?} does not belong to zone {zone:?}")]
    DomainNotInZone { fqdn: String, zone: String },
    #[error("checking delegation for {domain}.{zone}: {source}")]
    DelegationCheck {
        domain: String,
        zone: String,
        source: StoreError,
    },
    #[error("no active delegation for {domain}.{zone}")]
    NoActiveDelegation { domain: String, zone: String },
    #[error("delegation for {domain}.{zone} assigned to {assigned}, not signer {signer}")]
    DelegationAssignedToOther {
        domain: String,
        zone: String,
        assigned: String,
        signer: String,
    },
    #[error("getting registrar key for {zone}: {source}")]
    RegistrarKeyGet { zone: String, source: StoreError },
    #[error("delegation valid_from {valid_from} is in the future (now {now})")]
    DelegationNotYetValid { valid_from: i64, now: i64 },
    #[error("delegation valid_until {valid_until} has expired (now {now})")]
    DelegationExpired { valid_until: i64, now: i64 },
    #[error("delegation valid_until {valid_until} must be after valid_from {valid_from}")]
    DelegationInvalidRange { valid_from: i64, valid_until: i64 },
    #[error("delegation domain {domain:?} is not within zone {zone:?}")]
    DelegationNotInZone { domain: String, zone: String },
    #[error("checking registrar status: {0}")]
    RegistrarCheck(StoreError),
    #[error("signer {signer} is not the registrar for zone {zone}")]
    NotRegistrar { signer: String, zone: String },
}

/// Authority checker determines who can manage which DNS names.
///
/// npub-based names (e.g., `npub1xxx.zone`) are always allowed for their owner.
/// Custom names require an active delegation from the zone's registrar.
pub struct AuthorityChecker {
    store: Arc<Store>,
    config_keys: HashMap<String, String>,
}

impl AuthorityChecker {
    /// Create a new authority checker.
    pub fn new(store: Arc<Store>, config_keys: HashMap<String, String>) -> Self {
        Self { store, config_keys }
    }

    /// Verify that `pubkey_hex` has authority to manage DNS for `fqdn` in `zone`.
    ///
    /// npub1*.zone names are always allowed. Custom names require an active delegation.
    pub fn check_authority(
        &self,
        fqdn: &str,
        zone: &str,
        pubkey_hex: &str,
    ) -> Result<(), AuthError> {
        let fqdn = fqdn.trim_end_matches('.');

        // Convert pubkey hex to npub using nostr_sdk's built-in bech32 encoding
        let public_key = PublicKey::from_hex(pubkey_hex).map_err(|e| {
            AuthError::NpubEncoding(format!("invalid pubkey hex: {}", e))
        })?;
        let npub = public_key
            .to_bech32()
            .map_err(|e| AuthError::NpubEncoding(format!("bech32 encoding: {}", e)))?;

        debug!(fqdn = %fqdn, zone = %zone, npub = %npub, "checking authority");

        // Check if this is an npub-based name
        let zone_suffix = format!(".{}", zone);
        if fqdn.ends_with(&zone_suffix) {
            let prefix = &fqdn[..fqdn.len() - zone_suffix.len()];
            if prefix.starts_with("npub1") {
                if prefix == npub {
                    info!(npub = %npub, zone = %zone, "npub name matches signer, authority granted");
                    return Ok(());
                }
                return Err(AuthError::NpubMismatch {
                    name: prefix.to_string(),
                    npub,
                });
            }
        }

        // Custom name: need active delegation
        let domain = if fqdn.contains('.') {
            let parts: Vec<&str> = fqdn.splitn(2, '.').collect();
            let last = parts[parts.len() - 1];
            if last != zone {
                return Err(AuthError::DomainNotInZone {
                    fqdn: fqdn.to_string(),
                    zone: zone.to_string(),
                });
            }
            parts[0].to_string()
        } else {
            fqdn.to_string()
        };

        debug!(domain = %domain, zone = %zone, "looking up delegation for custom name");

        let delegation = self
            .store
            .get_active_delegation(&domain, zone)
            .map_err(|e| AuthError::DelegationCheck {
                domain: domain.clone(),
                zone: zone.to_string(),
                source: e,
            })?;

        match delegation {
            None => {
                warn!(domain = %domain, zone = %zone, "no active delegation found");
                Err(AuthError::NoActiveDelegation {
                    domain,
                    zone: zone.to_string(),
                })
            }
            Some(del) => {
                if del.npub != npub {
                    warn!(
                        domain = %domain,
                        zone = %zone,
                        assigned = %del.npub,
                        signer = %npub,
                        "delegation assigned to different npub"
                    );
                    return Err(AuthError::DelegationAssignedToOther {
                        domain,
                        zone: zone.to_string(),
                        assigned: del.npub,
                        signer: npub,
                    });
                }
                info!(domain = %domain, zone = %zone, npub = %npub, "delegation found, authority granted");
                Ok(())
            }
        }
    }

    /// Check if `pubkey_hex` is the authorized registrar for `zone`.
    ///
    /// Checks the database first, then falls back to config keys.
    pub fn is_registrar(&self, zone: &str, pubkey_hex: &str) -> Result<bool, AuthError> {
        debug!(zone = %zone, pubkey = %pubkey_hex, "checking registrar status");

        // Check DB first
        let db_key = self
            .store
            .get_registrar_key(zone)
            .map_err(|e| AuthError::RegistrarKeyGet {
                zone: zone.to_string(),
                source: e,
            })?;

        if !db_key.is_empty() {
            let matches = db_key == pubkey_hex;
            debug!(zone = %zone, db_key = %db_key, matches = matches, "registrar key from DB");
            return Ok(matches);
        }

        // Fall back to config keys
        if let Some(cfg_key) = self.config_keys.get(zone) {
            let matches = cfg_key == pubkey_hex;
            debug!(zone = %zone, cfg_key = %cfg_key, matches = matches, "registrar key from config");
            return Ok(matches);
        }

        debug!(zone = %zone, "no registrar key found in DB or config");
        Ok(false)
    }

    /// Validate that a delegation event is properly signed by the zone's registrar.
    ///
    /// Checks timestamps, zone membership, and registrar authority.
    pub fn validate_delegation(
        &self,
        delegation: &Delegation,
        zone: &str,
        signer_pubkey: &str,
    ) -> Result<(), AuthError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        debug!(
            domain = %delegation.domain,
            zone = %zone,
            signer = %signer_pubkey,
            now = now,
            valid_from = delegation.valid_from,
            valid_until = delegation.valid_until,
            "validating delegation"
        );

        if delegation.valid_from > now {
            return Err(AuthError::DelegationNotYetValid {
                valid_from: delegation.valid_from,
                now,
            });
        }
        if delegation.valid_until <= now {
            return Err(AuthError::DelegationExpired {
                valid_until: delegation.valid_until,
                now,
            });
        }
        if delegation.valid_until <= delegation.valid_from {
            return Err(AuthError::DelegationInvalidRange {
                valid_from: delegation.valid_from,
                valid_until: delegation.valid_until,
            });
        }

        let domain = delegation.domain.trim_end_matches('.');
        let zone_suffix = format!(".{}", zone);
        if !domain.ends_with(&zone_suffix) && domain != zone {
            return Err(AuthError::DelegationNotInZone {
                domain: delegation.domain.clone(),
                zone: zone.to_string(),
            });
        }

        let is_registrar = self.is_registrar(zone, signer_pubkey)?;

        if !is_registrar {
            return Err(AuthError::NotRegistrar {
                signer: signer_pubkey.to_string(),
                zone: zone.to_string(),
            });
        }

        info!(
            domain = %delegation.domain,
            zone = %zone,
            signer = %signer_pubkey,
            "delegation validated successfully"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::DelegationRecord;

    fn make_pubkey_hex() -> &'static str {
        // A valid 32-byte secp256k1 public key in hex
        "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
    }

    fn make_npub() -> String {
        let pk = PublicKey::from_hex(make_pubkey_hex()).unwrap();
        pk.to_bech32().unwrap()
    }

    fn make_delegation_record(npub: &str) -> DelegationRecord {
        DelegationRecord {
            event_id: "test_event".to_string(),
            domain: "alice".to_string(),
            zone: "cv".to_string(),
            npub: npub.to_string(),
            pubkey: make_pubkey_hex().to_string(),
            valid_from: 0,
            valid_until: 9999999999,
            renew_by: 9999999999,
            registrar_pubkey: make_pubkey_hex().to_string(),
            created_at: 0,
            processed_at: 0,
        }
    }

    fn setup_store() -> Arc<Store> {
        let store = Store::new(":memory:").expect("open in-memory db");
        store.init().expect("init schema");
        Arc::new(store)
    }

    #[test]
    fn test_npub_name_matches_signer() {
        let store = setup_store();
        let npub = make_npub();
        let checker = AuthorityChecker::new(store, HashMap::new());

        let fqdn = format!("{}.cv.", npub);
        let result = checker.check_authority(&fqdn, "cv", make_pubkey_hex());
        assert!(result.is_ok());
    }

    #[test]
    fn test_npub_name_mismatch() {
        let store = setup_store();
        let checker = AuthorityChecker::new(store, HashMap::new());

        let fqdn = "npub1wrongkey.cv.";
        let result = checker.check_authority(fqdn, "cv", make_pubkey_hex());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("does not match signer npub"));
    }

    #[test]
    fn test_custom_name_with_delegation() {
        let store = setup_store();
        let npub = make_npub();
        let rec = make_delegation_record(&npub);
        store
            .save_delegation(
                &rec.event_id,
                &rec.domain,
                &rec.zone,
                &rec.npub,
                &rec.pubkey,
                rec.valid_from,
                rec.valid_until,
                rec.renew_by,
                &rec.registrar_pubkey,
            )
            .unwrap();

        let checker = AuthorityChecker::new(store, HashMap::new());
        let result = checker.check_authority("alice.cv.", "cv", make_pubkey_hex());
        assert!(result.is_ok());
    }

    #[test]
    fn test_custom_name_without_delegation() {
        let store = setup_store();

        let checker = AuthorityChecker::new(store, HashMap::new());
        let result = checker.check_authority("alice.cv.", "cv", make_pubkey_hex());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("no active delegation"));
    }

    #[test]
    fn test_custom_name_wrong_signer() {
        let store = setup_store();
        let rec = DelegationRecord {
            npub: "npub1someotherkey".to_string(),
            ..make_delegation_record("npub1someotherkey")
        };
        store
            .save_delegation(
                &rec.event_id,
                &rec.domain,
                &rec.zone,
                &rec.npub,
                &rec.pubkey,
                rec.valid_from,
                rec.valid_until,
                rec.renew_by,
                &rec.registrar_pubkey,
            )
            .unwrap();

        let checker = AuthorityChecker::new(store, HashMap::new());
        let result = checker.check_authority("alice.cv.", "cv", make_pubkey_hex());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("assigned to"));
    }

    #[test]
    fn test_is_registrar_from_db() {
        let store = setup_store();
        let npub = make_npub();
        store
            .save_registrar_key("cv", make_pubkey_hex(), &npub, "test", "event1")
            .unwrap();

        let checker = AuthorityChecker::new(store, HashMap::new());
        assert!(checker.is_registrar("cv", make_pubkey_hex()).unwrap());
        assert!(!checker
            .is_registrar("cv", "0000000000000000000000000000000000000000000000000000000000000001")
            .unwrap());
    }

    #[test]
    fn test_is_registrar_from_config() {
        let store = setup_store();
        let mut config = HashMap::new();
        config.insert("cv".to_string(), make_pubkey_hex().to_string());

        let checker = AuthorityChecker::new(store, config);
        assert!(checker.is_registrar("cv", make_pubkey_hex()).unwrap());
    }

    #[test]
    fn test_is_registrar_not_found() {
        let store = setup_store();
        let checker = AuthorityChecker::new(store, HashMap::new());
        assert!(!checker.is_registrar("cv", make_pubkey_hex()).unwrap());
    }

    #[test]
    fn test_validate_delegation_valid() {
        let store = setup_store();
        let npub = make_npub();
        store
            .save_registrar_key("cv", make_pubkey_hex(), &npub, "test", "event1")
            .unwrap();

        let checker = AuthorityChecker::new(store, HashMap::new());
        let delegation = Delegation {
            domain: "alice.cv".to_string(),
            npub,
            valid_from: 0,
            valid_until: 9999999999,
            renew_by: 9999999999,
        };
        let result = checker.validate_delegation(&delegation, "cv", make_pubkey_hex());
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_delegation_expired() {
        let store = setup_store();
        let npub = make_npub();
        store
            .save_registrar_key("cv", make_pubkey_hex(), &npub, "test", "event1")
            .unwrap();

        let checker = AuthorityChecker::new(store, HashMap::new());
        let delegation = Delegation {
            domain: "alice.cv".to_string(),
            npub,
            valid_from: 0,
            valid_until: 1,
            renew_by: 1,
        };
        let result = checker.validate_delegation(&delegation, "cv", make_pubkey_hex());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("expired"));
    }

    #[test]
    fn test_validate_delegation_future() {
        let store = setup_store();
        let npub = make_npub();
        store
            .save_registrar_key("cv", make_pubkey_hex(), &npub, "test", "event1")
            .unwrap();

        let checker = AuthorityChecker::new(store, HashMap::new());
        let delegation = Delegation {
            domain: "alice.cv".to_string(),
            npub,
            valid_from: 99999999999,
            valid_until: 999999999999,
            renew_by: 99999999999,
        };
        let result = checker.validate_delegation(&delegation, "cv", make_pubkey_hex());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("future"));
    }

    #[test]
    fn test_validate_delegation_wrong_zone() {
        let store = setup_store();
        let npub = make_npub();
        store
            .save_registrar_key("cv", make_pubkey_hex(), &npub, "test", "event1")
            .unwrap();

        let checker = AuthorityChecker::new(store, HashMap::new());
        let delegation = Delegation {
            domain: "alice.com".to_string(),
            npub,
            valid_from: 0,
            valid_until: 9999999999,
            renew_by: 9999999999,
        };
        let result = checker.validate_delegation(&delegation, "cv", make_pubkey_hex());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not within zone"));
    }

    #[test]
    fn test_validate_delegation_not_registrar() {
        let store = setup_store();

        let checker = AuthorityChecker::new(store, HashMap::new());
        let delegation = Delegation {
            domain: "alice.cv".to_string(),
            npub: make_npub(),
            valid_from: 0,
            valid_until: 9999999999,
            renew_by: 9999999999,
        };
        let result = checker.validate_delegation(
            &delegation,
            "cv",
            "0000000000000000000000000000000000000000000000000000000000000001",
        );
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not the registrar"));
    }

    #[test]
    fn test_validate_delegation_domain_equals_zone() {
        let store = setup_store();
        let npub = make_npub();
        store
            .save_registrar_key("cv", make_pubkey_hex(), &npub, "test", "event1")
            .unwrap();

        let checker = AuthorityChecker::new(store, HashMap::new());
        let delegation = Delegation {
            domain: "cv".to_string(),
            npub,
            valid_from: 0,
            valid_until: 9999999999,
            renew_by: 9999999999,
        };
        let result = checker.validate_delegation(&delegation, "cv", make_pubkey_hex());
        assert!(result.is_ok());
    }
}
