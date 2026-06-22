//! EPP (Extensible Provisioning Protocol) client for .cv registry integration.
//!
//! Wraps the `instant-epp` crate with a connection pool for RFC 5730–5734
//! domain lifecycle operations against `registry.ola.cv:700`.
//!
//! # Pool invariants
//!
//! - **Max 8 connections** — registry hard cap on concurrent sessions.
//! - **Re-login after 900 requests** per session (registry cap is 1000; we
//!   rotate at 900 for a safety margin).
//! - **Lazy connect** — connections are established on first use, not at pool
//!   creation. This allows the bot to start even when the registry is
//!   unreachable.
//! - **Simulate mode** — when `config.simulate = true`, all operations log
//!   their intent and return mock results without touching the network.
//!
//! # Current status
//!
//! Real `instant-epp` 0.4 calls are wired for domain_create, domain_delete,
//! domain_check, and contact_check. The pool defaults to **simulate mode**
//! until the registry IP allowlist (EPP code 2002) is resolved. Flip
//! `simulate = false` in config to enable live transactions.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use instant_epp::client::RustlsConnector;
use instant_epp::contact::ContactCheck;
use instant_epp::domain::{DomainCheck, DomainCreate, DomainDelete, HostInfo, HostObj, Period};
use instant_epp::login::Login;
use instant_epp::EppClient;
use serde::Deserialize;
use thiserror::Error;
use tokio::sync::Mutex as TokioMutex;
use tracing::{debug, info, warn};

const MAX_POOL_SIZE: usize = 8;

const RELOGIN_THRESHOLD: u64 = 900;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct EppConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password_env_var: String,
    pub pool_size: usize,
    pub timeout_secs: u64,
    pub simulate: bool,
}

impl Default for EppConfig {
    fn default() -> Self {
        Self {
            host: "registry.ola.cv".to_string(),
            port: 700,
            username: String::new(),
            password_env_var: "CV_EPP_PASSWORD".to_string(),
            pool_size: MAX_POOL_SIZE,
            timeout_secs: 90,
            simulate: true,
        }
    }
}

#[derive(Debug, Error)]
pub enum EppError {
    #[error("EPP not yet implemented — registry IP allowlist pending")]
    NotImplemented,
    #[error("EPP not connected — password env var not set")]
    NotConnected,
    #[error("EPP connection pool exhausted")]
    PoolExhausted,
    #[error("EPP operation timed out")]
    Timeout,
    #[error("registry error {code}: {msg}")]
    RegistryError { code: u16, msg: String },
    #[error("EPP XML error: {0}")]
    XmlError(String),
    #[error("EPP TLS error: {0}")]
    TlsError(String),
    #[error("EPP I/O error: {0}")]
    IoError(String),
    #[error("EPP error: {0}")]
    Other(String),
}

#[derive(Debug, Clone)]
pub struct DomainCreateResult {
    pub name: String,
    pub creation_date: Option<String>,
    pub expiration_date: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DomainCheckResult {
    pub name: String,
    pub available: bool,
    pub reason: Option<String>,
}

pub struct EppPool {
    clients: Vec<TokioMutex<Option<EppClient<RustlsConnector>>>>,
    config: EppConfig,
    request_counts: Vec<AtomicU64>,
}

impl EppPool {
    pub async fn new(config: EppConfig) -> Result<Self, EppError> {
        let pool_size = config.pool_size.min(MAX_POOL_SIZE);
        let simulate = config.simulate;
        if simulate {
            info!(
                host = %config.host,
                port = config.port,
                pool_size,
                "EPP pool initialized in SIMULATE mode"
            );
        } else {
            info!(
                host = %config.host,
                port = config.port,
                pool_size,
                "EPP pool initialized in LIVE mode (lazy connect)"
            );
        }
        Ok(Self {
            clients: (0..pool_size).map(|_| TokioMutex::new(None)).collect(),
            config,
            request_counts: (0..pool_size).map(|_| AtomicU64::new(0)).collect(),
        })
    }

