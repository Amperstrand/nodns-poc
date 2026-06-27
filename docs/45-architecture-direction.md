# 45 — Architecture Direction: Two-Component Split

> **Status**: DRAFT. Active direction document. This is where we're heading, not where we are today.

## Purpose

This document maps the current codebase to the target architecture and provides enough context for a contributor (or AI agent) to start building without rediscovering everything from scratch.

## Target Architecture

The two components are **fully decoupled** — they communicate only through Nostr events. The payment processor never talks to the DNS connector directly. Instead, the registrar publishes a public **confirmation event** (a delegation/accept tag), and the connector listens for those events.

```
  USER                    NOSTR RELAY                   REGISTRAR
    │                         │                            │
    │── kind 11111 ─────────►│ (record claim)             │
    │   ["record","A",        │                            │
    │    "alice","1.2.3.4"]   │                            │
    │                         │                            │
    │── NIP-17 DM ──────────►│── NIP-17 DM ──────────────►│
    │   (Cashu token)         │   (gift-wrapped,           │
    │                         │    NIP-44 encrypted)       │
    │                         │                  ┌─────────┤
    │                         │                  │ Verify  │
    │                         │                  │ Cashu   │
    │                         │                  │ via CDK │
    │                         │                  └────┬────┤
    │                         │                       │
    │◄── NIP-17 DM ──────────│◄── NIP-17 DM ──────────┤
    │   "confirmed"           │   (reply)              │
    │                         │                       │
    │                  ┌──────┴───────────┐            │
    │                  │ Publish kind 11111│◄───────────┤
    │                  │ ["delegation",    │ (public accept =
    │                  │  "alice.nodns.shop",│  WHOIS record +
    │                  │  npub, from,      │  payment proof +
    │                  │  until, renew]    │  authority delegation)
    │                  └──────┬───────────┘            │
    │                         │                        │
    │                         ▼                        │
    │              ┌──────────────────────┐            │
    │              │  DNS CONNECTOR       │            │
    │              │                      │            │
    │              │ Subscribes to:       │            │
    │              │  1. kind 11111       │            │
    │              │     (record claims)  │            │
    │              │  2. kind 11111       │            │
    │              │     (delegation/     │            │
    │              │      accept events)  │            │
    │              │                      │            │
    │              │ For $npub names:     │            │
    │              │  mirror immediately  │            │
    │              │  (crypto authority)  │            │
    │              │                      │            │
    │              │ For $string names:   │            │
    │              │  wait for delegation │            │
    │              │  event from          │            │
    │              │  registrar, then     │            │
    │              │  mirror              │            │
    │              │                      │            │
    │              │ Push to backend:     │            │
    │              │  Knot / Cloudflare / │            │
    │              │  EPP                 │            │
    │              └──────────────────────┘            │
```

### The confirmation event = WHOIS + payment proof + delegation

When the registrar accepts payment, it publishes a **public** kind 11111 event:

```json
{
  "kind": 11111,
  "pubkey": "<registrar-npub>",
  "tags": [
    ["delegation", "alice.nodns.shop", "npub1user...", "1700000000", "1730000000", "2"]
  ]
}
```

This single event serves three purposes:
1. **Payment confirmation**: The registrar only publishes this after accepting Cashu payment
2. **Authority delegation**: The user's npub is now authorized to manage `alice.nodns.shop`
3. **WHOIS record**: Anyone can look up who owns a name by querying the relay for delegation events

The DNS connector already processes these events — `auth.rs` checks for active delegations. The only architectural change: the connector mirrors `$string` records **after** seeing a valid delegation, instead of checking payment inline.

### Why fully decoupled (not direct signaling)?

