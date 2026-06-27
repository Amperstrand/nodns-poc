# 45 — Architecture Direction: Two-Component Split

> **Status**: DRAFT. Active direction document. This is where we're heading, not where we are today.

## Purpose

This document maps the current codebase to the target architecture and provides enough context for a contributor (or AI agent) to start building without rediscovering everything from scratch.

## Target Architecture

```
                         PUBLIC LAYER
                         ┌─────────────────────────────────────────┐
                         │  Nostr Relay                            │
                         │                                         │
  User publishes ──────► │  kind 11111 event (record claim)        │
  kind 11111 event       │  NO payment tag in the event            │
                         │                                         │
  User sends ──────────► │  NIP-17 encrypted DM to registrar       │
  NIP-17 DM              │  Contains Cashu token                   │
                         └──────────────┬──────────────────────────┘
                                        │
                     ┌──────────────────┴──────────────────┐
                     ▼                                     ▼
          COMPONENT 1: DNS CONNECTOR           COMPONENT 2: PAYMENT PROCESSOR
          ┌──────────────────────────┐         ┌──────────────────────────┐
          │ Subscribes to kind 11111 │         │ Listens for NIP-17 DMs   │
          │ Validites signature      │         │ addressed to registrar   │
          │ Checks authority         │         │                          │
          │ (npub = always allowed)  │         │ Decrypts DM (NIP-44)     │
          │                          │         │ Extracts Cashu token     │
          │ Pushes to DNS backend:   │         │ Verifies via CDK         │
          │  - Knot DNS (DDNS+TSIG)  │◄────────│ checkstate               │
          │  - Cloudflare (API)      │         │                          │
          │  - EPP (.cv registry)    │         │ Tells connector: mirror  │
          │                          │         │ this record              │
          │ Stores to SQLite         │         │                          │
          └──────────────────────────┘         │ Replies via NIP-17 DM:   │
                                              │ "confirmed" or "rejected"│
                                              └──────────────────────────┘
```

### Why split?

1. **Separation of concerns**: DNS infrastructure ≠ payment processing. They have different failure modes, different scaling characteristics, and different trust requirements.
2. **Independent deployability**: The DNS connector can run without payment (free zones, `$npub` names). The payment processor can be upgraded without touching DNS logic.
3. **Testability**: Each component can be tested in isolation with mocked inputs.
4. **Parallel development**: Two people can work on the two components simultaneously without merge conflicts.

### Why NIP-17 for payment?

- **Private**: Payment details are encrypted. Relay operators and snoopers can't see who's paying for what.
- **Out of band**: The kind 11111 event stays purely about DNS records. No mixing of concerns.
- **Simple**: No P2PK locking, no escrow, no refund windows. The user pays, the registrar mirrors. Trust-based for the PoC.
- **Future-proof**: Can add P2PK/escrow later without changing the event format.

## Current Codebase Mapping

**Verdict: refactor in place. Do NOT rewrite from scratch.**

The current `nodns-bot-rs` already has most of the pieces. Here's what exists, what maps to each component, and what needs to change.

### What already exists (keep as-is)

| Module | Maps to | Status | Notes |
|---|---|---|---|
| `dns.rs` (`Updater`) | DNS Connector | ✅ Done | Knot DDNS, RFC 2136 + TSIG. Production-tested. |
| `cloudflare_backend.rs` (`CloudflareBackend`) | DNS Connector | ✅ Done | Cloudflare API backend. |
| `cloudflare_backend.rs` (`DnsBackend` enum) | DNS Connector | ✅ Done | **The connector abstraction already exists.** Enum with `Ddns` + `Cloudflare` variants. Common interface: `update_record`, `delete_record`, `update_txt_multi`, `append_record`, `test_connection`. |
| `payment.rs` (`Verifier`) | Payment Processor | ✅ Done | Cashu token verification via CDK `checkstate`. Already modular, separate module. |
| `subscriber.rs` | Both (shared) | ✅ Done | Relay subscription, signature verification. Pure connector, no coupling. |
| `parser.rs` | Both (shared) | ✅ Done | Kind 11111 tag parsing, validation. |
| `auth.rs` | DNS Connector | ✅ Done | Authority checking (npub = free, custom = delegation). |
| `store.rs` | Both (shared) | ✅ Done | SQLite persistence. |
| `config.rs` | Both (shared) | ✅ Done | Multi-zone, per-zone payment, backend selection. |
| `epp.rs` (`EppPool`) | DNS Connector | ⚠️ Exists, needs integration | EPP bridge to .cv registry. Works but is NOT behind the `DnsBackend` enum. Needs to become a third variant. |