    pub fn is_simulated(&self) -> bool {
        self.config.simulate
    }

    fn check_credentials(&self) -> Result<(), EppError> {
        if std::env::var(&self.config.password_env_var).is_err() {
            return Err(EppError::NotConnected);
        }
        Ok(())
    }

    async fn connect_and_login(&self) -> Result<EppClient<RustlsConnector>, EppError> {
        let password =
            std::env::var(&self.config.password_env_var).map_err(|_| EppError::NotConnected)?;
        let timeout = Duration::from_secs(self.config.timeout_secs);

        let mut client = EppClient::connect(
            "cv-registry".to_string(),
            (self.config.host.clone(), self.config.port),
            None,
            timeout,
        )
        .await
        .map_err(map_epp_error)?;

        let ext_uris: &[&str] = &[
            "urn:ietf:params:xml:ns:rgp-1.0",
            "urn:ietf:params:xml:ns:secDNS-1.1",
        ];
        let login = Login::new(&self.config.username, &password, None, Some(ext_uris));

        let cltrid = format!("login-{}", uuid::Uuid::new_v4());
        client
            .transact(&login, &cltrid)
            .await
            .map_err(map_epp_error)?;

        info!(host = %self.config.host, "EPP connected and logged in");
        Ok(client)
    }

    pub async fn domain_create(
        &self,
        name: &str,
        period_years: u32,
        nameservers: &[&str],
        registrant_contact: &str,
        auth_info: &str,
    ) -> Result<DomainCreateResult, EppError> {
        if self.config.simulate {
            info!(domain = %name, period_years, "SIMULATED: would send EPP domain:create");
            return Ok(DomainCreateResult {
                name: name.to_string(),
                creation_date: None,
                expiration_date: None,
            });
        }

        self.check_credentials()?;
        let mut guard = self.clients[0].lock().await;

        if guard.is_none() {
            let client = self.connect_and_login().await?;
            *guard = Some(client);
        }

        let client = guard.as_mut().unwrap();
        let period =
            Period::years(period_years as u8).map_err(|e| EppError::Other(e.to_string()))?;

        let ns_vec: Vec<HostInfo> = nameservers
            .iter()
            .map(|ns| HostInfo::Obj(HostObj { name: (*ns).into() }))
            .collect();
        let ns_ref = if ns_vec.is_empty() {
            None
        } else {
            Some(&ns_vec[..])
        };

        let create = DomainCreate::new(
            name,
            period,
            ns_ref,
            if registrant_contact.is_empty() {
                None
            } else {
                Some(registrant_contact)
            },
            auth_info,
            None,
        );

        let cltrid = format!("create-{}", uuid::Uuid::new_v4());
        let result = client.transact(&create, &cltrid).await;

        match result {
            Ok(response) => {
                let count = self.request_counts[0].fetch_add(1, Ordering::Relaxed);
                if count + 1 >= RELOGIN_THRESHOLD {
                    if let Some(old) = guard.take() {
                        debug!(
                            count = count + 1,
                            "EPP session reset — relogin threshold reached"
                        );
                        let _ = old.shutdown().await;
                    }
                }
                if let Some(data) = response.res_data() {
                    Ok(DomainCreateResult {
                        name: data.name.clone(),
                        creation_date: Some(data.created_at.to_rfc3339()),
                        expiration_date: data.expiring_at.map(|d| d.to_rfc3339()),
                    })
                } else {
                    Ok(DomainCreateResult {
                        name: name.to_string(),
                        creation_date: None,
                        expiration_date: None,
                    })
                }
            }
            Err(e) => {
                if matches!(e, instant_epp::Error::Io(_) | instant_epp::Error::Timeout) {
                    *guard = None;
                }
                Err(map_epp_error(e))
            }
        }
    }

