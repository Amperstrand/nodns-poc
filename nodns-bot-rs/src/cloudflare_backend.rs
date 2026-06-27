//! Cloudflare API DNS backend.
//!
//! Provides an alternative to RFC 2136 DDNS for zones managed via Cloudflare.
//! Record IDs are cached in memory to avoid repeated list calls.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::dns::{DnsError, Result};

const API_BASE: &str = "https://api.cloudflare.com/client/v4";
const MAX_RETRIES: u32 = 3;
const CIRCUIT_FAILURE_THRESHOLD: u32 = 3;
const CIRCUIT_COOLDOWN: Duration = Duration::from_secs(5 * 60);

struct CircuitBreaker {
    consecutive_failures: u32,
    open_until: Option<Instant>,
}

impl CircuitBreaker {
    const fn new() -> Self {
        Self {
            consecutive_failures: 0,
            open_until: None,
        }
    }

    fn is_open(&self) -> bool {
        matches!(self.open_until, Some(deadline) if deadline > Instant::now())
    }

    fn record_success(&mut self) {
        if self.consecutive_failures > 0 || self.open_until.is_some() {
            info!("Cloudflare circuit breaker closed — recovered after success");
        }
        self.consecutive_failures = 0;
        self.open_until = None;
    }

    fn record_failure(&mut self) {
        self.consecutive_failures += 1;
        if self.consecutive_failures >= CIRCUIT_FAILURE_THRESHOLD && !self.is_open() {
            self.open_until = Some(Instant::now() + CIRCUIT_COOLDOWN);
            warn!(
                failures = self.consecutive_failures,
                cooldown_secs = CIRCUIT_COOLDOWN.as_secs(),
                "Cloudflare circuit breaker OPENED — API calls suspended for 5 minutes"
            );
        }
    }
}

pub struct CloudflareBackend {
    api_token: String,
    zone_id: String,
    client: reqwest::Client,
    record_ids: Arc<Mutex<HashMap<String, String>>>,
    circuit: Arc<Mutex<CircuitBreaker>>,
}

impl CloudflareBackend {
    pub fn new(api_token: String, zone_id: String) -> Self {
        Self {
            api_token,
            zone_id,
            client: reqwest::Client::new(),
            record_ids: Arc::new(Mutex::new(HashMap::new())),
            circuit: Arc::new(Mutex::new(CircuitBreaker::new())),
        }
    }

    async fn check_circuit(&self) -> Result<()> {
        let cb = self.circuit.lock().await;
        if cb.is_open() {
            return Err(DnsError::Dns(
                "Cloudflare circuit breaker is open — API calls suspended".into(),
            ));
        }
        Ok(())
    }

    fn cache_key(fqdn: &str, record_type: &str) -> String {
        format!(
            "{}|{}",
            fqdn.trim_end_matches('.').to_lowercase(),
            record_type.to_uppercase()
        )
    }

