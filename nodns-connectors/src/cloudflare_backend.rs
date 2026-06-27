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
    api_base: String,
    client: reqwest::Client,
    record_ids: Arc<Mutex<HashMap<String, String>>>,
    circuit: Arc<Mutex<CircuitBreaker>>,
}

impl CloudflareBackend {
    pub fn new(api_token: String, zone_id: String) -> Self {
        Self::new_with_base(api_token, zone_id, API_BASE.to_string())
    }

    /// Alternate constructor allowing a custom API base URL (used by tests to
    /// point at a mock server). Production callers should use [`new`].
    pub fn new_with_base(api_token: String, zone_id: String, api_base: String) -> Self {
        Self {
            api_token,
            zone_id,
            api_base,
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

        let url = format!("{}/zones/{}/dns_records", self.api_base, self.zone_id);

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
        let url = format!("{}/zones/{}/dns_records", self.api_base, self.zone_id);

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
            self.api_base, self.zone_id, record_id
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
            self.api_base, self.zone_id, record_id
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

        let url = format!("{}/zones/{}", self.api_base, self.zone_id);

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

    #[cfg(test)]
    mod mock {
        use super::*;
        use serde_json::json;
        use wiremock::matchers::{body_partial_json, header, method, path, query_param};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        const TOKEN: &str = "test-token";
        const ZONE_ID: &str = "zone456";
        const FQDN: &str = "foo.example.com";

        fn backend(server_uri: String) -> CloudflareBackend {
            CloudflareBackend::new_with_base(TOKEN.into(), ZONE_ID.into(), server_uri)
        }

        fn dns_records_path() -> String {
            format!("/zones/{ZONE_ID}/dns_records")
        }

        fn record_path(id: &str) -> String {
            format!("/zones/{ZONE_ID}/dns_records/{id}")
        }

        fn zone_path() -> String {
            format!("/zones/{ZONE_ID}")
        }

        fn auth_header() -> String {
            format!("Bearer {TOKEN}")
        }

        fn empty_result() -> serde_json::Value {
            json!({ "success": true, "errors": [], "result": [] })
        }

        async fn mount_get_empty(server: &MockServer) {
            Mock::given(method("GET"))
                .and(path(dns_records_path()))
                .respond_with(ResponseTemplate::new(200).set_body_json(empty_result()))
                .expect(1)
                .mount(server)
                .await;
        }

        #[tokio::test]
        async fn upsert_record_creates_new_record_via_post() {
            let server = MockServer::start().await;
            let backend = backend(server.uri());

            mount_get_empty(&server).await;

            Mock::given(method("POST"))
                .and(path(dns_records_path()))
                .and(body_partial_json(json!({
                    "type": "A",
                    "name": FQDN,
                    "content": "1.2.3.4",
                    "ttl": 300,
                    "proxied": false,
                })))
                .and(header("authorization", auth_header()))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(json!({ "success": true, "result": { "id": "rec-new-1" } })),
                )
                .expect(1)
                .mount(&server)
                .await;

            let result = backend.upsert_record(FQDN, "A", "1.2.3.4", 300).await;
            assert!(result.is_ok(), "upsert should succeed: {:?}", result.err());

            let cache = backend.record_ids.lock().await;
            assert_eq!(
                cache.get("foo.example.com|A"),
                Some(&"rec-new-1".to_string()),
                "record id should be cached after create"
            );
        }

        #[tokio::test]
        async fn upsert_record_updates_existing_via_patch() {
            let server = MockServer::start().await;
            let backend = backend(server.uri());

            Mock::given(method("GET"))
                .and(path(dns_records_path()))
                .and(query_param("name", FQDN))
                .and(query_param("type", "A"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "success": true,
                    "result": [{ "id": "rec-existing", "type": "A", "name": FQDN }]
                })))
                .expect(1)
                .mount(&server)
                .await;

            Mock::given(method("PATCH"))
                .and(path(record_path("rec-existing")))
                .and(body_partial_json(json!({
                    "type": "A",
                    "name": FQDN,
                    "content": "5.6.7.8",
                    "proxied": false,
                })))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "success": true,
                    "result": { "id": "rec-existing", "content": "5.6.7.8" }
                })))
                .expect(1)
                .mount(&server)
                .await;

            Mock::given(method("POST"))
                .and(path(dns_records_path()))
                .respond_with(ResponseTemplate::new(500))
                .expect(0)
                .mount(&server)
                .await;

            let result = backend.upsert_record(FQDN, "A", "5.6.7.8", 300).await;
            assert!(
                result.is_ok(),
                "upsert update should succeed: {:?}",
                result.err()
            );

            let cache = backend.record_ids.lock().await;
            assert_eq!(
                cache.get("foo.example.com|A"),
                Some(&"rec-existing".to_string()),
                "record id should be cached after lookup"
            );
        }

        #[tokio::test]
        async fn upsert_record_returns_error_on_500() {
            let server = MockServer::start().await;
            let backend = backend(server.uri());

            mount_get_empty(&server).await;

            Mock::given(method("POST"))
                .and(path(dns_records_path()))
                .respond_with(ResponseTemplate::new(500).set_body_json(json!({
                    "success": false,
                    "errors": [{ "code": 1000, "message": "internal server error" }]
                })))
                .expect(1)
                .mount(&server)
                .await;

            let result = backend.upsert_record(FQDN, "A", "1.2.3.4", 300).await;
            let err = result.expect_err("should error on 500");
            let msg = err.to_string().to_lowercase();
            assert!(
                msg.contains("500"),
                "error should mention status 500: {msg}"
            );
            assert!(
                msg.contains("internal server error"),
                "error should include upstream message: {msg}"
            );
        }

        #[tokio::test]
        async fn upsert_record_returns_rate_limit_error_on_429() {
            let server = MockServer::start().await;
            let backend = backend(server.uri());

            mount_get_empty(&server).await;

            // send_with_retry retries up to MAX_RETRIES then surfaces a rate-limit error.
            Mock::given(method("POST"))
                .and(path(dns_records_path()))
                .respond_with(ResponseTemplate::new(429).set_body_json(json!({
                    "success": false,
                    "errors": [{ "code": 1003, "message": "rate limited" }]
                })))
                .expect((1 + MAX_RETRIES) as u64)
                .mount(&server)
                .await;

            let result = backend.upsert_record(FQDN, "A", "1.2.3.4", 300).await;
            let err = result.expect_err("should error on persistent 429");
            let msg = err.to_string().to_lowercase();
            assert!(
                msg.contains("rate limit"),
                "error should mention rate limit: {msg}"
            );
        }

        #[tokio::test]
        async fn delete_record_succeeds_when_record_exists() {
            let server = MockServer::start().await;
            let backend = backend(server.uri());

            Mock::given(method("GET"))
                .and(path(dns_records_path()))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "success": true,
                    "result": [{ "id": "rec-del", "type": "TXT", "name": FQDN }]
                })))
                .expect(1)
                .mount(&server)
                .await;

            Mock::given(method("DELETE"))
                .and(path(record_path("rec-del")))
                .and(header("authorization", auth_header()))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "success": true,
                    "result": { "id": "rec-del" }
                })))
                .expect(1)
                .mount(&server)
                .await;

            let result = backend.delete_record(FQDN, "TXT").await;
            assert!(result.is_ok(), "delete should succeed: {:?}", result.err());

            let cache = backend.record_ids.lock().await;
            assert!(
                !cache.contains_key("foo.example.com|TXT"),
                "record id should be evicted from cache after delete"
            );
        }

        #[tokio::test]
        async fn delete_record_is_noop_when_not_found() {
            let server = MockServer::start().await;
            let backend = backend(server.uri());

            mount_get_empty(&server).await;

            Mock::given(method("DELETE"))
                .and(path(dns_records_path()))
                .respond_with(ResponseTemplate::new(500))
                .expect(0)
                .mount(&server)
                .await;

            let result = backend.delete_record(FQDN, "A").await;
            assert!(
                result.is_ok(),
                "delete of missing record should succeed (noop): {:?}",
                result.err()
            );
        }

        #[tokio::test]
        async fn upsert_txt_multi_joins_segments_into_single_txt() {
            let server = MockServer::start().await;
            let backend = backend(server.uri());

            mount_get_empty(&server).await;

            Mock::given(method("POST"))
                .and(path(dns_records_path()))
                .and(body_partial_json(json!({
                    "type": "TXT",
                    "name": FQDN,
                    "content": "part-onepart-two",
                })))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(json!({ "success": true, "result": { "id": "txt-1" } })),
                )
                .expect(1)
                .mount(&server)
                .await;

            let segments = vec!["part-one".to_string(), "part-two".to_string()];
            let result = backend.upsert_txt_multi(FQDN, &segments, 300).await;
            assert!(
                result.is_ok(),
                "upsert_txt_multi should succeed: {:?}",
                result.err()
            );
        }

        #[tokio::test]
        async fn cache_skips_lookup_and_uses_patch_on_second_upsert() {
            let server = MockServer::start().await;
            let backend = backend(server.uri());

            // First upsert: GET (empty) + POST (create) — caches the id.
            Mock::given(method("GET"))
                .and(path(dns_records_path()))
                .respond_with(ResponseTemplate::new(200).set_body_json(empty_result()))
                .expect(1)
                .mount(&server)
                .await;

            Mock::given(method("POST"))
                .and(path(dns_records_path()))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(json!({ "success": true, "result": { "id": "cached-1" } })),
                )
                .expect(1)
                .mount(&server)
                .await;

            // Second upsert: cache hit → no GET, only PATCH against cached id.
            Mock::given(method("PATCH"))
                .and(path(record_path("cached-1")))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "success": true,
                    "result": { "id": "cached-1" }
                })))
                .expect(1)
                .mount(&server)
                .await;

            backend
                .upsert_record(FQDN, "A", "1.1.1.1", 300)
                .await
                .unwrap();
            backend
                .upsert_record(FQDN, "A", "2.2.2.2", 300)
                .await
                .unwrap();

            let cache = backend.record_ids.lock().await;
            assert_eq!(
                cache.get("foo.example.com|A"),
                Some(&"cached-1".to_string()),
                "cached id should remain after update"
            );
        }

        #[tokio::test]
        async fn health_check_passes_on_success() {
            let server = MockServer::start().await;
            let backend = backend(server.uri());

            Mock::given(method("GET"))
                .and(path(zone_path()))
                .and(header("authorization", auth_header()))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "success": true,
                    "result": { "id": ZONE_ID, "name": "example.com" }
                })))
                .expect(1)
                .mount(&server)
                .await;

            let result = backend.health_check().await;
            assert!(
                result.is_ok(),
                "health_check should succeed: {:?}",
                result.err()
            );
        }

        #[tokio::test]
        async fn health_check_fails_on_non_2xx() {
            let server = MockServer::start().await;
            let backend = backend(server.uri());

            Mock::given(method("GET"))
                .and(path(zone_path()))
                .respond_with(ResponseTemplate::new(403).set_body_json(json!({
                    "success": false,
                    "errors": [{ "code": 9109, "message": "Unauthorized to access zone resource" }]
                })))
                .expect(1)
                .mount(&server)
                .await;

            let result = backend.health_check().await;
            let err = result.expect_err("health_check should fail on 403");
            let msg = err.to_string().to_lowercase();
            assert!(
                msg.contains("403"),
                "error should mention the failing status: {msg}"
            );
        }
    }
}
