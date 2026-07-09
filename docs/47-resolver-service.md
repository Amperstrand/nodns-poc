> **Status**: DRAFT
> **Created**: 2026-07-09
> **Depends on**: payment.rs (Verifier), dnsproxy (existing DoH), Caddy (forward_auth)

# Cashu-Gated DoH Resolver Service

## Overview

A paid DNS-over-HTTPS (DoH) resolver at `dns.nodns.shop/dns-query` that resolves
`.nostr` names, `nodns.shop` zones, and the full internet — gated by Cashu ecash
payment. No accounts, no KYC: users pay testnut sats for a time-limited
subscription, receive an opaque token, and configure their browser/OS DoH client
to use the endpoint.

This is an **experiment** to test the hypothesis that a privacy-first DNS
resolver funded by ecash (no account, no tracking) is a viable product for the
Nostr/crypto-native audience. The differentiator from free resolvers (1.1.1.1,
8.8.8.8) and paid resolvers (NextDNS, Control D) is:

- **`.nostr` resolution** — the only public resolver that overlays Nostr-native names
- **Cashu payment** — no account, no credit card, no identity link
- **testnut for anti-spam** — the friction of obtaining testnut Cashu (wallet
  setup + faucet rate limits) is the abuse gate, not monetary cost

### Why not just run an open UDP resolver?

An open UDP recursive resolver is an amplification DDoS weapon (spoofed-source
reflection). We received a BSI/Hetzner abuse report (CB-Report 2026-07-07) for
exactly this. **This service is DoH-only (TCP+TLS)** — amplification is
structurally impossible because TCP sources cannot be spoofed. See
`deploy/DEPLOY.md` → "DNS hardening" for the full incident history.

## Safety invariants (non-negotiable)

These properties MUST hold at every stage. Any implementation that violates one
is a bug, not a trade-off.

| # | Invariant | Enforcement |
|---|---|---|
| 1 | **DoH only. No UDP port 53.** | dnsproxy runs `--port=0`. Caddy is the only public surface (HTTPS). No UDP DNS listener is ever exposed. |
| 2 | **Every query requires a valid subscription.** | Caddy `forward_auth` calls `/api/resolver/auth` before proxying to dnsproxy. No valid token → 402. Zero DNS resolution happens without payment. |
| 3 | **testnut-only mint filter.** | `Verifier` constructed with `mint_filter = "testnut"`. Non-testnut Cashu tokens are rejected at subscribe time. |
| 4 | **Per-subscription daily rate limit.** | Bot tracks `queries_today` per subscription. Exceeds `daily_query_limit` → 402 (renew). |
| 5 | **Caddy IP-level rate limit on `/dns-query`.** | Connection-flood backstop before the subscription check runs. |
| 6 | **No DNS query content logged.** | The bot only sees the auth request (token + source IP). The actual DNS query goes Caddy → dnsproxy directly after auth passes. The bot never touches it. Privacy is structural, not a policy promise. |
| 7 | **Knot DNS on port 53 is untouched.** | dnsproxy forwards `.nostr` and `nodns.shop` queries to `127.0.0.1:53` (Knot authoritative). Knot stays authoritative-only, REFUSED for non-hosted, RRL on. |

## Existing infrastructure (what we build on)

### dnsproxy (already running)

```
dnsproxy \
  --port=0 \                              # no plain DNS
  --https-port=8053 \                     # DoH only
  --listen=127.0.0.1 \                    # localhost only
  --tls-crt=... --tls-key=... \
  -u "[/nostr/]127.0.0.1:53" \            # .nostr → Knot authoritative
  -u "[/dns4sats.xyz/]127.0.0.1:53" \     # dns4sats.xyz → Knot
  -u https://dns.google/dns-query         # everything else → Google DoH (recursion)
```

This is AdGuard's `dnsproxy` binary. It handles DoH wireformat (RFC 8484),
`.nostr` routing, and upstream recursion. **It does not change.**

### Caddy route (currently open — needs gating)

```
dns.nodns.shop {
    handle /dns-query {
        reverse_proxy https://127.0.0.1:8053 { tls_insecure_skip_verify }
    }
}
```