### What needs to change

| Module | Current | Target | Effort |
|---|---|---|---|
| `event_processor.rs` | Monolithic pipeline: parse → auth → payment → store → DNS, all in one function | Split: DNS connector handles parse → auth → store → DNS. Payment processor handles NIP-17 → verify → signal. | **Medium** — the pipeline is hardcoded but the pieces are modular. Extract payment out of the pipeline. |
| `payment.rs` | Called inline in `event_processor` (cashu tag from the event) | Receives payment via NIP-17 DM, not from event tags. | **Low** — `Verifier::verify_payment()` stays the same. Change the INPUT source (DM listener vs event tag). |
| `epp.rs` | Separate `EppPool` passed alongside `DnsBackend` | Becomes `DnsBackend::Epp(EppPool)` variant. | **Low** — implement the same `update_record`/`delete_record` interface for EPP. |
| NIP-17 listener | Does not exist | New module: subscribes to kind 13/14 events, decrypts, extracts Cashu token. | **New** — needs `nostr-sdk` NIP-17 support. |

### What is NOT needed for v1

- **P2PK escrow** (`docs/43-payment-escrow-model.md`) — future, not v1.
- **Public bid market** (#32 Model B) — future, not v1.
- **Per-record Cashu antispam** (#34) — pivoted away. Antispam = namespace access.
- **31111 migration** (#59) — archived. 11111 stays.
- **Blind bids / commit-reveal** — future research.

## DnsBackend: The Connector Interface

This already exists in `cloudflare_backend.rs`. The target is to add EPP as a third variant:

```rust
pub enum DnsBackend {
    Ddns(Updater),                          // Knot DNS — DONE
    Cloudflare(CloudflareBackend),          // Cloudflare API — DONE
    Epp(EppConnector),                      // .cv registry — TODO
}

impl DnsBackend {
    pub async fn update_record(&self, fqdn: &str, ttl: u32, record_type: u16, rdata: &str) -> Result<()>;
    pub async fn update_txt_multi(&self, fqdn: &str, ttl: u32, segments: &[String]) -> Result<()>;
    pub async fn delete_record(&self, fqdn: &str, record_type: u16) -> Result<()>;
    pub async fn append_record(&self, fqdn: &str, ttl: u32, record_type: u16, rdata: &str) -> Result<()>;
    pub async fn test_connection(&self) -> Result<()>;
}
```

The connector is selected per-zone via `config.rs`:

```toml
[[dns.zones]]
zone = "nodns.shop"
backend = "ddns"           # → DnsBackend::Ddns

[[dns.zones]]
zone = "example.com"
backend = "cloudflare"     # → DnsBackend::Cloudflare
cloudflare_api_token = "..."
cloudflare_zone_id = "..."

[[dns.zones]]
zone = "cv"
backend = "epp"            # → DnsBackend::Epp (TODO)
```

## Payment Flow (v1 — Simple)

```
1. User publishes kind 11111 with ["record", "A", "alice", "1.2.3.4", ...]
   → No payment tag in the event. Pure record claim.

2. User sends NIP-17 encrypted DM to registrar's npub:
   → Content: Cashu token (testnut, 2 sats)
   → DM includes reference to the record claim (event ID or record name)

3. Payment processor receives DM:
   a. Decrypts (NIP-44)
   b. Extracts Cashu token
   c. Verifies via CDK checkstate (token is unspent, correct mint, sufficient amount)
   d. If valid: tells DNS connector to mirror the record
   e. Claims the token (swap/melt)
   f. Replies via NIP-17 DM: "confirmed"

4. DNS connector:
   a. Already received the kind 11111 event (via relay subscription)
   b. Validated signature, authority (npub = always allowed, custom = delegation)
   c. Was holding the record pending payment confirmation
   d. Receives signal from payment processor → pushes DDNS/API/EPP update
   e. Record goes live
```

### Trust model

- User trusts the registrar to mirror after payment. No escrow, no P2PK.
- If the registrar doesn't mirror, the user's record still exists on the relay (publicly verifiable).
- The user can dispute publicly: "I paid, here's the DM receipt, they didn't mirror."
- This is sufficient for a PoC with testnut tokens (no real value).

### Future: Escrow (not v1)

When real value is involved, add P2PK locking with refund deadlines (`docs/43`). The NIP-17 DM flow stays the same — only the token format changes (P2PK-locked instead of plain).

## Refactor Plan

### Phase 1: Extract payment from event pipeline (low risk)

Current: `event_processor.rs` calls `payment::check_event_payment()` inline, reading cashu tags from the event.

Target: Remove payment from the event pipeline entirely. The event processor handles ONLY: parse → auth → store → DNS. Payment is a separate concern.

Steps:
1. Remove `payments` field from `ParsedEvent` (or ignore it)
2. Remove cashu tag parsing from `parser.rs` (or keep parsing but don't act on it)
3. Remove `payment::check_event_payment()` call from `event_processor.rs`
4. The event processor pushes records to DNS WITHOUT payment verification (for free zones / `$npub` names)

### Phase 2: Add NIP-17 payment listener (new module)

New module: `payment_processor.rs` (or `nip17_listener.rs`)

1. Subscribe to kind 13 events (gift-wrapped DMs) addressed to registrar's npub
2. Decrypt using registrar's nsec (NIP-44)
3. Extract Cashu token from DM content
4. Verify via existing `payment::Verifier`
5. Signal the DNS connector to process the pending record
6. Reply via NIP-17 DM

### Phase 3: Add EPP as DnsBackend variant (low risk)

1. Create `EppConnector` struct wrapping `EppPool`
2. Implement `update_record`, `delete_record` etc. by calling `EppPool::domain_create`, `domain_delete`
3. Add `DnsBackend::Epp(EppConnector)` variant
4. Update `config.rs` to support `backend = "epp"`

### Phase 4: Split into two binaries (optional)

If desired, split into:
- `nodns-connector` — subscribes to relays, processes events, pushes to DNS backends
- `nodns-payments` — listens for NIP-17 DMs, verifies Cashu, signals connector

They communicate via a shared SQLite database or an internal channel. This is optional — the current single-binary structure works fine with clear module boundaries.

## Current Config State

```toml
# deploy/config-multi-zone.toml (production)

[payment]
enabled = false              # ← Payment OFF in production

[[dns.zones]]
zone = "nodns.shop"
# No backend specified → defaults to "ddns" (Knot DNS)
# No per-zone payment section → free zone

[[dns.zones]]
zone = "cv"
[dns.zones.payment]
enabled = true               # ← Payment ON for .cv pilot (but in simulate mode)
create_price = 30000         # 30000 sats (testnut)
npub_names_free = false
mint_url = "https://testnut.cashu.space"

[epp]
simulate = true              # ← EPP in simulate mode (no real registrations)
```

## References

- [AGENTS.md](../AGENTS.md) — Full technical reference
- [CLAUDE.md](../CLAUDE.md) — AI agent entry point
- [docs/11-protocol-experimental-draft.md](11-protocol-experimental-draft.md) — Kind 11111 wire format
- [docs/22-pricing-and-payments.md](22-pricing-and-payments.md) — Pricing model (Model A: fixed rates)
- [docs/42-kind-31111-migration.md](42-kind-31111-migration.md) — ARCHIVED. 11111 stays.
- [docs/43-payment-escrow-model.md](43-payment-escrow-model.md) — Future escrow model (not v1)
- [docs/44-minimal-consensus-roadmap.md](44-minimal-consensus-roadmap.md) — Decision roadmap
- [NIP-17: Private Direct Messages](https://github.com/nostr-protocol/nips/blob/master/17.md)
- [NIP-44: Encrypted Payloads](https://github.com/nostr-protocol/nips/blob/master/44.md)
