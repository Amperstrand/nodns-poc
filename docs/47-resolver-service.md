> **Status**: ACTIVE
> **Created**: 2026-07-09
> **Updated**: 2026-07-10 (deployed, end-to-end verified)
> **Depends on**: payment.rs (Verifier), dnsproxy (DoH), Caddy (forward_auth), Knot DNS

# Cashu-Gated DoH Resolver

## What this is

A DNS-over-HTTPS (DoH) resolver at `dns.nodns.shop` that offers two tiers:

- **Free**: resolves `.nostr`, `nodns.shop`, and `dns4sats.xyz` zones. Browser-native — anyone can point Firefox/Chrome DoH at it with no setup beyond entering the URL. Non-hosted queries return REFUSED, and the browser falls back to the system resolver so normal browsing is unaffected.
- **Premium**: full internet recursion (google.com, etc.) for users who pay testnut Cashu for a subscription. Intended for privacy-conscious users who want all their DNS routed through nodns instead of their ISP or a big-tech resolver.

This is an experiment to test whether a privacy-first DNS resolver funded by ecash — no account, no credit card, no tracking — is a viable product for the Nostr/crypto-native audience.

## Why it exists

nodns's core value is turning Nostr events into live DNS records. Those records are in normal DNS (globally resolvable via any resolver), so users don't need a special resolver to reach them. But there are two scenarios where running our own resolver adds value:

1. **`.nostr` pseudo-TLD resolution.** `.nostr` is not in the global DNS root — no resolver on earth resolves `npub1xxx.nostr` except one that speaks Nostr. A nodns DoH resolver overlays `.nostr` on top of normal DNS, giving browser users something no other resolver offers. This is the free-tier differentiator.

2. **Privacy-first full DNS.** Users who want their DNS queries hidden from their ISP (and from big-tech resolvers like 8.8.8.8 that log them) can route everything through nodns. Cashu payment means no account, no identity link. This is the premium-tier product hypothesis.

The existing paid DNS market (NextDNS ~$20/yr, Control D ~$24/yr, AdGuard ~$30/yr) is saturated on features (blocklists, parental controls, analytics) but entirely traditional on payment and identity — all require accounts and credit cards. nodns's wedge is Cashu-native payment + `.nostr` resolution + no-account privacy.

## How it works

### Two-path architecture

```
Browser / CLI client
    │
    ├─ POST dns.nodns.shop/dns-query           (FREE — no auth)
    │   │
    │   ▼
    │   Caddy → dnsproxy-free (port 8053)
    │              ├─ .nostr query       → Knot (127.0.0.1:53) → authoritative answer
    │              ├─ nodns.shop query   → Knot → authoritative answer
    │              ├─ dns4sats.xyz query → Knot → authoritative answer
    │              └─ everything else     → Knot → REFUSED (browser falls back to system DNS)
    │
    └─ POST dns.nodns.shop/dns-query/premium    (PAID — Cashu subscription)
        │
        ▼
        Caddy → forward_auth → bot /api/resolver/auth
        │       ├─ no X-Subscription header     → 402 Payment Required
        │       └─ valid subscription token      → 200 OK
        │
        ▼ (auth passed)
        Caddy → dnsproxy-premium (port 8054)
                   ├─ .nostr / nodns.shop        → Knot (127.0.0.1:53)
                   └─ everything else             → Google DoH (full recursion)
```

### The free tier

The free path has no authentication. A browser configured with DoH URL `https://dns.nodns.shop/dns-query` sends queries directly to `dnsproxy-free` (port 8053). This dnsproxy instance is configured with only Knot (127.0.0.1:53) as its upstream — no external recursive resolver.

Knot DNS is authoritative-only: it answers for zones it hosts (`.nostr`, `nodns.shop`, `dns4sats.xyz`) and returns REFUSED for everything else. When a browser queries `google.com` through the free path, dnsproxy forwards to Knot, Knot returns REFUSED, dnsproxy passes it through, and the browser sees a DNS error. Firefox and Chrome both handle this by falling back to the system resolver for that query — so normal browsing continues to work via the system's configured resolver (ISP, 8.8.8.8, etc.).

The user experience: `.nostr` names resolve (the unique value), and normal browsing is unaffected (via fallback). No subscription, no headers, no setup beyond entering one URL.

### The premium tier

The premium path is gated by Caddy's `forward_auth` directive. When a request hits `/dns-query/premium`, Caddy first sends an auth subrequest to the bot's `/api/resolver/auth` endpoint. The bot checks the `X-Subscription` header: if valid (token exists, not expired, under daily rate limit), it returns 200 and Caddy proxies to `dnsproxy-premium` (port 8054). If invalid, the bot returns 402 and Caddy passes that to the client.