This route is live but unauthenticated today. The experiment adds `forward_auth`.

### payment.rs Verifier (reuse, don't rebuild)

```rust
// payment.rs:191 — already implemented, already tested
pub async fn verify_payment(
    &self,
    token_string: &str,
    required_amount: i64,
) -> Result<u64, PaymentError>
```

This method: decodes the Cashu token → checks mint URL matches config → checks
`mint_filter` → checks amount ≥ required → calls CDK `/v1/checkstate` → verifies
all proofs unspent. It has circuit breaker + timeout on the mint call. **The
resolver subscribe endpoint is a direct caller of this method.**

## Payment protocol: NUT-24 + NUT-18

We follow **NUT-24** (Cashu-native HTTP 402), not the Coinbase x402 spec (which
is USDC/EVM-focused). NUT-24 is simpler, Cashu-native, and the registrar already
generates NUT-18 payment requests.

### NUT-24 flow

NUT-24 defines the `X-Cashu` HTTP header for both the payment challenge and the
payment itself:

```
1. Client → POST /api/resolver/subscribe  (no X-Cashu header)

2. Server → 402 Payment Required
   X-Cashu: creqA<base64_urlsafe(CBOR(PaymentRequest))>
   Content-Type: application/json
   Body: { "error": "payment required", "details": {...} }

3. Client mints testnut Cashu tokens, retries:
   POST /api/resolver/subscribe
   X-Cashu: cashuB<base64_urlsafe(CBOR(Token))>

4. Server verifies token via Verifier::verify_payment(token, price_sats)
   ├─ Valid → 200 OK, returns subscription token
   └─ Invalid → 400 Bad Request
```

### NUT-18 payment request (the 402 challenge body)

The `X-Cashu` header in the 402 response contains a NUT-18 payment request
encoded as `creqA` (CBOR + base64_urlsafe). No transport field — payment is
in-band per NUT-24.

```json
{
  "a": 10,                                    // amount in sats
  "u": "sat",                                 // unit
  "m": ["https://testnut.cashu.space"],       // accepted mints
  "d": "nodns resolver — 30 day subscription" // description
}
```

Encoded: `creqA` + `base64_urlsafe(CBOR(json))`.

The nodns-registrar already generates these via
`PaymentRequest([transport], id, amount, unit, mints, description).toEncodedCreqA()`.
The bot needs the server-side equivalent: serialize a payment request to `creqA`
for the 402 challenge. This is a small CBOR+base64 encoding step.

### Optional: P2PK locking (NUT-11)

For production (not the experiment), the payment request can include a `nut10`
field requiring P2PK locking:

```json
{
  "a": 10,
  "u": "sat",
  "m": ["https://testnut.cashu.space"],
  "nut10": {
    "k": "P2PK",
    "d": "02<server-pubkey-hex>",
    "t": [["sigflag", "SIG_INPUTS"]]
  }
}
```

This forces the client to lock tokens to the server's pubkey, so intercepted
tokens can only be spent by the server. Reference: `thesimplekid/cashu-proxy`
uses this pattern. **Deferred for the experiment** — testnut tokens have no
monetary value, so interception is low-risk.

## Architecture

```
                                    ┌─────────────────────────────────┐
                                    │         dns.nodns.shop           │
                                    │       (Caddy, HTTPS only)        │
                                    │                                  │
  Client ─── POST /dns-query ──────►│  1. rate_limit (IP, burst)       │
  (browser/OS                        │  2. forward_auth ──────────────┐ │
   DoH config)   X-Subscription:     │  3. reverse_proxy ─────┐       │ │
  ─────────────────────────────────►│     → 127.0.0.1:8053    │       │ │
                                    │        (dnsproxy)       │       │ │
                                    └────────────────────────┬───┬───┘ │
                                                             │   │     │
                                     ┌───────────────────────┘   │     │
                                     ▼                          ▼     │
                              ┌──────────────┐          ┌──────────────┘
                              │  Bot (9090)  │          │   dnsproxy (8053)
                              │ /api/resolver│          │   .nostr → Knot:53
                              │   /auth      │          │   everything → Google DoH
                              │              │          │
                              │ Check token: │          └──────────────────
                              │ valid?       │
                              │ not expired? │
                              │ under limit? │
                              │ ├─ NO → 402  │
                              │ └─ YES→ 200  │
                              └──────────────┘

  Subscribe flow (one-time per period):

  Client ─── POST /api/resolver/subscribe ───►  Bot (9090)
             X-Cashu: cashuB...                  │
                                                ├─ 402 + X-Cashu: creqA... (no token)
                                                │
                                                ├─ verify_payment(token, 10)
                                                │   (CDK checkstate, mint_filter=testnut)
                                                │
                                                ├─ INSERT resolver_subscriptions
                                                │
                                                └─ 200 { token, expires_at, doh_endpoint }
```

