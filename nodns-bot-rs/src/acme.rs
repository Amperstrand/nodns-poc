//! ACME DNS-01 certificate automation via Let's Encrypt.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use instant_acme::{
    Account, AccountCredentials, AuthorizationStatus, ChallengeType, ExternalAccountKey,
    Identifier, NewAccount, NewOrder, OrderStatus, RetryPolicy,
};
use thiserror::Error;
use tracing::{error, info, warn};

use crate::config::AcmeConfig;
use crate::store::Store;
use base64::Engine;
use nodns_connectors::connector::DnsConnector;
use sha2::{Digest, Sha256};

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

pub struct AcmeService {
    account: Arc<Mutex<HashMap<String, Account>>>,
    config: AcmeConfig,
    updaters: Arc<HashMap<String, Arc<dyn DnsConnector>>>,
    store: Arc<Store>,
    zones: Vec<String>,
    zerossl_eab: Option<ExternalAccountKey>,
}

impl AcmeService {
    pub fn new(
        config: AcmeConfig,
        updaters: Arc<HashMap<String, Arc<dyn DnsConnector>>>,
        store: Arc<Store>,
        zones: Vec<String>,
    ) -> Self {
        let zerossl_eab = if !config.zerossl_eab_kid.is_empty()
            && !config.zerossl_eab_hmac_key.is_empty()
        {
            match base64::engine::general_purpose::STANDARD.decode(&config.zerossl_eab_hmac_key) {
                Ok(key_bytes) => {
                    info!(
                        "ZeroSSL EAB credentials loaded (kid: {}...)",
                        &config.zerossl_eab_kid[..8.min(config.zerossl_eab_kid.len())]
                    );
                    Some(ExternalAccountKey::new(
                        config.zerossl_eab_kid.clone(),
                        &key_bytes,
                    ))
                }
                Err(e) => {
                    warn!(error = %e, "failed to decode ZeroSSL EAB HMAC key — ZeroSSL will not be available");
                    None
                }
            }
        } else {
            None
        };

        Self {
            account: Arc::new(Mutex::new(HashMap::new())),
            config,
            updaters,
            store,
            zones,
            zerossl_eab,
        }
    }

    fn log_stage(&self, order_id: &str, stage: &str, message: &str, details: Option<&str>) {
        if let Err(e) = self
            .store
            .save_acme_order_log(order_id, stage, message, details)
        {
            warn!(order_id = %order_id, error = %e, "failed to save ACME order log");
        }
    }

    async fn get_or_create_account(
        &self,
        order_id: &str,
        directory_url: &str,
    ) -> Result<Account, AcmeError> {
        {
            let guard = self.account.lock().unwrap_or_else(|e| {
                tracing::error!("ACME account cache mutex poisoned, recovering: {}", e);
                e.into_inner()
            });
            if let Some(acct) = guard.get(directory_url) {
                return Ok(acct.clone());
            }
        }

        let account = match self.restore_account(order_id, directory_url)? {
            Some(acct) => acct,
            None => self.create_account(order_id, directory_url).await?,
        };

        {
            let mut guard = self.account.lock().unwrap_or_else(|e| {
                tracing::error!("ACME account cache mutex poisoned, recovering: {}", e);
                e.into_inner()
            });
            if let Some(existing) = guard.get(directory_url) {
                return Ok(existing.clone());
            }
            guard.insert(directory_url.to_string(), account.clone());
        }

        Ok(account)
    }

    fn restore_account(
        &self,
        order_id: &str,
        directory_url: &str,
    ) -> Result<Option<Account>, AcmeError> {
        let meta_key = format!("acme_creds:{directory_url}");
        let json = match self.store.get_meta(&meta_key) {
            Ok(Some(v)) => v,
            Ok(None) => return Ok(None),
            Err(e) => return Err(AcmeError::StoreError(e.to_string())),
        };

        self.log_stage(
            order_id,
            "account_restore",
            "Restoring existing ACME account from stored credentials",
            None,
        );

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

        info!("restored existing ACME account for {}", directory_url);
        Ok(Some(account))
    }

