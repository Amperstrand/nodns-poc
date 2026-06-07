//! ACME DNS-01 certificate automation via Let's Encrypt.
//!
//! Automates certificate issuance using DNS-01 challenges. When a user
//! requests a cert the bot:
//! 1. Creates an ACME order
//! 2. Publishes `_acme-challenge` TXT via DDNS UPDATE
//! 3. Tells LE to verify
//! 4. Retrieves and stores the issued certificate

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use instant_acme::{
    Account, AccountCredentials, AuthorizationStatus, ChallengeType, Identifier, NewAccount,
    NewOrder, OrderStatus, RetryPolicy,
};
use thiserror::Error;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::config::AcmeConfig;
use crate::dns::Updater;
use crate::store::Store;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum AcmeError {
    #[error("ACME disabled")]
    Disabled,
    #[error("domain not found in records")]
    DomainNotFound,
    #[error("ACME account error: {0}")]
    AccountError(String),
    #[error("order failed: {0}")]
    OrderFailed(String),
    #[error("challenge failed: {0}")]
    ChallengeFailed(String),
    #[error("DNS update failed: {0}")]
    DnsUpdateFailed(String),
    #[error("store error: {0}")]
    StoreError(String),
}

// ---------------------------------------------------------------------------
// AcmeService
// ---------------------------------------------------------------------------

pub struct AcmeService {
    account: Arc<Mutex<Option<Account>>>,
    config: AcmeConfig,
    updaters: Arc<HashMap<String, Updater>>,
    store: Arc<Store>,
    zones: Vec<String>,
}

impl AcmeService {
    pub fn new(
        config: AcmeConfig,
        updaters: Arc<HashMap<String, Updater>>,
        store: Arc<Store>,
        zones: Vec<String>,
    ) -> Self {
        Self {
            account: Arc::new(Mutex::new(None)),
            config,
            updaters,
            store,
            zones,
        }
    }

    // -------------------------------------------------------------------
    // Account management
    // -------------------------------------------------------------------

    async fn get_or_create_account(&self) -> Result<Account, AcmeError> {
        {
            let guard = self.account.lock().unwrap();
            if let Some(ref acct) = *guard {
                return Ok(acct.clone());
            }
        }

        let account = match self.restore_account()? {
            Some(acct) => acct,
            None => self.create_account().await?,
        };

        {
            let mut guard = self.account.lock().unwrap();
            *guard = Some(account.clone());
        }

        Ok(account)
    }