1. **Independent deployment**: The connector and payment processor can run on different machines, different binaries, even different teams. They share zero code.
2. **Auditability**: The confirmation event is public. Anyone can verify "registrar accepted alice.nodns.shop for this npub on this date."
3. **Resilience**: If the payment processor is down, the connector keeps serving existing records. New `$string` registrations queue until the registrar comes back online.
4. **Simplicity**: The connector doesn't need a payment module at all. It just checks for delegations — which it already does in `auth.rs`.
5. **Parallel development**: The rewrite team can build the connector without knowing anything about Cashu or NIP-17.

### NIP-17 payment protocol details

NIP-17 uses a three-layer envelope for privacy:

```
Layer 3 (outer): kind 1059 (gift wrap)
  └─ random pubkey, p-tag = registrar npub
  └─ encrypted with registrar's pubkey (NIP-44)

Layer 2 (seal): kind 13 (seal)
  └─ signed by sender's real npub
  └─ encrypted with NIP-44

Layer 1 (rumor): kind 14 (DM) or custom kind
  └─ plaintext content: Cashu token + record reference
```

**Rust support:**
- The `nostr` crate supports NIP-17/NIP-59 behind the `nip59` feature flag
- `nostr-sdk` does **not** enable `nip59` by default — must be explicitly enabled
- Enable with: `nostr-sdk = { version = "0.44", features = ["nip59"] }` or use `all-nips`
- The rust-nostr repo has a [bot example](https://github.com/rust-nostr/nostr/blob/d5f937372f63/sdk/examples/bot.rs) that receives gift-wrapped DMs and auto-replies — exact pattern for a payment daemon

**Standardized payment over NIP-17:**
- **NUT-18** defines payment requests over NIP-17 (Nostr transport, `type: nostr`, `["n","17"]` tag)
- **NIP-61** defines nutzaps (kind 9321) for ecash transfer — not a DM, uses kind 10019 for mint discovery
- For v1: use a simple JSON payload in the DM content (custom format). Can migrate to NUT-18/NIP-61 later for standardization.

**Registrar inbox discovery:**
- Registrar publishes kind 10050 with relay URLs where it listens for DMs
- Users discover the registrar's inbox from the `_nodns.{zone}` TXT record or NIP-89 handler event

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

## DnsConnector: The Connector Trait

The `DnsBackend` enum has been converted to a `DnsConnector` trait for testability and modularity:

```rust
#[async_trait::async_trait]
pub trait DnsConnector: Send + Sync {
    async fn update_record(&self, fqdn: &str, ttl: u32, record_type: u16, rdata: &str) -> Result<()>;
    async fn update_txt_multi(&self, fqdn: &str, ttl: u32, segments: &[String]) -> Result<()>;
    async fn delete_record(&self, fqdn: &str, record_type: u16) -> Result<()>;
    async fn append_record(&self, fqdn: &str, ttl: u32, record_type: u16, rdata: &str) -> Result<()>;
    async fn test_connection(&self) -> Result<()>;
}
```

Implementations:
- `Updater` (dns.rs) — Knot DNS via DDNS+TSIG ✅
- `CloudflareBackend` (cloudflare_backend.rs) — Cloudflare API ✅
- `EppConnector` (future) — .cv registry via EPP — TODO

The connector is selected per-zone via `config.rs`:

```toml
[[dns.zones]]
zone = "nodns.shop"
backend = "ddns"           # → Updater

[[dns.zones]]
zone = "example.com"
backend = "cloudflare"     # → CloudflareBackend
cloudflare_api_token = "..."
cloudflare_zone_id = "..."

[[dns.zones]]
zone = "cv"
backend = "epp"            # → EppConnector (TODO)
```

## Payment Flow (v1 — Decoupled via public events)

The key insight: **the connector and payment processor never talk directly.** They communicate through public Nostr events. The connector already knows how to check for delegation events (`auth.rs`). The payment processor just needs to publish one after accepting payment.

```
$npub names (FREE — no payment needed):
  1. User publishes kind 11111 ["record","A","","1.2.3.4"]
  2. Connector sees event → npub authority = always allowed → mirror immediately
  Done. No payment processor involved.

$string names (PAID — requires registrar confirmation):
  1. User publishes kind 11111 ["record","A","alice","1.2.3.4"]
     → Connector sees event → $string name → no delegation found → HOLD (don't mirror yet)

  2. User sends NIP-17 DM to registrar with Cashu token
     → Payment processor decrypts DM
     → Verifies Cashu via CDK checkstate
     → Claims token

  3. Registrar publishes kind 11111 ["delegation","alice.nodns.shop", npub, from, until, renew]
     → This is the PUBLIC confirmation (whois + payment proof + authority)

  4. Connector sees delegation event → re-checks held record → authority now valid → mirror
     Record goes live.

  5. Registrar replies via NIP-17 DM: "confirmed: alice.nodns.shop"
```

### What the connector needs to do

The connector already processes delegation events via `auth.rs`. The only change: for `$string` records without a delegation, **hold the record** instead of rejecting it. When a delegation event arrives (either retroactively or concurrently), process the held record.

This is a minor change to `event_processor.rs` — change the authority check from "reject if no delegation" to "queue if no delegation, process when delegation arrives."

### What the payment processor needs to do

1. Listen for NIP-17 DMs (kind 1059 gift-wrapped events with `p` tag = registrar npub)
2. Decrypt: unwrap gift wrap (1059) → unseal (kind 13) → read rumor (kind 14 or custom)
3. Parse DM content: extract Cashu token + record name
4. Verify Cashu via existing `payment::Verifier` (CDK checkstate)
5. Claim the token
6. Publish kind 11111 delegation event (public confirmation)
7. Reply via NIP-17 DM to user

### Trust model

- User trusts the registrar to publish the delegation after payment. No escrow, no P2PK.
- The delegation event is public — anyone can verify the registrar accepted.
- If the registrar doesn't publish, the user's record claim exists on the relay (publicly verifiable) but isn't mirrored.
- The user can dispute: "I paid, here's the DM, they didn't delegate."
- Sufficient for a PoC with testnut tokens (no real value).

### Future: Escrow (not v1)

When real value is involved, add P2PK locking with refund deadlines (`docs/43`). The NIP-17 DM flow stays the same — only the token format changes (P2PK-locked instead of plain).

## Refactor Plan

### Phase 1: Convert DnsBackend to trait (IN PROGRESS)

Convert from enum to `DnsConnector` trait with `async_trait`. Makes connectors mockable, testable, and extractable into a separate crate. See issue #75/#73.

### Phase 2: Extract payment from event pipeline

Current: `event_processor.rs` calls `payment::check_event_payment()` inline.

Target: Remove payment entirely from the event pipeline. The connector:
- Mirrors `$npub` records immediately (always authorized)
- Holds `$string` records pending a delegation event from the registrar
- Never checks payment — just checks for delegations (which it already does in `auth.rs`)

### Phase 3: Add NIP-17 payment listener (new module)

New module that:
1. Subscribes to kind 1059 events with `p` tag = registrar npub
2. Unwraps gift wrap → unseals → reads DM content
3. Extracts Cashu token
4. Verifies via `payment::Verifier`
5. Claims token
6. Publishes kind 11111 delegation event (the public confirmation)
7. Replies via NIP-17 DM

Requires enabling `nip59` feature in `nostr-sdk` Cargo dependency.

### Phase 4: Add EPP as DnsConnector implementation

Implement `DnsConnector` trait for an `EppConnector` wrapping `EppPool`. Same interface as Knot/Cloudflare — just calls `domain_create` / `domain_delete` instead of DDNS.

### Phase 5: Extract connectors into separate crate (optional)

Move `dns.rs`, `cloudflare_backend.rs`, `epp.rs`, and the `DnsConnector` trait into a `nodns-connectors/` crate. The bot depends on it as a library. The rewrite team can depend on it without copying code.

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
