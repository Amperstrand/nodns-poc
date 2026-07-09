# 40 — Bridge Agent Architecture: Pluggable DNS Backends

> **Status**: DRAFT. Design for generalizing nodns-bot-rs into a distributed bridge agent that any zone operator can deploy.

## Overview

The current `nodns-bot-rs` is a single-purpose bot: it subscribes to relay.cashu.email, validates kind 11111 events, and pushes TSIG-signed DDNS updates to a local Knot DNS instance. This design generalizes it into a **distributed bridge agent** that any zone operator can deploy with their preferred DNS backend.

## Architecture

```
Nostr relay (relay.cashu.email)
    │
    │ kind 11111 events
    ▼
┌──────────────────────────────────┐
│  Bridge Agent (Rust daemon)      │
│                                  │
│  ┌─────────────────────────────┐ │
│  │  Event Pipeline             │ │
│  │  1. Parse (parser.rs)       │ │
│  │  2. Auth check (auth.rs)    │ │
│  │  3. Payment verify          │ │
│  │  4. Zone check (TXT match)  │
│  │  5. Dispatch to backend ────┼─┼──▶ Backend trait
│  └─────────────────────────────┘ │
│                                  │
│  ┌─────────────────────────────┐ │
│  │  DNS Backend (trait)        │ │
│  │  ├─ DdnsBackend      (2136) │ │  ← current (Knot DNS)
│  │  ├─ CloudflareBackend (API) │ │  ← new
│  │  ├─ EppBackend       (EPP)  │ │  ← .cv pilot
│  │  └─ CustomBackend    (user) │ │  ← extensible
│  └─────────────────────────────┘ │
│                                  │
│  ┌─────────────────────────────┐ │
│  │  State Store (SQLite)       │ │
│  │  events, meta, delegations  │ │
│  └─────────────────────────────┘ │
└──────────────────────────────────┘
```

## DNS Backend Trait

```rust
#[async_trait::async_trait]
pub trait DnsBackend: Send + Sync {
    /// Add or replace a DNS record
    async fn upsert_record(
        &self,
        zone: &str,
        name: &str,
        record_type: &str,
        rdata: &str,
        ttl: u32,
    ) -> Result<(), BackendError>;

    /// Delete a DNS record
    async fn delete_record(
        &self,
        zone: &str,
        name: &str,
        record_type: &str,
    ) -> Result<(), BackendError>;

    /// Check if the backend is healthy and configured
    async fn health_check(&self) -> Result<bool, BackendError>;

    /// Backend name for logging
    fn name(&self) -> &str;
}
```

## Backend Implementations

### 1. RFC 2136 DDNS Backend (current — `DdnsBackend`)

The existing `dns.rs` logic wrapped in the trait. Uses hickory-proto to construct and send TSIG-signed DDNS UPDATE messages.

**Config:**
```toml
[[zones]]
zone = "nodns.shop"
backend = "ddns"

[zones.backend_config]
knot_address = "127.0.0.1:53"
tsig_key_name = "nodns-bot"
tsig_key_secret = "base64-secret"
tsig_algorithm = "hmac-sha256"
```

**Already working** — this is the current production setup.

### 2. Cloudflare API Backend (`CloudflareBackend`)

Uses Cloudflare's REST API to manage DNS records. Zone operators who already use Cloudflare just provide an API token scoped to their zone.

**Config:**
```toml
[[zones]]
zone = "mycooldns.com"
backend = "cloudflare"

[zones.backend_config]
api_token = "cf-token-here"        # scoped to zone DNS edit
zone_id = "abc123..."              # Cloudflare zone ID
# Optional: proxy = true for orange-cloud records
```

**Implementation:**
- `upsert_record` → `POST /zones/{id}/dns_records` (or PATCH if exists)
- `delete_record` → `DELETE /zones/{id}/dns_records/{record_id}`
- `health_check` → `GET /zones/{id}`
- Uses `reqwest` for HTTP calls
- Caches record IDs in SQLite for update/delete operations
- Rate limit aware (Cloudflare: 1200 req/5min)

### 3. EPP Backend (`EppBackend`)

For ccTLD operators who manage domains via EPP (Extensible Provisioning Protocol). Used by the .cv pilot.

**Config:**
```toml
[[zones]]
zone = "cv"
backend = "epp"

[zones.backend_config]
epp_server = "ssl://registry.cv:700"
client_cert = "/path/to/cert.pem"
client_key = "/path/to/key.pem"
client_id = "nodns-bot"
client_pw = "epp-password"
simulate = true                    # pilot mode
```

**Implementation:**
- Uses `instant-epp` 0.4 crate (already in Cargo.toml)
- `upsert_record` → EPP domain update (add/create host)
- `delete_record` → EPP domain update (remove host)
- `health_check` → EPP hello/login
- EPP is synchronous request-response — no polling needed

### 4. Custom Backend (`CustomBackend`)

Executes a user-defined command for each DNS operation. Enables any DNS backend without Rust code.

**Config:**
```toml
[[zones]]
zone = "custom.example"
backend = "exec"

[zones.backend_config]
upsert_cmd = "/etc/nodns/upsert.sh"
delete_cmd = "/etc/nodns/delete.sh"
# Scripts receive JSON on stdin: {"zone":"...","name":"...","type":"...","rdata":"...","ttl":3600}
```