    fn restore_account(&self) -> Result<Option<Account>, AcmeError> {
        let json = match self.store.get_meta("acme_credentials") {
            Ok(Some(v)) => v,
            Ok(None) => return Ok(None),
            Err(e) => return Err(AcmeError::StoreError(e.to_string())),
        };

        let credentials: AccountCredentials =
            serde_json::from_str(&json).map_err(|e| AcmeError::AccountError(e.to_string()))?;

        let account = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                Account::builder()
                    .map_err(|e| AcmeError::AccountError(e.to_string()))?
                    .from_credentials(credentials)
                    .await
                    .map_err(|e| AcmeError::AccountError(e.to_string()))
            })
        })?;

        info!("restored existing ACME account");
        Ok(Some(account))
    }

    async fn create_account(&self) -> Result<Account, AcmeError> {
        let contact: Vec<String> = if self.config.contact_email.is_empty() {
            vec![]
        } else {
            vec![format!("mailto:{}", self.config.contact_email)]
        };
        let contact_refs: Vec<&str> = contact.iter().map(|s| s.as_str()).collect();

        let (account, credentials) = Account::builder()
            .map_err(|e| AcmeError::AccountError(e.to_string()))?
            .create(
                &NewAccount {
                    contact: &contact_refs,
                    terms_of_service_agreed: true,
                    only_return_existing: false,
                },
                self.config.directory_url.clone(),
                None,
            )
            .await
            .map_err(|e| AcmeError::AccountError(e.to_string()))?;

        let json = serde_json::to_string(&credentials)
            .map_err(|e| AcmeError::StoreError(e.to_string()))?;
        self.store
            .set_meta("acme_credentials", &json)
            .map_err(|e| AcmeError::StoreError(e.to_string()))?;

        info!("created new ACME account");
        Ok(account)
    }

    // -------------------------------------------------------------------
    // Certificate issuance
    // -------------------------------------------------------------------

    pub async fn request_certificate(
        &self,
        domain: &str,
        npub: &str,
    ) -> Result<String, AcmeError> {
        let order_id = Uuid::new_v4().to_string();

        self.store
            .save_acme_order(&order_id, domain, npub, "pending")
            .map_err(|e| AcmeError::StoreError(e.to_string()))?;

        info!(order_id = %order_id, domain = %domain, "starting ACME order");

        if let Err(e) = self.run_acme_flow(&order_id, domain).await {
            let err_msg = e.to_string();
            error!(order_id = %order_id, error = %err_msg, "ACME order failed");
            if let Err(se) = self.store.update_acme_order_status(
                &order_id,
                "failed",
                None,
                None,
                Some(&err_msg),
            ) {
                error!(order_id = %order_id, error = %se, "failed to update order status");
            }
            return Err(e);
        }

        Ok(order_id)
    }

    async fn run_acme_flow(&self, order_id: &str, domain: &str) -> Result<(), AcmeError> {
        let account = self.get_or_create_account().await?;

        let mut order = account
            .new_order(&NewOrder::new(&[Identifier::Dns(domain.to_string())]))
            .await
            .map_err(|e| AcmeError::OrderFailed(e.to_string()))?;

        info!(order_id = %order_id, "ACME order created, processing authorizations");

        let mut authorizations = order.authorizations();
        while let Some(result) = authorizations.next().await {
            let mut authz = result.map_err(|e| AcmeError::ChallengeFailed(e.to_string()))?;
            match authz.status {
                AuthorizationStatus::Pending => {}
                AuthorizationStatus::Valid => continue,
                _ => {
                    return Err(AcmeError::ChallengeFailed(format!(
                        "unexpected authorization status: {:?}",
                        authz.status
                    )));
                }
            }

            let mut challenge = authz
                .challenge(ChallengeType::Dns01)
                .ok_or_else(|| AcmeError::ChallengeFailed("no dns01 challenge found".into()))?;

            let dns_value = challenge.key_authorization().dns_value();
            let challenge_name = format!("_acme-challenge.{}", challenge.identifier());

            info!(
                order_id = %order_id,
                challenge_name = %challenge_name,
                "publishing DNS-01 challenge TXT record"
            );

            let zone = self
                .find_zone_for_domain(&challenge_name)
                .ok_or_else(|| {
                    AcmeError::DnsUpdateFailed(format!(
                        "no zone configured for {}",
                        challenge_name
                    ))
                })?;

            let updater = self.updaters.get(&zone).ok_or_else(|| {
                AcmeError::DnsUpdateFailed(format!("no updater for zone {}", zone))
            })?;

            updater
                .update_record(&challenge_name, self.config.challenge_ttl, 16, &dns_value)
                .await
                .map_err(|e| AcmeError::DnsUpdateFailed(e.to_string()))?;

            info!(order_id = %order_id, "challenge TXT published, signaling ready");

            challenge
                .set_ready()
                .await
                .map_err(|e| AcmeError::ChallengeFailed(e.to_string()))?;
        }

        info!(order_id = %order_id, "polling for order readiness");

        let status = order
            .poll_ready(&RetryPolicy::default())
            .await
            .map_err(|e| AcmeError::OrderFailed(e.to_string()))?;

        if status != OrderStatus::Ready {
            return Err(AcmeError::OrderFailed(format!(
                "unexpected order status after polling: {:?}",
                status
            )));
        }

        info!(order_id = %order_id, "finalizing order");

        let private_key_pem = order
            .finalize()
            .await
            .map_err(|e| AcmeError::OrderFailed(e.to_string()))?;

        info!(order_id = %order_id, "polling for certificate");

        let cert_chain_pem = order
            .poll_certificate(&RetryPolicy::default())
            .await
            .map_err(|e| AcmeError::OrderFailed(e.to_string()))?;

        // Clean up challenge TXT record
        let challenge_fqdn = format!("_acme-challenge.{}", domain);
        if let Some(zone) = self.find_zone_for_domain(&challenge_fqdn) {
            if let Some(updater) = self.updaters.get(&zone) {
                if let Err(e) = updater.delete_record(&challenge_fqdn, 16).await {
                    warn!(
                        order_id = %order_id,
                        error = %e,
                        "failed to clean up challenge TXT record"
                    );
                } else {
                    info!(order_id = %order_id, "challenge TXT record cleaned up");
                }
            }
        }

        self.store
            .update_acme_order_status(
                order_id,
                "issued",
                Some(&cert_chain_pem),
                Some(&private_key_pem),
                None,
            )
            .map_err(|e| AcmeError::StoreError(e.to_string()))?;

        info!(order_id = %order_id, domain = %domain, "certificate issued successfully");
        Ok(())
    }

    // -------------------------------------------------------------------
    // Zone matching
    // -------------------------------------------------------------------

    fn find_zone_for_domain(&self, domain: &str) -> Option<String> {
        let domain = domain.trim_end_matches('.');
        for zone in &self.zones {
            let suffix = format!(".{}", zone);
            if domain.ends_with(&suffix) || domain == zone.as_str() {
                return Some(zone.clone());
            }
        }
        None
    }
}