    async fn create_account(
        &self,
        order_id: &str,
        directory_url: &str,
    ) -> Result<Account, AcmeError> {
        let email = if self.config.contact_email.is_empty() {
            "cert@nodns.shop".to_string()
        } else {
            self.config.contact_email.clone()
        };

        let ca_name = if directory_url.contains("zerossl.com") {
            "ZeroSSL"
        } else {
            "Let's Encrypt"
        };

        self.log_stage(
            order_id,
            "account_create",
            &format!("Creating ACME account with {ca_name}"),
            Some(&serde_json::json!({ "email": email, "directory_url": directory_url, "ca": ca_name }).to_string()),
        );

        let contact = [format!("mailto:{email}")];
        let contact_refs: Vec<&str> = contact.iter().map(String::as_str).collect();

        let eab = if directory_url.contains("zerossl.com") {
            self.zerossl_eab.as_ref()
        } else {
            None
        };

        let (account, credentials) = Account::builder()
            .map_err(|e| AcmeError::AccountError(e.to_string()))?
            .create(
                &NewAccount {
                    contact: &contact_refs,
                    terms_of_service_agreed: true,
                    only_return_existing: false,
                },
                directory_url.to_string(),
                eab,
            )
            .await
            .map_err(|e| AcmeError::AccountError(e.to_string()))?;

        let meta_key = format!("acme_creds:{directory_url}");
        let json = serde_json::to_string(&credentials)
            .map_err(|e| AcmeError::StoreError(e.to_string()))?;
        self.store
            .set_meta(&meta_key, &json)
            .map_err(|e| AcmeError::StoreError(e.to_string()))?;

        info!("created new ACME account for {}", directory_url);
        Ok(account)
    }

    pub async fn request_certificate(
        &self,
        order_id: &str,
        domain: &str,
        _npub: &str,
        csr_der: Option<Vec<u8>>,
        directory_url_override: Option<&str>,
    ) -> Result<String, AcmeError> {
        info!(order_id = %order_id, domain = %domain, "starting ACME order");

        let directory_url = directory_url_override.unwrap_or(&self.config.directory_url);

        if let Err(e) = self
            .run_acme_flow(order_id, domain, csr_der.as_deref(), directory_url)
            .await
        {
            let err_msg = e.to_string();
            error!(order_id = %order_id, error = %err_msg, "ACME order failed");
            self.log_stage(order_id, "error", &format!("Order failed: {err_msg}"), None);
            if let Err(se) =
                self.store
                    .update_acme_order_status(order_id, "failed", None, None, Some(&err_msg))
            {
                error!(order_id = %order_id, error = %se, "failed to update order status");
            }
            return Err(e);
        }

        Ok(order_id.to_string())
    }