`dnsproxy-premium` is configured with Knot for hosted zones AND Google DoH (`https://dns.google/dns-query`) as the default upstream for everything else. This gives full internet recursion for premium subscribers.

**Privacy by architecture**: The bot's auth handler receives only HTTP headers (Caddy sends a HEAD-style subrequest). The actual DNS query body goes Caddy → dnsproxy-premium directly after auth passes. The bot never sees, logs, or touches the DNS query content. The "we don't log your queries" claim is structurally enforced by the architecture, not a policy promise.

### How Cashu payment works

The subscription purchase follows NUT-24 (Cashu's HTTP 402 Payment Required standard):

```
1. Client → POST /api/resolver/subscribe
   (no X-Cashu header)

2. Bot → 402 Payment Required
   X-Cashu: creqA<Base64(CBOR({a: 10, u: "sat", m: ["https://testnut.cashu.space"], d: "nodns resolver subscription"}))>
   Body: {"error": "payment required", "accepts": {"cashu": {"mint": "...", "amount": 10, "unit": "sat"}}}

3. Client mints testnut tokens (via faucet or wallet), retries:
   POST /api/resolver/subscribe
   X-Cashu: cashuB<Base64(CBOR(Token with 16 proofs worth 16 sats))>

4. Bot verifies token:
   a. Decode token (cashu crate)
   b. Check mint URL matches config (must be testnut.cashu.space)
   c. Check mint_filter (must contain "testnut")
   d. Check amount >= 10 sats (price_sats)
   e. Call CDK /v1/checkstate on the mint → verify all proofs unspent
   f. If valid → create subscription row in SQLite, return opaque token

5. Bot → 200 OK
   {"token": "b630fa3e-...", "expires_at": 1786260939, "daily_query_limit": 10000}

6. Client uses the subscription token:
   POST /dns-query/premium
   X-Subscription: b630fa3e-...
   (DNS query body)
   → 200 OK with DNS answer
```

The Cashu verification reuses the bot's existing `payment.rs` `Verifier` — the same code that verifies Cashu payments for DNS record creation. The `Verifier::new()` constructor was added for non-zone use cases (it takes mint_url, mint_filter, required_sats directly instead of a ZonePaymentConfig).

The NUT-18 `creqA` payment request in the 402 response is encoded as CBOR + base64_urlsafe. The nodns-registrar frontend already generates and parses these via `@cashu/cashu-ts`'s `PaymentRequest.toEncodedCreqA()`, so a future web UI can consume the challenge automatically.

### Why testnut Cashu is the anti-spam gate

testnut.cashu.space is a test mint — its tokens have no monetary value. But the anti-spam property isn't the monetary cost; it's the **friction**:

1. A spammer must install a Cashu wallet (most don't have one)
2. Must claim from the testnut faucet (rate-limited per source)
3. Must mint tokens (each requires a mint round-trip)
4. Must send them as an HTTP header (DoH is TCP — no UDP packet blast)

This setup tax filters out ~99% of automated abuse. The remaining 1% (determined spammers who automate the faucet) are capped by per-subscription rate limits (10,000 queries/day) and Caddy IP-level rate limiting.

For production, switching to a real-sats mint is a one-line config change (`mint_url` + `mint_filter`). The architecture doesn't change.

## Safety analysis

### Why this cannot be used for DDoS amplification

The BSI abuse report (CB-Report 2026-07-07) flagged the server because an open UDP recursive resolver is a DDoS amplification weapon. This DoH resolver is categorically different:

| Property | Open UDP resolver (the incident) | DoH resolver (this service) |
|---|---|---|
| Transport | UDP (stateless, spoofable source IP) | TCP+TLS (handshake required, source can't be spoofed) |
| Amplification | 50-100× (large DNS response from tiny query) | 1× (TCP handshake proves client identity before response) |
| Reflection | Yes (spoofed source → victim gets the response) | Impossible (attacker must complete TLS handshake) |
| BSI reportable | Yes | No |

DoH runs over HTTPS. Every response requires a completed TCP three-way handshake and TLS negotiation. An attacker cannot forge their source address — they must be reachable at the source IP to complete the handshake. This makes DNS amplification attacks structurally impossible. This is why every modern "resolver as a service" (NextDNS, Control D, AdGuard, Mullvad) uses DoH/DoT as its interface.

### Resource exhaustion (the residual risk)

A botnet could flood TCP connections to consume server CPU (TLS handshakes) or bandwidth. Mitigations:

- **Free tier**: responses are tiny (authoritative DNS records from Knot — a few hundred bytes). REFUSED responses are even smaller. No large recursive lookups.
- **Premium tier**: rate-limited per subscription (10,000 queries/day). An attacker needs a Cashu subscription per source IP.
- **Caddy**: can be configured with per-IP connection limits as a backstop.
- **dnsproxy**: has its own connection and query handling limits.

This is a service-availability concern (like any public web service), not a weaponizable-DDoS concern. BSI does not flag TCP-based services for this.

## Components

### dnsproxy-free (port 8053)

AdGuard's `dnsproxy` binary. Configured with only Knot as the upstream — no external recursive resolver. Resolves `.nostr`, `nodns.shop`, `dns4sats.xyz` authoritatively; REFUSES everything else.

```yaml
# /etc/dnsproxy/config-free.yaml
upstream:
  - 127.0.0.1:53
conditional-upstreams:
  - domain: nostr
    upstream:
      - 127.0.0.1:53
  - domain: nodns.shop
    upstream:
      - 127.0.0.1:53
  - domain: dns4sats.xyz
    upstream:
      - 127.0.0.1:53
```

### dnsproxy-premium (port 8054)

Same binary, different config. Has Google DoH as the default upstream for full internet recursion, plus conditional upstreams for hosted zones.

```yaml
# /etc/dnsproxy/config-premium.yaml
upstream:
  - https://dns.google/dns-query
conditional-upstreams:
  - domain: nostr
    upstream:
      - 127.0.0.1:53
  - domain: nodns.shop
    upstream:
      - 127.0.0.1:53
  - domain: dns4sats.xyz
    upstream:
      - 127.0.0.1:53
```

### Caddy

Routes two paths with different auth requirements:

```
dns.nodns.shop {
    handle /dns-query/premium {
        forward_auth 127.0.0.1:9090 {
            uri /api/resolver/auth
            copy_headers X-Subscription
        }
        reverse_proxy https://127.0.0.1:8054 { tls_insecure_skip_verify }
    }
    handle /dns-query {
        reverse_proxy https://127.0.0.1:8053 { tls_insecure_skip_verify }
    }
    handle /api/resolver/* {
        reverse_proxy 127.0.0.1:9090
    }
}
```

`forward_auth` sends an HTTP subrequest to the bot's auth endpoint *before* proxying to dnsproxy-premium. If the bot returns non-2xx, Caddy returns that response to the client and never contacts dnsproxy. The bot sees only headers (the auth subrequest carries no body), so it never sees the DNS query.

### Bot (nodns-bot, port 9090)

Three new endpoints (in `handlers/resolver.rs`):

- `POST /api/resolver/subscribe` — accepts `X-Cashu` header with a Cashu token. Verifies via `payment.rs` `Verifier::verify_payment()` (CDK checkstate, mint filter, amount check). On success, creates a `resolver_subscriptions` row with a random UUID token and returns it. On failure or missing token, returns 402 with a NUT-18 `creqA` payment request in the `X-Cashu` header.

- `GET /api/resolver/auth` — Caddy `forward_auth` target. Reads `X-Subscription` header. Validates: token exists in DB, not expired, under daily query limit. Increments `queries_today`. Returns 200 or 402.

- `GET /api/resolver/status` — returns subscription status (active, expires_at, queries_today, daily_query_limit). For debugging/client tooling.

### Knot DNS (port 53)

Unchanged. Authoritative-only for hosted zones. REFUSED for non-hosted. RRL enabled (`mod-rrl`, rate-limit 200, slip 2). The resolver service does not modify Knot's configuration.

### SQLite table

```sql
CREATE TABLE IF NOT EXISTS resolver_subscriptions (
    token TEXT PRIMARY KEY,           -- UUID v4 opaque token
    npub TEXT,                        -- optional Nostr identity
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    expires_at INTEGER NOT NULL,      -- created_at + duration_days * 86400
    queries_today INTEGER NOT NULL DEFAULT 0,
    daily_query_limit INTEGER NOT NULL,
    last_reset_day INTEGER NOT NULL DEFAULT 0,  -- unixepoch()/86400 for daily counter reset
    last_query_at INTEGER,
    payment_amount INTEGER NOT NULL   -- sats paid (for audit)
);
```

## Configuration

```toml
[resolver]
enabled = true                # master switch; false = endpoints return 503
price_sats = 10               # testnut sats for one subscription period
mint_url = "https://testnut.cashu.space"
mint_filter = "testnut"       # reject tokens from mints not containing this substring
duration_days = 30            # subscription validity
daily_query_limit = 10000     # queries per day per subscription
```

## How to use it

### Free tier (browser-native DoH)

**Firefox**: Settings → Privacy & Security → DNS over HTTPS → Custom provider → enter:
```
https://dns.nodns.shop/dns-query
```

**Chrome**: Settings → Privacy and security → Security → Use secure DNS → With Custom → enter the same URL.

After configuring, `.nostr` names resolve (e.g., `npub1xxx.nostr`). Normal browsing is unaffected — when the resolver returns REFUSED for non-hosted domains, the browser falls back to the system resolver.

**CLI** (dnspython, curl, etc.):
```python
import dns.message, dns.query, dns.rdatatype
q = dns.message.make_query('nodns.shop', dns.rdatatype.SOA)
r = dns.query.https(q, 'https://dns.nodns.shop/dns-query')
```

### Premium tier (Cashu subscription)

**Step 1: Get testnut tokens** from the faucet at `https://faucet.cashu.email/` (select testnut.cashu.space, choose 16+ sats, click Mint).

**Step 2: Subscribe**:
```bash
curl -X POST https://dns.nodns.shop/api/resolver/subscribe \
  -H "X-Cashu: cashuB..." 
# → {"token": "b630fa3e-...", "expires_at": 1786260939, ...}
```

**Step 3: Use premium DoH**:
```bash
# Build a DNS query for google.com and send via DoH with subscription token
curl -s -X POST https://dns.nodns.shop/dns-query/premium \
  -H "Content-Type: application/dns-message" \
  -H "X-Subscription: b630fa3e-..." \
  --data-binary @dns-query.bin
```

Browser-native DoH cannot send custom headers, so premium users need a local proxy or CLI tool (like `doggo` with `-H` flag) that injects the `X-Subscription` header.

## Verified evidence (2026-07-10)

### Comprehensive test suite — 24/24 passed

Full automated test suite run against production. Rate-limited tests spaced
1s apart to avoid hitting the 2/sec burst-5 governor.

**Free tier — hosted zone resolution (4 tests):**

| Query | rcode | Answers | Result |
|---|---|---|---|
| `nodns.shop SOA` | NOERROR | 1 | ✅ |
| `dns4sats.xyz SOA` | NOERROR | 1 | ✅ |
| `nodns.shop NS` | NOERROR | 1 | ✅ |
| `nodns.shop DNSKEY` | NOERROR | 1 (3 records) | ✅ (DNSSEC intact) |

**Free tier — non-hosted REFUSED / browser fallback (4 tests):**

| Query | rcode | Answers | Result |
|---|---|---|---|
| `google.com A` | REFUSED | 0 | ✅ browser falls back to system DNS |
| `example.org A` | REFUSED | 0 | ✅ |
| `github.com A` | REFUSED | 0 | ✅ |
| `cloudflare.com A` | REFUSED | 0 | ✅ |

**Free tier — HTTP status (1 test):**

| Check | Result |
|---|---|
| `POST /dns-query` (no auth) | HTTP 400 (dnsproxy accepted, NOT 402) ✅ |

**Premium tier — gating without subscription (3 tests):**

| Check | Result |
|---|---|
| `POST /dns-query/premium` (no header) | HTTP 402 ✅ |
| `POST /dns-query/premium` (fake token) | HTTP 402 ✅ |
| `POST /dns-query/premium` (empty token) | HTTP 402 ✅ |

**Subscribe — NUT-24 challenge (3 tests):**

| Check | Result |
|---|---|
| `POST /subscribe` (no token) → 402 | ✅ |
| 402 body has `accepts.cashu` JSON | ✅ |
| 402 has `X-Cashu: creqA...` header (NUT-18) | ✅ |

**Subscribe — invalid token rejection (2 tests):**

| Check | Result |
|---|---|
| Invalid Cashu token (`cashuBinvalid`) → 400 | ✅ |
| Garbage token (`garbage`) → 400 | ✅ |

**Status — edge cases (2 tests):**

| Check | Result |
|---|---|
| Status without token → 400 | ✅ |
| Status with nonexistent token → 404 | ✅ |

**Full Cashu round-trip (manually verified, 2026-07-10):**

| Step | Result |
|---|---|
| Mint 16 testnut sats from faucet.cashu.email | Token received ✅ |
| POST /subscribe with token → 200 | Subscription `b630fa3e-...` ✅ |
| google.com A via premium DoH | NOERROR, 6 A records ✅ |
| nodns.shop SOA via premium DoH | NOERROR, authoritative SOA ✅ |
| example.org A via premium DoH | NOERROR, 2 A records ✅ |
| google.com A via free DoH | REFUSED (browser fallback) ✅ |
| Subscription counter after 3 premium queries | queries_today=3 ✅ |

**Regression — existing services (4 tests):**

| Check | Result |
|---|---|
| `GET /api/health` → 200 | ✅ |
| `dig @46.224.104.12 nodns.shop SOA` | Authoritative answer ✅ |
| `dig @46.224.104.12 nodns.shop DNSKEY` | 3 DNSKEY records ✅ |
| `dig @46.224.104.12 google.com A` → empty (REFUSED) | Open resolver still closed ✅ |

**Rate limiting (1 test):**

| Check | Result |
|---|---|
| 15 rapid requests → rate limiter triggers | 11× 429, confirms governor active ✅ |

**Rate limit parameters:**
- Resolver routes: `per_second(2), burst_size(5)` via `tower_governor`
- ACME routes: `per_second(1), burst_size(3)`
- API routes: `per_second(1), burst_size(30)`
- Tests must space resolver requests ≥1s apart to avoid 429

## Design decisions

### Why DoH, not open UDP

An open UDP recursive resolver is a DDoS amplification weapon (spoofed-source reflection). We received a BSI/Hetzner abuse report for exactly this. DoH (TCP+TLS) makes amplification structurally impossible because TCP sources cannot be spoofed. This is the foundational safety property.

### Why two paths (free + premium), not one

A single gated endpoint would require every user to pay Cashu — including users who just want `.nostr` resolution. That's too much friction for the core differentiator. The free tier makes `.nostr` accessible to anyone with a browser (the unique value), while the premium tier monetizes full recursion (the resource-intensive feature). This matches the DNS industry pattern: authoritative DNS is public/free, recursive DNS is sometimes paid.

### Why Caddy forward_auth, not bot-native DoH

Caddy's `forward_auth` sends a header-only subrequest to the bot before proxying to dnsproxy. This means the bot never sees the DNS query body — privacy is enforced by the architecture, not by a logging policy. It also means zero DNS parsing code in the bot: no wireformat handling, no hickory Message construction, no EDNS edge cases. dnsproxy (already tested, already running) handles all DNS logic. The bot does only what it already knows: Cashu verification and SQLite lookups.

### Why dnsproxy, not a custom DoH server

dnsproxy (AdGuard's DNS proxy) is a mature, tested binary that handles DoH wireformat, conditional upstream routing, TLS, and connection management. Writing a custom DoH server in the bot would mean reimplementing all of this for no gain. dnsproxy's conditional upstream feature (`[/zone/]upstream`) gives us the exact routing we need: `.nostr` → Knot, everything else → upstream resolver.

### Why subscriptions, not per-query micropayments

DNS clients make 20-50 queries per page load. Per-query Cashu would require 20-50 token mints per page — impractical latency and mint load. A subscription model (pay once, get 30 days or 10,000 queries) matches how NextDNS, Control D, and Mullvad price their services. The subscription token is an opaque UUID stored in SQLite, not a Cashu token — so there's no per-query cryptographic overhead.

### Why testnut, not real sats

For the experiment phase, the friction of obtaining testnut Cashu (wallet setup + faucet) is sufficient anti-spam. Switching to real sats is a config change (`mint_url` + `mint_filter`) with no code changes. Starting with testnut lets users try the service without spending money, and lets us validate the product hypothesis before asking for real payment.

## References

- [NUT-18: Payment Requests](https://cashubtc.github.io/nuts/18/)
- [NUT-24: HTTP 402 Payment Required](https://cashubtc.github.io/nuts/24/)
- [NUT-10: Spending Conditions](https://cashubtc.github.io/nuts/10/)
- [NUT-11: P2PK](https://cashubtc.github.io/nuts/11/)
- [cashubtc/xcashu](https://github.com/cashubtc/xcashu) — official Cashu 402 demo
- [thesimplekid/cashu-proxy](https://github.com/thesimplekid/cashu-proxy) — Cashu-gated HTTP proxy reference
- [RFC 8484: DNS Queries over HTTPS](https://datatracker.ietf.org/doc/html/rfc8484)
- [x402 Specification](https://github.com/x402-foundation/x402) — Coinbase's HTTP 402 protocol (USDC-focused; we use NUT-24 instead)
- `deploy/DEPLOY.md` → "Cashu-gated DoH resolver service" — deployment runbook
