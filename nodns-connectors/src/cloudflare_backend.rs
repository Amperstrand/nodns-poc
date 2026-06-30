//! Cloudflare API DNS backend.
//!
//! Provides an alternative to RFC 2136 DDNS for zones managed via Cloudflare.
//! Record IDs are cached in memory to avoid repeated list calls.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::{debug, info, warn};

const API_BASE: &str = "https://api.cloudflare.com/client/v4";
const MAX_RETRIES: u32 = 3;

#[derive(Debug, thiserror::Error)]
pub enum CloudflareError {
    #[error("Cloudflare API error ({status}): {message}")]
    Api { status: u16, message: String },
    #[error("Cloudflare rate limit exceeded after retries")]
    RateLimited,
    #[error("Cloudflare circuit breaker is open — API calls suspended")]
    CircuitOpen,
    #[error("Cloudflare API request failed: {0}")]
    Request(String),
    #[error("Cloudflare API response parse failed: {0}")]
    Parse(String),
    #[error("Cloudflare API: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, CloudflareError>;

#[derive(Debug, Clone)]
pub struct CloudflareRecord {
    pub id: String,
    pub name: String,
    pub rtype: String,
    pub content: String,
    pub ttl: u32,
}

pub struct CloudflareBackend {
    api_token: String,
    zone_id: String,
    api_base: String,
    client: reqwest::Client,
    record_ids: Arc<Mutex<HashMap<String, String>>>,
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
        }
    }

    fn cache_key(fqdn: &str, record_type: &str) -> String {
        format!(
            "{}|{}",
            fqdn.trim_end_matches('.').to_lowercase(),
            record_type.to_uppercase()
        )
    }

    pub async fn find_record_id(&self, fqdn: &str, record_type: &str) -> Result<Option<String>> {
        let key = Self::cache_key(fqdn, record_type);
        {
            let cache = self.record_ids.lock().await;
            if let Some(id) = cache.get(&key) {
                return Ok(Some(id.clone()));
            }
        }

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
                return Err(CloudflareError::Request(e.to_string()));
            }
        };

        let status = resp.status();
        let body: serde_json::Value = match resp.json().await {
            Ok(b) => b,
            Err(e) => {
                return Err(CloudflareError::Parse(e.to_string()));
            }
        };

        if !status.is_success() {
            let msg = body["errors"][0]["message"]
                .as_str()
                .unwrap_or("unknown error");
            return Err(CloudflareError::Api {
                status: status.as_u16(),
                message: msg.into(),
            });
        }

        let result_array = body["result"]
            .as_array()
            .ok_or_else(|| CloudflareError::Other("unexpected response shape".into()))?;

        if result_array.is_empty() {
            return Ok(None);
        }

        let id = result_array[0]["id"]
            .as_str()
            .ok_or_else(|| CloudflareError::Other("missing record id".into()))?
            .to_string();

        let mut cache = self.record_ids.lock().await;
        cache.insert(key, id.clone());

        Ok(Some(id))
    }

    pub async fn create_record(
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
            .ok_or_else(|| CloudflareError::Other("missing id in create response".into()))?
            .to_string();

        let key = Self::cache_key(fqdn, record_type);
        let mut cache = self.record_ids.lock().await;
        cache.insert(key, id);

        info!(fqdn = %fqdn, rtype = %record_type, "Cloudflare record created");
        Ok(())
    }

    pub async fn patch_record(
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

    pub async fn delete_record_by_id(&self, record_id: &str, fqdn: &str) -> Result<()> {
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
                    return Err(CloudflareError::Request(e.to_string()));
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
                    return Err(CloudflareError::Parse(e.to_string()));
                }
            };

            if !status.is_success() {
                if status.as_u16() == 429 {
                    return Err(CloudflareError::RateLimited);
                }
                let msg = body["errors"][0]["message"]
                    .as_str()
                    .unwrap_or("unknown error");
                return Err(CloudflareError::Api {
                    status: status.as_u16(),
                    message: msg.into(),
                });
            }

            return Ok(body);
        }
    }

    pub async fn list_records(&self, name_prefix: &str) -> Result<Vec<CloudflareRecord>> {
        let url = format!("{}/zones/{}/dns_records", self.api_base, self.zone_id);
        let body = self
            .send_with_retry(&url, reqwest::Method::GET, None)
            .await?;

        let result_array = body["result"]
            .as_array()
            .ok_or_else(|| CloudflareError::Other("unexpected response shape".into()))?;

        let prefix_lower = name_prefix.trim_end_matches('.').to_lowercase();
        let records = result_array
            .iter()
            .filter_map(|r| {
                let name = r["name"].as_str()?.to_string();
                if !name.to_lowercase().ends_with(&prefix_lower) {
                    return None;
                }
                Some(CloudflareRecord {
                    id: r["id"].as_str()?.to_string(),
                    name,
                    rtype: r["type"].as_str()?.to_string(),
                    content: r["content"].as_str()?.to_string(),
                    ttl: r["ttl"].as_u64()? as u32,
                })
            })
            .collect();

        Ok(records)
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
                return Err(CloudflareError::Request(format!(
                    "health check failed: {e}"
                )));
            }
        };

        if !resp.status().is_success() {
            return Err(CloudflareError::Api {
                status: resp.status().as_u16(),
                message: format!("health check returned {}", resp.status()),
            });
        }

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

        #[tokio::test]
        async fn list_records_returns_filtered_records() {
            let server = MockServer::start().await;
            let backend = backend(server.uri());

            Mock::given(method("GET"))
                .and(path(dns_records_path()))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "success": true,
                    "result": [
                        { "id": "rec-1", "name": "npub1abc.nostr", "type": "A", "content": "1.2.3.4", "ttl": 3600 },
                        { "id": "rec-2", "name": "www.npub1abc.nostr", "type": "CNAME", "content": "example.com", "ttl": 3600 },
                        { "id": "rec-3", "name": "npub1xyz.nostr", "type": "A", "content": "5.6.7.8", "ttl": 3600 },
                        { "id": "rec-4", "name": "other.example.com", "type": "TXT", "content": "hello", "ttl": 3600 }
                    ]
                })))
                .expect(1)
                .mount(&server)
                .await;

            let records = backend
                .list_records("npub1abc.nostr")
                .await
                .expect("list_records should succeed");

            assert_eq!(
                records.len(),
                2,
                "should return only records ending with npub1abc.nostr"
            );
            assert_eq!(records[0].id, "rec-1");
            assert_eq!(records[0].name, "npub1abc.nostr");
            assert_eq!(records[0].rtype, "A");
            assert_eq!(records[1].id, "rec-2");
            assert_eq!(records[1].name, "www.npub1abc.nostr");
            assert_eq!(records[1].rtype, "CNAME");
        }
    }
}