    async fn run_acme_flow(
        &self,
        order_id: &str,
        domain: &str,
        csr_der: Option<&[u8]>,
        directory_url: &str,
    ) -> Result<(), AcmeError> {
        // Stage 1: Account
        info!(order_id = %order_id, domain = %domain, "stage 1: getting ACME account");

        let account = self
            .get_or_create_account(order_id, directory_url)
            .await
            .map_err(|e| {
                error!(order_id = %order_id, error = %e, "stage 1 failed: account creation");
                e
            })?;

        // Stage 2: Order
        let identifiers = [Identifier::Dns(domain.to_string())];
        let identifier_names: Vec<String> = identifiers
            .iter()
            .map(|i| match i {
                Identifier::Dns(s) => s.clone(),
                _ => format!("{i:?}"),
            })
            .collect();

        self.log_stage(
            order_id,
            "order_create",
            "Creating certificate order",
            Some(
                &serde_json::json!({
                    "domain": domain,
                    "identifiers": identifier_names,
                })
                .to_string(),
            ),
        );

        info!(order_id = %order_id, "stage 2: creating ACME order");

        let mut order = account
            .new_order(&NewOrder::new(&identifiers))
            .await
            .map_err(|e| {
                error!(order_id = %order_id, error = %e, "stage 2 failed: new_order");
                AcmeError::OrderFailed(e.to_string())
            })?;

        let _ = self
            .store
            .update_acme_order_status(order_id, "ordering", None, None, None);

        // Stage 3: Challenges
        info!(order_id = %order_id, "stage 3: processing DNS-01 challenges");

        let mut authorizations = order.authorizations();
        while let Some(result) = authorizations.next().await {
            let mut authz = result.map_err(|e| {
                error!(order_id = %order_id, error = %e, "stage 3 failed: authorization fetch");
                AcmeError::ChallengeFailed(e.to_string())
            })?;
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

            self.log_stage(
                order_id,
                "challenge_prepare",
                "Preparing DNS-01 challenge",
                Some(
                    &serde_json::json!({
                        "challenge_name": challenge_name,
                        "challenge_value": dns_value,
                        "identifier": challenge.identifier().to_string(),
                    })
                    .to_string(),
                ),
            );

            let zone = self.find_zone_for_domain(&challenge_name).ok_or_else(|| {
                AcmeError::DnsUpdateFailed(format!("no zone configured for {challenge_name}"))
            })?;

            let updater = self
                .updaters
                .get(&zone)
                .ok_or_else(|| AcmeError::DnsUpdateFailed(format!("no updater for zone {zone}")))?;

            let fqdn = format!("{challenge_name}.");

            self.log_stage(
                order_id,
                "challenge_publish",
                "Publishing _acme-challenge TXT record via DDNS",
                Some(
                    &serde_json::json!({
                        "fqdn": fqdn,
                        "ttl": self.config.challenge_ttl,
                        "rdata": dns_value,
                    })
                    .to_string(),
                ),
            );

            info!(
                order_id = %order_id,
                challenge_name = %challenge_name,
                "stage 3: publishing DNS-01 challenge TXT"
            );

            updater
                .update_record(&challenge_name, self.config.challenge_ttl, 16, &dns_value)
                .await
                .map_err(|e| {
                    error!(order_id = %order_id, error = %e, "stage 3 failed: DDNS update");
                    AcmeError::DnsUpdateFailed(e.to_string())
                })?;

            self.log_stage(order_id, "challenge_signal", "Signaling CA to verify", None);

            info!(order_id = %order_id, "stage 3: challenge TXT published, signaling ready");

            challenge.set_ready().await.map_err(|e| {
                error!(order_id = %order_id, error = %e, "stage 3 failed: set_ready");
                AcmeError::ChallengeFailed(e.to_string())
            })?;
        }

        let _ =
            self.store
                .update_acme_order_status(order_id, "challenge_published", None, None, None);

        // Stage 4: Poll
        self.log_stage(
            order_id,
            "challenge_verify",
            "Polling CA for verification result",
            None,
        );

        info!(order_id = %order_id, "stage 4: polling for order readiness");

        let status = order
            .poll_ready(&RetryPolicy::default())
            .await
            .map_err(|e| {
                error!(order_id = %order_id, error = %e, "stage 4 failed: poll_ready");
                AcmeError::OrderFailed(e.to_string())
            })?;

        if status != OrderStatus::Ready {
            return Err(AcmeError::OrderFailed(format!(
                "unexpected order status after polling: {status:?}"
            )));
        }

        let _ = self
            .store
            .update_acme_order_status(order_id, "verifying", None, None, None);

        // Stage 5: Finalize
        let finalize_mode = if csr_der.is_some() {
            "csr_provided"
        } else {
            "key_generated"
        };
        self.log_stage(
            order_id,
            "order_finalize",
            "Finalizing order",
            Some(&serde_json::json!({ "mode": finalize_mode }).to_string()),
        );

        info!(order_id = %order_id, "stage 5: finalizing order and downloading certificate");

        let private_key_pem = if let Some(csr) = csr_der {
            order.finalize_csr(csr).await.map_err(|e| {
                error!(order_id = %order_id, error = %e, "stage 5 failed: finalize_csr");
                AcmeError::OrderFailed(e.to_string())
            })?;
            None
        } else {
            Some(order.finalize().await.map_err(|e| {
                error!(order_id = %order_id, error = %e, "stage 5 failed: finalize");
                AcmeError::OrderFailed(e.to_string())
            })?)
        };

        if let Some(ref key_pem) = private_key_pem {
            if let Err(e) = self.store.update_acme_order_status(
                order_id,
                "finalizing",
                None,
                Some(key_pem),
                None,
            ) {
                warn!(order_id = %order_id, error = %e, "failed to persist private key early");
            } else {
                info!(order_id = %order_id, "private key persisted early for crash safety");
            }
        }

        info!(order_id = %order_id, "stage 5: order finalized, polling for certificate");

        let cert_chain_pem = order
            .poll_certificate(&RetryPolicy::default())
            .await
            .map_err(|e| {
                error!(order_id = %order_id, error = %e, "stage 5 failed: poll_certificate");
                AcmeError::OrderFailed(e.to_string())
            })?;

        let cert_fingerprint = {
            let mut hasher = Sha256::new();
            hasher.update(cert_chain_pem.as_bytes());
            let hash = hasher.finalize();
            let mut hex = String::with_capacity(hash.len() * 2);
            for b in &hash {
                use std::fmt::Write;
                write!(hex, "{b:02x}").unwrap();
            }
            hex
        };

        self.log_stage(
            order_id,
            "cert_download",
            "Certificate issued successfully",
            Some(
                &serde_json::json!({
                    "cert_fingerprint_sha256": cert_fingerprint,
                })
                .to_string(),
            ),
        );

        // Cleanup
        let challenge_fqdn = format!("_acme-challenge.{domain}");
        if let Some(zone) = self.find_zone_for_domain(&challenge_fqdn) {
            if let Some(updater) = self.updaters.get(&zone) {
                if let Err(e) = updater.delete_record(&challenge_fqdn, 16).await {
                    warn!(
                        order_id = %order_id,
                        error = %e,
                        "failed to clean up challenge TXT record"
                    );
                    self.log_stage(
                        order_id,
                        "cleanup",
                        "Failed to clean up challenge TXT record",
                        Some(&serde_json::json!({ "error": e.to_string() }).to_string()),
                    );
                } else {
                    info!(order_id = %order_id, "challenge TXT record cleaned up");
                    self.log_stage(order_id, "cleanup", "Cleaned up challenge TXT record", None);
                }
            }
        }

        self.store
            .update_acme_order_status(
                order_id,
                "issued",
                Some(&cert_chain_pem),
                private_key_pem.as_deref(),
                None,
            )
            .map_err(|e| AcmeError::StoreError(e.to_string()))?;

        info!(order_id = %order_id, domain = %domain, "certificate issued successfully");
        Ok(())
    }