    async fn find_record_id(&self, fqdn: &str, record_type: &str) -> Result<Option<String>> {
        let key = Self::cache_key(fqdn, record_type);
        {
            let cache = self.record_ids.lock().await;
            if let Some(id) = cache.get(&key) {
                return Ok(Some(id.clone()));
            }
        }

        self.check_circuit().await?;

        let url = format!("{}/zones/{}/dns_records", API_BASE, self.zone_id);

        let resp = match self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_token))
            .query(&[("name", fqdn.trim_end_matches('.')), ("type", record_type)])
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                self.record_circuit_failure().await;
                return Err(DnsError::Dns(format!("Cloudflare API request failed: {e}")));
            }
        };

        let status = resp.status();
        let body: serde_json::Value = match resp.json().await {
            Ok(b) => b,
            Err(e) => {
                self.record_circuit_failure().await;
                return Err(DnsError::Dns(format!("Cloudflare API parse failed: {e}")));
            }
        };

        if !status.is_success() {
            let msg = body["errors"][0]["message"]
                .as_str()
                .unwrap_or("unknown error");
            self.record_circuit_failure().await;
            return Err(DnsError::Dns(format!(
                "Cloudflare API error ({}): {}",
                status.as_u16(),
                msg
            )));
        }

        self.record_circuit_success().await;

        let result_array = body["result"]
            .as_array()
            .ok_or_else(|| DnsError::Dns("Cloudflare API: unexpected response shape".into()))?;

        if result_array.is_empty() {
            return Ok(None);
        }

        let id = result_array[0]["id"]
            .as_str()
            .ok_or_else(|| DnsError::Dns("Cloudflare API: missing record id".into()))?
            .to_string();

        let mut cache = self.record_ids.lock().await;
        cache.insert(key, id.clone());

        Ok(Some(id))
    }

    async fn create_record(
        &self,
        fqdn: &str,
        record_type: &str,
        content: &str,
        ttl: u32,
    ) -> Result<()> {
        let url = format!("{}/zones/{}/dns_records", API_BASE, self.zone_id);

        let payload = serde_json::json!({
            "type": record_type,
            "name": fqdn.trim_end_matches('.'),
            "content": content,
            "ttl": if ttl == 1 { 1 } else { ttl.max(60) },
            "proxied": false,
        });

        let resp = self
            .send_with_retry(&url, reqwest::Method::POST, Some(&payload))
            .await?;

        let id = resp["result"]["id"]
            .as_str()
            .ok_or_else(|| DnsError::Dns("Cloudflare API: missing id in create response".into()))?
            .to_string();

        let key = Self::cache_key(fqdn, record_type);
        let mut cache = self.record_ids.lock().await;
        cache.insert(key, id);

        info!(fqdn = %fqdn, rtype = %record_type, "Cloudflare record created");
        Ok(())
    }

    async fn patch_record(
        &self,
        record_id: &str,
        fqdn: &str,
        record_type: &str,
        content: &str,
        ttl: u32,
    ) -> Result<()> {
        let url = format!(
            "{}/zones/{}/dns_records/{}",
            API_BASE, self.zone_id, record_id
        );

        let payload = serde_json::json!({
            "type": record_type,
            "name": fqdn.trim_end_matches('.'),
            "content": content,
            "ttl": if ttl == 1 { 1 } else { ttl.max(60) },
            "proxied": false,
        });

        self.send_with_retry(&url, reqwest::Method::PATCH, Some(&payload))
            .await?;

        info!(fqdn = %fqdn, rtype = %record_type, "Cloudflare record updated");
        Ok(())
    }

    async fn delete_record_by_id(&self, record_id: &str, fqdn: &str) -> Result<()> {
        let url = format!(
            "{}/zones/{}/dns_records/{}",
            API_BASE, self.zone_id, record_id
        );

        self.send_with_retry(&url, reqwest::Method::DELETE, None)
            .await?;

        let mut cache = self.record_ids.lock().await;
        let key_pattern = fqdn.trim_end_matches('.').to_lowercase();
        cache.retain(|k, _| !k.starts_with(&format!("{}|", key_pattern)));

        info!(fqdn = %fqdn, "Cloudflare record deleted");
        Ok(())
    }

    async fn send_with_retry(
        &self,
        url: &str,
        method: reqwest::Method,
        payload: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value> {
        self.check_circuit().await?;

        let mut attempt = 0u32;
        loop {
            attempt += 1;

            let mut req = self
                .client
                .request(method.clone(), url)
                .header("Authorization", format!("Bearer {}", self.api_token))
                .header("Content-Type", "application/json");

            if let Some(p) = payload {
                req = req.json(p);
            }

            let resp = match req.send().await {
                Ok(r) => r,
                Err(e) => {
                    self.record_circuit_failure().await;
                    return Err(DnsError::Dns(format!("Cloudflare API request failed: {e}")));
                }
            };

            let status = resp.status();

            if status.as_u16() == 429 && attempt <= MAX_RETRIES {
                let wait = std::time::Duration::from_millis(500 * 2u64.pow(attempt - 1));
                warn!(
                    attempt,
                    wait_ms = wait.as_millis(),
                    "Cloudflare rate limited (429), backing off"
                );
                tokio::time::sleep(wait).await;
                continue;
            }

            let body: serde_json::Value = match resp.json().await {
                Ok(b) => b,
                Err(e) => {
                    self.record_circuit_failure().await;
                    return Err(DnsError::Dns(format!("Cloudflare API parse failed: {e}")));
                }
            };

            if !status.is_success() {
                if status.as_u16() == 429 {
                    self.record_circuit_failure().await;
                    return Err(DnsError::Dns(
                        "Cloudflare rate limit exceeded after retries".into(),
                    ));
                }
                let msg = body["errors"][0]["message"]
                    .as_str()
                    .unwrap_or("unknown error");
                self.record_circuit_failure().await;
                return Err(DnsError::Dns(format!(
                    "Cloudflare API error ({}): {}",
                    status.as_u16(),
                    msg
                )));
            }

            self.record_circuit_success().await;
            return Ok(body);
        }
    }

    async fn record_circuit_success(&self) {
        let mut cb = self.circuit.lock().await;
        cb.record_success();
    }

    async fn record_circuit_failure(&self) {
        let mut cb = self.circuit.lock().await;
        cb.record_failure();
    }

    pub async fn upsert_record(
        &self,
        fqdn: &str,
        record_type: &str,
        rdata: &str,
        ttl: u32,
    ) -> Result<()> {
        debug!(fqdn = %fqdn, rtype = %record_type, ttl, "Cloudflare upsert");

        match self.find_record_id(fqdn, record_type).await? {
            Some(id) => self.patch_record(&id, fqdn, record_type, rdata, ttl).await,
            None => self.create_record(fqdn, record_type, rdata, ttl).await,
        }
    }

    pub async fn delete_record(&self, fqdn: &str, record_type: &str) -> Result<()> {
        debug!(fqdn = %fqdn, rtype = %record_type, "Cloudflare delete");

        let id = match self.find_record_id(fqdn, record_type).await? {
            Some(id) => id,
            None => {
                info!(fqdn = %fqdn, rtype = %record_type, "Cloudflare delete: record not found (noop)");
                return Ok(());
            }
        };

        self.delete_record_by_id(&id, fqdn).await
    }

    pub async fn upsert_txt_multi(&self, fqdn: &str, segments: &[String], ttl: u32) -> Result<()> {
        let content = segments.join("");
        self.upsert_record(fqdn, "TXT", &content, ttl).await
    }

    pub async fn health_check(&self) -> Result<()> {
        self.check_circuit().await?;

        let url = format!("{}/zones/{}", API_BASE, self.zone_id);

        let resp = match self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_token))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                self.record_circuit_failure().await;
                return Err(DnsError::Dns(format!(
                    "Cloudflare health check failed: {e}"
                )));
            }
        };

        if !resp.status().is_success() {
            self.record_circuit_failure().await;
            return Err(DnsError::Dns(format!(
                "Cloudflare health check returned: {}",
                resp.status()
            )));
        }

        self.record_circuit_success().await;
        info!(zone_id = %self.zone_id, "Cloudflare connection test passed");
        Ok(())
    }
}