### Why Caddy forward_auth (not bot-native DoH)

Caddy's `forward_auth` directive sends a subrequest to the bot's auth endpoint
*before* proxying to dnsproxy. This gives us:

- **Privacy by architecture**: the bot only sees the auth request (headers +
  source IP). The DNS query body goes Caddy → dnsproxy directly. The bot has no
  way to log which domains a subscriber queries.
- **Zero DNS code in the bot**: no wireformat parsing, no hickory Message
  construction, no EDNS handling. The bot does only what it already knows:
  Cashu verification + SQLite CRUD.
- **dnsproxy unchanged**: the existing DoH resolver (tested, working,
  `.nostr`-aware) stays exactly as-is.

The trade-off: the auth endpoint is called on every DNS query, adding ~1ms
latency (localhost SQLite lookup). This is negligible compared to DNS resolution
latency.

## API specification

### POST /api/resolver/subscribe

Purchase a subscription. Returns a subscription token for use in DoH queries.

**Request:**
```
POST /api/resolver/subscribe
Content-Type: application/json
X-Cashu: cashuB<base64_urlsafe(CBOR(Token))>     # Cashu token (testnut sats)
X-Nostr-Npub: npub1...                            # optional, for identity
```

**Responses:**

| Status | Condition | Body |
|---|---|---|
| 200 | Valid Cashu token (verified via CDK checkstate, amount met, mint=testnut) | `{ "token": "<opaque>", "expires_at": <unix_ts>, "daily_query_limit": 10000, "doh_endpoint": "https://dns.nodns.shop/dns-query" }` |
| 402 | No `X-Cashu` header or token rejected | `X-Cashu: creqA<...>` (NUT-18 payment request). JSON body: `{ "error": "payment required", "accepts": { "cashu": { "mint": "https://testnut.cashu.space", "amount": 10, "unit": "sat" } } }` |
| 400 | Token from wrong mint, wrong unit, insufficient amount | `{ "error": "invalid payment", "reason": "..." }` |

**Side effects on success:**
- Cashu token is spent (proofs recorded as spent via CDK checkstate — the mint
  prevents double-spend on subsequent `checkstate` calls).
- A new row in `resolver_subscriptions` with a random opaque token.
- Rate limited: `GovernorConfigBuilder` per-IP, 1 req/sec, burst 3 (same as
  ACME order endpoint).

### GET /api/resolver/auth

Caddy `forward_auth` target. Validates a subscription token for an incoming DoH
query. **This endpoint is called by Caddy, not by end users directly.**

**Request (from Caddy):**
```
GET /api/resolver/auth
X-Subscription: <opaque-token>
X-Forwarded-For: <client-ip>         # Caddy adds this
```

**Responses:**

| Status | Condition | Action |
|---|---|---|
| 200 | Valid token, not expired, under daily limit | Caddy proxies to dnsproxy |
| 402 | No token, expired, or over daily limit | Caddy returns 402 to client |

**Side effects on 200:** `queries_today` incremented, `last_query_at` updated.
Daily counter resets at UTC midnight (compared via `last_reset_day`).

### GET /api/resolver/status

Check subscription status (optional, for debugging).

**Request:**
```
GET /api/resolver/status
X-Subscription: <opaque-token>
```