    fn find_zone_for_domain(&self, domain: &str) -> Option<String> {
        let domain = domain.trim_end_matches('.');
        for zone in &self.zones {
            let suffix = format!(".{zone}");
            if domain.ends_with(&suffix) || domain == zone.as_str() {
                return Some(zone.clone());
            }
        }
        None
    }

    pub async fn recover_pending_orders(&self) {
        match self.store.list_non_terminal_acme_orders() {
            Ok(orders) if orders.is_empty() => {
                info!("no pending ACME orders to recover");
            }
            Ok(orders) => {
                warn!(
                    count = orders.len(),
                    "found non-terminal ACME orders from previous run, marking as failed"
                );
                for order in &orders {
                    let msg = if order.private_key_pem.is_some() {
                        "bot restarted after key generation — private key preserved, but certificate issuance was interrupted. Please re-request."
                    } else {
                        "bot restarted during certificate issuance. Please re-request."
                    };
                    if let Err(e) = self.store.update_acme_order_status(
                        &order.id,
                        "failed",
                        None,
                        None,
                        Some(msg),
                    ) {
                        error!(order_id = %order.id, error = %e, "failed to mark order as failed during recovery");
                    } else {
                        self.log_stage(&order.id, "recovery", msg, None);
                        warn!(order_id = %order.id, domain = %order.domain, "marked as failed: bot restarted mid-flow");
                    }
                }
            }
            Err(e) => {
                error!(error = %e, "failed to query non-terminal ACME orders for recovery");
            }
        }
    }
}
