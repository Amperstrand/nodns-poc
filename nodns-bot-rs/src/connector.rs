use anyhow::Result;

#[async_trait::async_trait]
pub trait DnsConnector: Send + Sync {
    async fn update_record(
        &self,
        fqdn: &str,
        ttl: u32,
        record_type: u16,
        rdata: &str,
    ) -> Result<()>;

    async fn update_txt_multi(&self, fqdn: &str, ttl: u32, segments: &[String]) -> Result<()>;

    async fn delete_record(&self, fqdn: &str, record_type: u16) -> Result<()>;

    async fn append_record(
        &self,
        fqdn: &str,
        ttl: u32,
        record_type: u16,
        rdata: &str,
    ) -> Result<()>;

    async fn test_connection(&self) -> Result<()>;
}