**Response:**
```json
{
  "active": true,
  "expires_at": 1722520640,
  "queries_today": 3421,
  "daily_query_limit": 10000,
  "doh_endpoint": "https://dns.nodns.shop/dns-query"
}
```

## Data model

### New table: `resolver_subscriptions`

```sql
CREATE TABLE IF NOT EXISTS resolver_subscriptions (
    token           TEXT PRIMARY KEY,         -- opaque random token (32 bytes hex)
    npub            TEXT,                     -- optional Nostr identity
    created_at      INTEGER NOT NULL,         -- unix timestamp
    expires_at      INTEGER NOT NULL,         -- unix timestamp
    queries_today   INTEGER NOT NULL DEFAULT 0,
    daily_query_limit INTEGER NOT NULL,       -- from config at creation time
    last_reset_day  INTEGER NOT NULL,         -- Julian day number for daily reset
    last_query_at   INTEGER,                  -- unix timestamp of last query
    payment_amount  INTEGER NOT NULL          -- sats paid (for audit)
);
```

Added to `store.rs` SCHEMA constant + `CREATE INDEX IF NOT EXISTS
idx_resolver_expires ON resolver_subscriptions(expires_at)` in `run_migrations`.

### New store methods

```rust
impl Store {
    pub fn create_resolver_subscription(
        &self, npub: Option<&str>, expires_at: i64,
        daily_query_limit: i64, payment_amount: u64,
    ) -> Result<String, StoreError>;

    pub fn validate_resolver_subscription(
        &self, token: &str,
    ) -> Result<bool, StoreError>;  // also increments queries_today

    pub fn get_resolver_subscription(
        &self, token: &str,
    ) -> Result<Option<ResolverSubscription>, StoreError>;
}
```

## Configuration

New `[resolver]` section in `config.toml`:

```toml
[resolver]
enabled = true
price_sats = 10                      # testnut sats for one subscription period
mint_url = "https://testnut.cashu.space"
mint_filter = "testnut"              # reject non-testnut tokens
duration_days = 30                   # subscription validity
daily_query_limit = 10000            # queries per day per subscription
```

### Config struct (config.rs)

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ResolverConfig {
    pub enabled: bool,
    pub price_sats: i64,
    pub mint_url: String,
    pub mint_filter: String,
    pub duration_days: u32,
    pub daily_query_limit: i64,
}

