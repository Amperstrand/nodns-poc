//! Real Cloudflare integration test.
//!
//! `#[ignore]` by default — run with `cargo test --test cloudflare_integration -- --ignored`.
//! Requires `CF_API_TOKEN` (skipped if absent); `CF_ZONE_ID` defaults to dns4sats.xyz.
//!
//! Self-contained: `nodns-bot` is a binary crate with no `lib.rs`, so the
//! production `crate::cloudflare_backend::CloudflareBackend` is unreachable from
//! `tests/`. This file re-implements the same Cloudflare REST contract
//! (POST/PATCH/DELETE against `/zones/{id}/dns_records`, matching the
//! `DnsConnector` method signatures) to exercise the live create→update→delete
//! cycle against the real API.

use anyhow::{anyhow, Result};

const API_BASE: &str = "https://api.cloudflare.com/client/v4";
const DEFAULT_ZONE_ID: &str = "71009097e6f9ee0e65f4cd254f86e3f2";
const TEST_FQDN: &str = "_nodns-test.dns4sats.xyz";

struct CloudflareBackend {
    token: String,
    zone_id: String,
    client: reqwest::Client,
}

impl CloudflareBackend {
    fn new(token: String, zone_id: String) -> Self {
        Self {
            token,
            zone_id,
            client: reqwest::Client::new(),
        }
    }

    fn auth_bearer(&self) -> String {
        format!("Bearer {}", self.token)
    }

    async fn test_connection(&self) -> Result<()> {
        let url = format!("{API_BASE}/zones/{}", self.zone_id);
        let resp = self
            .client
            .get(&url)
            .header("Authorization", self.auth_bearer())
            .send()
            .await?;
        ensure_success(resp).await
    }

    async fn find_record_id(&self, fqdn: &str, rtype: &str) -> Result<Option<String>> {
        let url = format!("{API_BASE}/zones/{}/dns_records", self.zone_id);
        let resp = self
            .client
            .get(&url)
            .header("Authorization", self.auth_bearer())
            .query(&[("name", fqdn.trim_end_matches('.')), ("type", rtype)])
            .send()
            .await?;
        let body: serde_json::Value = resp.json().await?;
        let arr = body["result"]
            .as_array()
            .ok_or_else(|| anyhow!("Cloudflare list: unexpected response shape"))?;
        Ok(arr
            .first()
            .and_then(|r| r["id"].as_str())
            .map(str::to_owned))
    }

    async fn update_record(
        &self,
        fqdn: &str,
        ttl: u32,
        record_type: u16,
        rdata: &str,
    ) -> Result<()> {
        let rtype = record_type_str(record_type);
        let payload = serde_json::json!({
            "type": rtype,
            "name": fqdn.trim_end_matches('.'),
            "content": rdata,
            "ttl": if ttl == 1 { 1 } else { ttl.max(60) },
            "proxied": false,
        });
        match self.find_record_id(fqdn, rtype).await? {
            Some(id) => {
                let url = format!("{API_BASE}/zones/{}/dns_records/{id}", self.zone_id);
                let resp = self
                    .client
                    .request(reqwest::Method::PATCH, &url)
                    .header("Authorization", self.auth_bearer())
                    .json(&payload)
                    .send()
                    .await?;
                ensure_success(resp).await
            }
            None => {
                let url = format!("{API_BASE}/zones/{}/dns_records", self.zone_id);
                let resp = self
                    .client
                    .post(&url)
                    .header("Authorization", self.auth_bearer())
                    .json(&payload)
                    .send()
                    .await?;
                ensure_success(resp).await
            }
        }
    }

    async fn delete_record(&self, fqdn: &str, record_type: u16) -> Result<()> {
        let rtype = record_type_str(record_type);
        let Some(id) = self.find_record_id(fqdn, rtype).await? else {
            return Ok(());
        };
        let url = format!("{API_BASE}/zones/{}/dns_records/{id}", self.zone_id);
        let resp = self
            .client
            .delete(&url)
            .header("Authorization", self.auth_bearer())
            .send()
            .await?;
        ensure_success(resp).await
    }
}

async fn ensure_success(resp: reqwest::Response) -> Result<()> {
    let status = resp.status();
    let body: serde_json::Value = resp.json().await?;
    if status.is_success() {
        return Ok(());
    }
    let msg = body["errors"][0]["message"]
        .as_str()
        .unwrap_or("unknown error");
    Err(anyhow!("Cloudflare API {status}: {msg}"))
}

fn record_type_str(rt: u16) -> &'static str {
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

#[tokio::test]
#[ignore]
async fn cloudflare_create_update_delete_cycle() {
    let token = match std::env::var("CF_API_TOKEN") {
        Ok(t) => t,
        Err(_) => {
            println!("Skipping real Cloudflare test: CF_API_TOKEN not set");
            return;
        }
    };
    let zone_id = std::env::var("CF_ZONE_ID").unwrap_or_else(|_| DEFAULT_ZONE_ID.to_owned());

    let cf = CloudflareBackend::new(token, zone_id);

    let cycle = async {
        cf.test_connection().await?;
        let _ = cf.delete_record(TEST_FQDN, 1).await;
        cf.update_record(TEST_FQDN, 300, 1, "127.0.0.1").await?;
        cf.update_record(TEST_FQDN, 300, 1, "127.0.0.2").await?;
        Ok::<_, anyhow::Error>(())
    }
    .await;

    if let Err(e) = cf.delete_record(TEST_FQDN, 1).await {
        eprintln!("cleanup delete failed for {TEST_FQDN}: {e}");
    }

    cycle.expect("create→update cycle against real Cloudflare must succeed");
}