**Implementation:**
- Spawns subprocess, passes JSON via stdin
- Exit code 0 = success, non-zero = error
- stderr captured for error messages

## Multi-Zone Configuration

```toml
[server]
bind = "127.0.0.1:9090"

[nostr]
relays = ["wss://relay.cashu.email"]
zone = "multi"                     # multi-zone mode

# Zone 1: nodns.shop via DDNS (current production)
[[zones]]
zone = "nodns.shop"
backend = "ddns"
operator_npub = "7effcccb48fc9d091a8cab663a566523c8249d7770d5fd3c31c96a0f2b8db9ed"

[zones.backend_config]
knot_address = "127.0.0.1:53"
tsig_key_name = "nodns-bot"
tsig_key_secret = "REPLACE_WITH_REAL_SECRET"

[zones.payment]
enabled = true
create_price = 2
npub_names_free = true
mint_url = "https://testnut.cashu.space"

# Zone 2: mycooldns.com via Cloudflare API
[[zones]]
zone = "mycooldns.com"
backend = "cloudflare"
operator_npub = "abc123..."

[zones.backend_config]
api_token = "${CLOUDFLARE_API_TOKEN}"
zone_id = "def456..."

[zones.payment]
enabled = true
create_price = 5
npub_names_free = false
mint_url = "https://testnut.cashu.space"

# Zone 3: .cv via EPP
[[zones]]
zone = "cv"
backend = "epp"
operator_npub = "ghi789..."

[zones.backend_config]
epp_server = "ssl://registry.cv:700"
simulate = true
```

## Backend Selection at Runtime

The event processor already has a `zone` field in each event. The bridge agent maps zone → backend:

```rust
struct BridgeAgent {
    zones: HashMap<String, ZoneConfig>,
    backends: HashMap<String, Box<dyn DnsBackend>>,
}

impl BridgeAgent {
    async fn process_event(&self, event: &Event) -> Result<(), Error> {
        let parsed = parser::parse(event)?;
        let zone = &parsed.zone;
        
        let backend = self.backends.get(zone)
            .ok_or_else(|| Error::ZoneNotConfigured(zone.clone()))?;
        
        if parsed.is_delete() {
            backend.delete_record(zone, &parsed.name, &parsed.record_type).await?;
        } else {
            backend.upsert_record(zone, &parsed.name, &parsed.record_type, &parsed.rdata, parsed.ttl).await?;
        }
        
        Ok(())
    }
}
```

## Deployment Models

### Model A: VPS + DDNS (current — nodns.shop)
Zone operator runs a VPS with Knot DNS + bridge agent. Standard deployment.

### Model B: Serverless + Cloudflare API
Zone operator runs ONLY the bridge agent (no DNS server). Cloudflare handles DNS serving. The agent can run on:
- A small VPS ($5/mo)
- A Docker container
- Eventually: a Cloudflare Worker (if Nostr WebSocket support is added)

### Model C: Embedded (OpenWrt router)
Zone operator runs a lightweight bridge agent on their router. The agent:
- Subscribes to relay.cashu.email directly
- Caches DNS records locally
- Serves them via a local DNS resolver (dnsmasq/unbound)
- Front-runs the legacy DNS system

**Implementation**: The bridge agent compiled for `mipsel` / `aarch64` with a minimal `ExecBackend` that writes to `/tmp/dns_records/` and signals dnsmasq to reload.

### Model D: TLD Registry (EPP)
For ccTLD operators (.cv, etc.) who want Nostr-native DNS. The bridge agent:
- Subscribes to relay.cashu.email
- Validates events
- Pushes updates via EPP to the registry
- The registry's authoritative servers serve the records globally

## Migration Path

1. **Phase 1 (done)**: Extract `DnsBackend` trait from existing `dns.rs`
2. **Phase 2**: Implement `CloudflareBackend`
3. **Phase 3**: Generalize `EppBackend` from existing `epp.rs` pilot code
4. **Phase 4**: Multi-zone config support (already partially done)
5. **Phase 5**: `ExecBackend` for custom integrations
6. **Phase 6**: Documentation + Docker images for easy deployment

## Open Questions

- **Cloudflare rate limits**: 1200 req/5min. At scale, may need batching or queuing. Mitigation: queue updates, batch every 5s.
- **EPP latency**: EPP requests take 1-5s each. At scale, may need connection pooling. Mitigation: persistent EPP session.
- **Backend health**: What happens when a backend is temporarily down? Mitigation: retry queue in SQLite, exponential backoff.
- **Backend-specific record limitations**: Cloudflare doesn't support all DNS record types. EPP has its own host object model. Mitigation: each backend declares supported types, event processor rejects unsupported types early.

## References

- [RFC 2136](https://datatracker.ietf.org/doc/html/rfc2136) — Dynamic DNS Updates
- [Cloudflare API — DNS Records](https://developers.cloudflare.com/api/resources/dns_records/)
- [RFC 5730-5733](https://datatracker.ietf.org/doc/html/rfc5730) — EPP
- `docs/39-protocol-v2-design.md` — Zone attestation, discovery, P2PK
- `nodns-bot-rs/src/dns.rs` — Current DDNS implementation
- `nodns-bot-rs/src/epp.rs` — Current EPP pilot implementation
