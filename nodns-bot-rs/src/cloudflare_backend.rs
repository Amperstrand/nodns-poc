//! Cloudflare API DNS backend and the unified DnsBackend enum.
//!
//! Provides an alternative to RFC 2136 DDNS for zones managed via Cloudflare.
//! Record IDs are cached in memory to avoid repeated list calls.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::dns::{DnsError, Result, Updater};

const API_BASE: &str = "https://api.cloudflare.com/client/v4";
const MAX_RETRIES: u32 = 3;

pub struct CloudflareBackend {
    api_token: String,
    zone_id: String,
    client: reqwest::Client,
    record_ids: Arc<Mutex<HashMap<String, String>>>,
}

impl CloudflareBackend {
    pub fn new(api_token: String, zone_id: String) -> Self {
        Self {
            api_token,
            zone_id,
            client: reqwest::Client::new(),
            record_ids: Arc::new(Mutex::new(HashMap::new())),
        }
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

        let url = format!("{}/zones/{}/dns_records", API_BASE, self.zone_id);

        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_token))
            .query(&[("name", fqdn.trim_end_matches('.')), ("type", record_type)])
            .send()
            .await
            .map_err(|e| DnsError::Dns(format!("Cloudflare API request failed: {e}")))?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| DnsError::Dns(format!("Cloudflare API parse failed: {e}")))?;

        if !status.is_success() {
            let msg = body["errors"][0]["message"]
                .as_str()
                .unwrap_or("unknown error");
            return Err(DnsError::Dns(format!(
                "Cloudflare API error ({}): {}",
                status.as_u16(),
                msg
            )));
        }

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

            let resp = req
                .send()
                .await
                .map_err(|e| DnsError::Dns(format!("Cloudflare API request failed: {e}")))?;

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

            let body: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| DnsError::Dns(format!("Cloudflare API parse failed: {e}")))?;

            if !status.is_success() {
                if status.as_u16() == 429 {
                    return Err(DnsError::Dns(
                        "Cloudflare rate limit exceeded after retries".into(),
                    ));
                }
                let msg = body["errors"][0]["message"]
                    .as_str()
                    .unwrap_or("unknown error");
                return Err(DnsError::Dns(format!(
                    "Cloudflare API error ({}): {}",
                    status.as_u16(),
                    msg
                )));
            }

            return Ok(body);
        }
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
        let url = format!("{}/zones/{}", API_BASE, self.zone_id);

        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_token))
            .send()
            .await
            .map_err(|e| DnsError::Dns(format!("Cloudflare health check failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(DnsError::Dns(format!(
                "Cloudflare health check returned: {}",
                resp.status()
            )));
        }

        info!(zone_id = %self.zone_id, "Cloudflare connection test passed");
        Ok(())
    }
}

pub enum DnsBackend {
    Ddns(Updater),
    Cloudflare(CloudflareBackend),
}

impl DnsBackend {
    pub async fn update_record(
        &self,
        fqdn: &str,
        ttl: u32,
        record_type: u16,
        rdata: &str,
    ) -> Result<()> {
        let type_str = dns_record_type_str(record_type);
        match self {
            DnsBackend::Ddns(u) => u.update_record(fqdn, ttl, record_type, rdata).await,
            DnsBackend::Cloudflare(c) => c.upsert_record(fqdn, type_str, rdata, ttl).await,
        }
    }

    pub async fn update_txt_multi(&self, fqdn: &str, ttl: u32, segments: &[String]) -> Result<()> {
        match self {
            DnsBackend::Ddns(u) => u.update_txt_multi(fqdn, ttl, segments).await,
            DnsBackend::Cloudflare(c) => c.upsert_txt_multi(fqdn, segments, ttl).await,
        }
    }

    pub async fn delete_record(&self, fqdn: &str, record_type: u16) -> Result<()> {
        let type_str = dns_record_type_str(record_type);
        match self {
            DnsBackend::Ddns(u) => u.delete_record(fqdn, record_type).await,
            DnsBackend::Cloudflare(c) => c.delete_record(fqdn, type_str).await,
        }
    }

    pub async fn append_record(
        &self,
        fqdn: &str,
        ttl: u32,
        record_type: u16,
        rdata: &str,
    ) -> Result<()> {
        let type_str = dns_record_type_str(record_type);
        match self {
            DnsBackend::Ddns(u) => u.append_record(fqdn, ttl, record_type, rdata).await,
            DnsBackend::Cloudflare(c) => c.upsert_record(fqdn, type_str, rdata, ttl).await,
        }
    }

    pub async fn test_connection(&self) -> Result<()> {
        match self {
            DnsBackend::Ddns(u) => u.test_connection().await,
            DnsBackend::Cloudflare(c) => c.health_check().await,
        }
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
}