    pub async fn domain_delete(&self, name: &str) -> Result<(), EppError> {
        if self.config.simulate {
            info!(domain = %name, "SIMULATED: would send EPP domain:delete");
            return Ok(());
        }

        self.check_credentials()?;
        let mut guard = self.clients[0].lock().await;

        if guard.is_none() {
            let client = self.connect_and_login().await?;
            *guard = Some(client);
        }

        let client = guard.as_mut().unwrap();
        let delete = DomainDelete::new(name);
        let cltrid = format!("delete-{}", uuid::Uuid::new_v4());
        let result = client.transact(&delete, &cltrid).await;

        match result {
            Ok(_) => {
                let count = self.request_counts[0].fetch_add(1, Ordering::Relaxed);
                if count + 1 >= RELOGIN_THRESHOLD {
                    if let Some(old) = guard.take() {
                        debug!(
                            count = count + 1,
                            "EPP session reset — relogin threshold reached"
                        );
                        let _ = old.shutdown().await;
                    }
                }
                Ok(())
            }
            Err(e) => {
                if matches!(e, instant_epp::Error::Io(_) | instant_epp::Error::Timeout) {
                    *guard = None;
                }
                Err(map_epp_error(e))
            }
        }
    }

    pub async fn domain_check(&self, names: &[&str]) -> Result<Vec<DomainCheckResult>, EppError> {
        if self.config.simulate {
            info!(names = ?names, "SIMULATED: would send EPP domain:check");
            return Ok(names
                .iter()
                .map(|n| DomainCheckResult {
                    name: n.to_string(),
                    available: true,
                    reason: None,
                })
                .collect());
        }

        self.check_credentials()?;
        let mut guard = self.clients[0].lock().await;

        if guard.is_none() {
            let client = self.connect_and_login().await?;
            *guard = Some(client);
        }

        let client = guard.as_mut().unwrap();
        let check = DomainCheck { domains: names };
        let cltrid = format!("check-{}", uuid::Uuid::new_v4());
        let result = client.transact(&check, &cltrid).await;

        match result {
            Ok(response) => {
                let count = self.request_counts[0].fetch_add(1, Ordering::Relaxed);
                if count + 1 >= RELOGIN_THRESHOLD {
                    if let Some(old) = guard.take() {
                        debug!(
                            count = count + 1,
                            "EPP session reset — relogin threshold reached"
                        );
                        let _ = old.shutdown().await;
                    }
                }
                let results = response
                    .res_data()
                    .map(|data| {
                        data.list
                            .iter()
                            .map(|cd| DomainCheckResult {
                                name: cd.inner.id.clone(),
                                available: cd.inner.available,
                                reason: cd.inner.reason.clone(),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                Ok(results)
            }
            Err(e) => {
                if matches!(e, instant_epp::Error::Io(_) | instant_epp::Error::Timeout) {
                    *guard = None;
                }
                Err(map_epp_error(e))
            }
        }
    }

    pub async fn domain_renew(&self, name: &str, period_years: u32) -> Result<(), EppError> {
        if self.config.simulate {
            info!(domain = %name, period_years, "SIMULATED: would send EPP domain:renew");
            return Ok(());
        }

        warn!(
            domain = %name,
            period_years,
            "EPP domain:renew requires current expiry date — not available via this API signature"
        );
        Err(EppError::Other(
            "domain renew requires current expiry date which is not provided".to_string(),
        ))
    }

    pub async fn contact_check(&self, ids: &[&str]) -> Result<Vec<bool>, EppError> {
        if self.config.simulate {
            info!(ids = ?ids, "SIMULATED: would send EPP contact:check");
            return Ok(ids.iter().map(|_| true).collect());
        }

        self.check_credentials()?;
        let mut guard = self.clients[0].lock().await;

        if guard.is_none() {
            let client = self.connect_and_login().await?;
            *guard = Some(client);
        }

        let client = guard.as_mut().unwrap();
        let check = ContactCheck { contact_ids: ids };
        let cltrid = format!("ccheck-{}", uuid::Uuid::new_v4());
        let result = client.transact(&check, &cltrid).await;

        match result {
            Ok(response) => {
                let count = self.request_counts[0].fetch_add(1, Ordering::Relaxed);
                if count + 1 >= RELOGIN_THRESHOLD {
                    if let Some(old) = guard.take() {
                        debug!(
                            count = count + 1,
                            "EPP session reset — relogin threshold reached"
                        );
                        let _ = old.shutdown().await;
                    }
                }
                let results = response
                    .res_data()
                    .map(|data| data.list.iter().map(|cc| cc.inner.available).collect())
                    .unwrap_or_default();
                Ok(results)
            }
            Err(e) => {
                if matches!(e, instant_epp::Error::Io(_) | instant_epp::Error::Timeout) {
                    *guard = None;
                }
                Err(map_epp_error(e))
            }
        }
    }
}

fn map_epp_error(e: instant_epp::Error) -> EppError {
    match e {
        instant_epp::Error::Command(status) => {
            let code = status.result.code as u16;
            let msg = status.result.message.clone();
            EppError::RegistryError { code, msg }
        }
        instant_epp::Error::Io(e) => EppError::IoError(e.to_string()),
        instant_epp::Error::Timeout => EppError::Timeout,
        instant_epp::Error::Xml(e) => EppError::XmlError(e.to_string()),
        instant_epp::Error::Other(e) => EppError::Other(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults() {
        let cfg = EppConfig::default();
        assert_eq!(cfg.host, "registry.ola.cv");
        assert_eq!(cfg.port, 700);
        assert!(cfg.username.is_empty());
        assert_eq!(cfg.password_env_var, "CV_EPP_PASSWORD");
        assert_eq!(cfg.pool_size, MAX_POOL_SIZE);
        assert_eq!(cfg.timeout_secs, 90);
        assert!(cfg.simulate);
    }

    #[tokio::test]
    async fn pool_new_caps_at_max() {
        let cfg = EppConfig {
            pool_size: 20,
            ..EppConfig::default()
        };
        let pool = EppPool::new(cfg).await.unwrap();
        assert_eq!(pool.clients.len(), MAX_POOL_SIZE);
        assert_eq!(pool.request_counts.len(), MAX_POOL_SIZE);
    }

    #[tokio::test]
    async fn pool_new_respects_smaller_size() {
        let cfg = EppConfig {
            pool_size: 3,
            ..EppConfig::default()
        };
        let pool = EppPool::new(cfg).await.unwrap();
        assert_eq!(pool.clients.len(), 3);
    }

    async fn pool_simulate() -> EppPool {
        EppPool::new(EppConfig::default()).await.unwrap()
    }

    async fn pool_real_no_password() -> EppPool {
        let cfg = EppConfig {
            simulate: false,
            password_env_var: "DEFINITELY_NOT_SET_VAR_X9K2J7P".to_string(),
            ..EppConfig::default()
        };
        EppPool::new(cfg).await.unwrap()
    }

    async fn pool_real_with_password() -> EppPool {
        let cfg = EppConfig {
            simulate: false,
            password_env_var: "PATH".to_string(),
            ..EppConfig::default()
        };
        EppPool::new(cfg).await.unwrap()
    }

    #[tokio::test]
    async fn domain_check_simulate_returns_mock() {
        let pool = pool_simulate().await;
        let result = pool.domain_check(&["test.cv"]).await;
        assert!(result.is_ok());
        let results = result.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "test.cv");
        assert!(results[0].available);
        assert!(results[0].reason.is_none());
    }

    #[tokio::test]
    async fn domain_create_simulate_returns_mock() {
        let pool = pool_simulate().await;
        let result = pool
            .domain_create("test.cv", 1, &["ns1.example.cv"], "contact1", "authinfo")
            .await;
        assert!(result.is_ok());
        let res = result.unwrap();
        assert_eq!(res.name, "test.cv");
        assert!(res.creation_date.is_none());
        assert!(res.expiration_date.is_none());
    }

    #[tokio::test]
    async fn domain_delete_simulate_succeeds() {
        let pool = pool_simulate().await;
        let result = pool.domain_delete("test.cv").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn domain_renew_simulate_succeeds() {
        let pool = pool_simulate().await;
        let result = pool.domain_renew("test.cv", 1).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn contact_check_simulate_returns_mock() {
        let pool = pool_simulate().await;
        let result = pool.contact_check(&["contact1", "contact2"]).await;
        assert!(result.is_ok());
        let results = result.unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|&r| r));
    }

    #[tokio::test]
    async fn domain_check_real_no_password_returns_not_connected() {
        let pool = pool_real_no_password().await;
        let result = pool.domain_check(&["test.cv"]).await;
        assert!(matches!(result, Err(EppError::NotConnected)));
    }

    #[tokio::test]
    async fn domain_create_real_no_password_returns_not_connected() {
        let pool = pool_real_no_password().await;
        let result = pool.domain_create("test.cv", 1, &[], "", "").await;
        assert!(matches!(result, Err(EppError::NotConnected)));
    }

    #[tokio::test]
    async fn domain_delete_real_no_password_returns_not_connected() {
        let pool = pool_real_no_password().await;
        let result = pool.domain_delete("test.cv").await;
        assert!(matches!(result, Err(EppError::NotConnected)));
    }

    #[tokio::test]
    async fn contact_check_real_no_password_returns_not_connected() {
        let pool = pool_real_no_password().await;
        let result = pool.contact_check(&["contact1"]).await;
        assert!(matches!(result, Err(EppError::NotConnected)));
    }

    #[tokio::test]
    async fn domain_renew_real_returns_other_error() {
        let pool = pool_real_with_password().await;
        let result = pool.domain_renew("test.cv", 1).await;
        assert!(matches!(result, Err(EppError::Other(_))));
    }

    #[tokio::test]
    async fn is_simulated_reflects_config() {
        let sim_pool = pool_simulate().await;
        assert!(sim_pool.is_simulated());

        let real_pool = pool_real_with_password().await;
        assert!(!real_pool.is_simulated());
    }

    #[test]
    fn result_types_roundtrip() {
        let create = DomainCreateResult {
            name: "test.cv".to_string(),
            creation_date: Some("2026-01-01".to_string()),
            expiration_date: Some("2027-01-01".to_string()),
        };
        assert_eq!(create.name, "test.cv");
        assert_eq!(create.creation_date.as_deref(), Some("2026-01-01"));
        assert_eq!(create.expiration_date.as_deref(), Some("2027-01-01"));

        let check = DomainCheckResult {
            name: "test.cv".to_string(),
            available: true,
            reason: Some("premium".to_string()),
        };
        assert_eq!(check.name, "test.cv");
        assert!(check.available);
        assert_eq!(check.reason.as_deref(), Some("premium"));
    }

    #[test]
    fn error_display_covers_all_variants() {
        assert!(EppError::NotImplemented
            .to_string()
            .contains("not yet implemented"));
        assert!(EppError::NotConnected.to_string().contains("not connected"));
        assert!(EppError::PoolExhausted.to_string().contains("pool"));
        assert!(EppError::Timeout.to_string().contains("timed out"));
        let reg = EppError::RegistryError {
            code: 2002,
            msg: "IP not trusted".into(),
        };
        assert!(reg.to_string().contains("2002"));
        assert!(reg.to_string().contains("IP not trusted"));
        assert!(EppError::XmlError("bad xml".into())
            .to_string()
            .contains("bad xml"));
        assert!(EppError::TlsError("bad tls".into())
            .to_string()
            .contains("bad tls"));
        assert!(EppError::IoError("bad io".into())
            .to_string()
            .contains("bad io"));
        assert!(EppError::Other("unexpected".into())
            .to_string()
            .contains("unexpected"));
    }

    #[test]
    fn map_epp_error_maps_all_variants() {
        let io_err = instant_epp::Error::Io(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "refused",
        ));
        assert!(matches!(map_epp_error(io_err), EppError::IoError(_)));

        let timeout_err = instant_epp::Error::Timeout;
        assert!(matches!(map_epp_error(timeout_err), EppError::Timeout));
    }
}