impl Default for ResolverConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            price_sats: 10,
            mint_url: "https://testnut.cashu.space".to_string(),
            mint_filter: "testnut".to_string(),
            duration_days: 30,
            daily_query_limit: 10000,
        }
    }
}
```

## Caddy configuration

The existing `dns.nodns.shop` route gets `forward_auth` added:

```
dns.nodns.shop {
    log { }

    @doh path /dns-query
    handle @doh {
        forward_auth 127.0.0.1:9090 {
            uri /api/resolver/auth
            copy_headers X-Subscription
            method HEAD
        }
        reverse_proxy https://127.0.0.1:8053 {
            transport http {
                tls_insecure_skip_verify
            }
        }
        header {
            Content-Type application/dns-message
            Access-Control-Allow-Origin *
            Access-Control-Allow-Methods "GET, POST, OPTIONS"
            Access-Control-Allow-Headers "Content-Type, X-Subscription"
        }
    }

    handle /api/resolver/* {
        reverse_proxy 127.0.0.1:9090
    }

    # Optional: rate limit on /dns-query
    # (Caddy doesn't have built-in RRL; use a module or rely on bot-side limit)
}
```

**Note on `method HEAD`**: Caddy's `forward_auth` sends the auth subrequest using
the original request's method by default. For DoH (POST), this would send the
DNS query body to the auth endpoint — wasteful and a privacy leak. Setting
`method HEAD` ensures the auth subrequest is header-only (just the
`X-Subscription` token), no body.

**Rollback**: Remove the `forward_auth` block → reverts to the current
open-proxy behavior. No bot changes needed to roll back.

## Client onboarding flow

### 1. Get testnut Cashu

```
# Option A: Use a Cashu wallet (enuts, minibits, nutstash) with testnut mint
# Option B: Use the nodns CLI (future) or curl against the testnut faucet
```

### 2. Subscribe

```bash
curl -X POST https://dns.nodns.shop/api/resolver/subscribe \
  -H "X-Cashu: cashuBeyJ0b2tlbiI6W3sicHJvb2ZzIjpb..." \
  -H "Content-Type: application/json"

# Response:
# {
#   "token": "a1b2c3d4e5f6...",
#   "expires_at": 1725111640,
#   "daily_query_limit": 10000,
#   "doh_endpoint": "https://dns.nodns.shop/dns-query"
# }
```

### 3. Configure DoH client

**Firefox**: Settings → Privacy & Security → DNS over HTTPS → Custom provider:
```
https://dns.nodns.shop/dns-query
```
(Firefox doesn't natively send custom headers with DoH. For the experiment, use
a DoH client that supports custom headers — e.g., `doggo`, `kdig`, or a local
proxy that adds `X-Subscription`.)

**doggo** (CLI DNS client with DoH support):
```bash
doggo @https://dns.nodns.shop/dns-query example.com \
  -H "X-Subscription: a1b2c3d4e5f6..."
```

**Local proxy** (adds the header for clients that can't):
```bash
# Simple socat/sed proxy that injects X-Subscription header
# (or a small Go/Rust binary — deferred for the experiment)
```

**Note**: Browser-native DoH doesn't support custom headers. This is a known
limitation. The experiment targets CLI tools and local proxies initially. A
browser extension or a local DoH proxy (like `dnsproxy` with a custom upstream
header injector) is the path to browser support. **This is acceptable for an
experiment.**

## Security analysis

### Amplification (the thing that got us flagged)

**Not possible.** DoH runs over TCP+TLS. TCP requires a three-way handshake
with the real client — source IP cannot be spoofed. There is no UDP path.
dnsproxy has `--port=0` (no plain DNS listener). Caddy is HTTPS-only. No amount
of misconfiguration creates an amplification vector, because there is no
stateless transport.

### Abuse / resource exhaustion

- **Subscription gate**: no DNS resolution without a valid subscription token.
  An attacker cannot consume resolver resources without first obtaining testnut
  Cashu (friction: wallet setup, faucet rate limits).
- **Per-subscription rate limit**: `daily_query_limit` caps queries per token.
  An attacker who gets one subscription is bounded.
- **Caddy connection limits**: Caddy can be configured with connection rate
  limiting per source IP as a backstop.
- **Token rotation**: subscription tokens are opaque random 32-byte values.
  Guessing is computationally infeasible.

### Privacy

- **The bot never sees DNS queries.** Caddy `forward_auth` sends only headers
  (method HEAD). The DNS query body goes Caddy → dnsproxy. The bot logs: token
  hash, source IP (for rate limiting), timestamp. It does NOT log: queried
  domains, query type, response.
- **No account**: subscriptions are created from Cashu tokens with no personal
  data. The `npub` field is optional.
- **No correlation**: each subscription is an independent opaque token. There
  is no user table to correlate multiple subscriptions to one identity.

### Double-spend

- Cashu tokens are single-use. The mint's `checkstate` endpoint (called by
  `verify_payment`) prevents double-spend. A token spent at our subscribe
  endpoint cannot be reused — the mint marks it spent.

## Implementation phases

### Phase 1: Bot subscribe + auth endpoints (the core)

**Files touched:**
- `nodns-bot-rs/src/config.rs` — add `ResolverConfig` struct + field on `Config`
- `nodns-bot-rs/src/store.rs` — add `resolver_subscriptions` table + 3 methods
- `nodns-bot-rs/src/handlers/mod.rs` — add `resolver_subscribe_handler`, `resolver_auth_handler`, `resolver_status_handler`
- `nodns-bot-rs/src/main.rs` — add `resolver_routes` router, construct Verifier from `[resolver]` config, add to AppState

**Verification:**
- `cargo test` — unit tests for store methods + config parsing
- Local: `curl -X POST localhost:9090/api/resolver/subscribe` without token → 402
- Local: `curl -X POST localhost:9090/api/resolver/subscribe -H "X-Cashu: <testnut token>"` → 200 with subscription token
- Local: `curl -H "X-Subscription: <token>" localhost:9090/api/resolver/auth` → 200

### Phase 2: Caddy forward_auth wiring

**Files touched:**
- `/etc/caddy/Caddyfile` on VPS — add `forward_auth` to `dns.nodns.shop`

**Verification:**
- `curl -X POST https://dns.nodns.shop/dns-query` without `X-Subscription` → 402
- `curl -X POST https://dns.nodns.shop/dns-query -H "X-Subscription: <token>" -H "Content-Type: application/dns-message" --data-binary @query.bin` → DNS answer
- `doggo @https://dns.nodns.shop/dns-query example.com -H "X-Subscription: <token>"` → resolves

### Phase 3: NUT-18 creqA challenge (the 402 body)

**Files touched:**
- `nodns-bot-rs/src/handlers/mod.rs` — add CBOR + base64_urlsafe encoding for
  the NUT-18 payment request in the 402 response's `X-Cashu` header

**Verification:**
- Decode the `X-Cashu` header from the 402 response → valid NUT-18 payment request JSON
- The registrar's `PaymentRequest.fromEncodedRequest()` can parse it

### Phase 4 (deferred): Client tooling

- Browser extension or local DoH proxy that injects `X-Subscription` header
- A simple web page at `dns.nodns.shop` with subscribe button + setup instructions
- CLI command: `nodns resolver subscribe` (in nodns-cli)

### Not in scope

- `.nostr` overlay changes (dnsproxy already does this)
- DoH wireformat handling in the bot (Caddy + dnsproxy handle it)
- Per-query micropayments (subscription model is simpler and sufficient)
- Blocklists, filtering, parental controls (we're not NextDNS)
- P2PK locking of tokens (testnut has no monetary value; deferred to production)
- Real-sats production mode (switch `mint_filter` + `mint_url` when ready)

## References

### Cashu specs
- **NUT-18** (Payment Requests): https://cashubtc.github.io/nuts/18/
- **NUT-24** (HTTP 402 Payment Required): https://cashubtc.github.io/nuts/24/
- **NUT-10** (Spending Conditions): https://cashubtc.github.io/nuts/10/
- **NUT-11** (P2PK): https://cashubtc.github.io/nuts/11/

### Reference implementations
- **cashubtc/xcashu** — official Cashu 402 demo: https://github.com/cashubtc/xcashu
- **thesimplekid/cashu-proxy** — Cashu-gated HTTP proxy with P2PK, SQLite double-spend: https://github.com/thesimplekid/cashu-proxy
- **ngmisl/deez-cashus** — Go Cashu-402 middleware with spent-token tracking: https://github.com/ngmisl/deez-cashus
- **Routstr/otrta-client** — Cashu gateway for AI APIs with change return via X-Cashu: https://github.com/Routstr/otrta-client
- **Traviseric/cashu-l402** — TypeScript Cashu NUT-24 + L402 library: https://github.com/Traviseric/cashu-l402

### x402 (Coinbase — for awareness, not adoption)
- **x402 spec v2**: https://github.com/x402-foundation/x402/blob/main/specs/x402-specification-v2.md
- Uses `PAYMENT-REQUIRED` / `PAYMENT-SIGNATURE` / `PAYMENT-RESPONSE` headers
- USDC/EVM-focused, not Cashu-native. NUT-24 is the right protocol for this project.

### Existing nodns code (reuse targets)
- `payment.rs:191` — `Verifier::verify_payment(token, amount)` — CDK checkstate, mint filter, amount check
- `payment.rs:143` — `Verifier::from_zone_config(&ZonePaymentConfig)` — constructor
- `config.rs:138` — `ZonePaymentConfig` — model for `ResolverConfig`
- `store.rs:1282` — `SCHEMA` constant — add table here
- `store.rs:1226` — `run_migrations` — add index here
- `main.rs:678-756` — axum router pattern — add `resolver_routes`
- `nodns-registrar/contexts/WalletContext.tsx:170` — `createPaymentRequest().toEncodedCreqA()` — client-side NUT-18 generation (already exists)