#[async_trait::async_trait]
impl crate::connector::DnsConnector for CloudflareBackend {
    async fn update_record(
        &self,
        fqdn: &str,
        ttl: u32,
        record_type: u16,
        rdata: &str,
    ) -> anyhow::Result<()> {
        let type_str = dns_record_type_str(record_type);
        CloudflareBackend::upsert_record(self, fqdn, type_str, rdata, ttl).await?;
        Ok(())
    }

    async fn update_txt_multi(
        &self,
        fqdn: &str,
        ttl: u32,
        segments: &[String],
    ) -> anyhow::Result<()> {
        CloudflareBackend::upsert_txt_multi(self, fqdn, segments, ttl).await?;
        Ok(())
    }

    async fn delete_record(&self, fqdn: &str, record_type: u16) -> anyhow::Result<()> {
        let type_str = dns_record_type_str(record_type);
        CloudflareBackend::delete_record(self, fqdn, type_str).await?;
        Ok(())
    }

    async fn append_record(
        &self,
        fqdn: &str,
        ttl: u32,
        record_type: u16,
        rdata: &str,
    ) -> anyhow::Result<()> {
        let type_str = dns_record_type_str(record_type);
        CloudflareBackend::upsert_record(self, fqdn, type_str, rdata, ttl).await?;
        Ok(())
    }

    async fn test_connection(&self) -> anyhow::Result<()> {
        CloudflareBackend::health_check(self).await?;
        Ok(())
    }
}

fn dns_record_type_str(rt: u16) -> &'static str {
    match rt {
        1 => "A",
        2 => "NS",
        5 => "CNAME",
        12 => "PTR",
        15 => "MX",
        16 => "TXT",
        28 => "AAAA",
        33 => "SRV",
        _ => "UNKNOWN",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_is_case_insensitive_on_fqdn() {
        let key = CloudflareBackend::cache_key("FOO.example.com.", "a");
        assert_eq!(key, "foo.example.com|A");
    }

    #[test]
    fn cache_key_strips_trailing_dot() {
        let key = CloudflareBackend::cache_key("foo.example.com.", "TXT");
        assert_eq!(key, "foo.example.com|TXT");
    }

    #[test]
    fn dns_record_type_str_known_types() {
        assert_eq!(dns_record_type_str(1), "A");
        assert_eq!(dns_record_type_str(28), "AAAA");
        assert_eq!(dns_record_type_str(5), "CNAME");
        assert_eq!(dns_record_type_str(16), "TXT");
        assert_eq!(dns_record_type_str(15), "MX");
        assert_eq!(dns_record_type_str(33), "SRV");
    }

    #[test]
    fn dns_record_type_str_unknown() {
        assert_eq!(dns_record_type_str(99), "UNKNOWN");
    }

    #[test]
    fn cloudflare_backend_construction() {
        let backend = CloudflareBackend::new("token123".into(), "zone456".into());
        assert_eq!(backend.api_token, "token123");
        assert_eq!(backend.zone_id, "zone456");
    }

    #[test]
    fn circuit_breaker_starts_closed() {
        let cb = CircuitBreaker::new();
        assert!(!cb.is_open());
    }

    #[test]
    fn circuit_breaker_opens_after_threshold() {
        let mut cb = CircuitBreaker::new();
        for _ in 0..CIRCUIT_FAILURE_THRESHOLD {
            cb.record_failure();
        }
        assert!(cb.is_open());
    }

    #[test]
    fn circuit_breaker_resets_on_success() {
        let mut cb = CircuitBreaker::new();
        cb.record_failure();
        cb.record_failure();
        cb.record_success();
        assert!(!cb.is_open());
        assert_eq!(cb.consecutive_failures, 0);
    }

    #[tokio::test]
    async fn circuit_breaker_suspends_api_calls() {
        let backend = CloudflareBackend::new("token".into(), "zone".into());
        for _ in 0..CIRCUIT_FAILURE_THRESHOLD {
            backend.record_circuit_failure().await;
        }
        let result = backend.check_circuit().await;
        assert!(result.is_err());
    }
}
