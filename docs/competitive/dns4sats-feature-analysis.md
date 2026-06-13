# dns4sats Feature Analysis — What nodns Can Learn

> Source: Reverse-engineered from dns4sats-prod Cloudflare Worker (bundled JS, Jun 2026)
> Purpose: Evaluate which dns4sats patterns could improve nodns

---

## Overview

**dns4sats** is a paid DynDNS service running as a Cloudflare Worker. Users register subdomains on managed zones (dns4sats.xyz, cashu.dev, etc.) by paying with Cashu ecash tokens. DNS records are created/updated via the Cloudflare API.

**nodns** is a decentralized DNS thought experiment. Users publish Nostr kind 11111 events to claim DNS records. A Rust bot subscribes to relays, validates events + Cashu payments, and pushes DDNS updates to Knot DNS.

| Aspect | dns4sats | nodns |
|---|---|---|
| **DNS backend** | Cloudflare API (zone-level) | Knot DNS (authoritative, DNSSEC, TSIG) |
| **Trigger** | HTTP `/nic/update` request | Nostr kind 11111 event |
| **Payment** | Cashu token per registration | Cashu token per record |
| **Auth** | Password or Cashu token hash | Nostr npub ownership (cryptographic) |
| **Deployment** | Cloudflare Worker + KV | VPS (Rust bot + Knot DNS + Caddy) |

---

## Feature 1: Multi-Zone Support

### How dns4sats does it

dns4sats serves **8+ zones** from a single worker. Each zone maps to a Cloudflare zone ID, with a per-zone API token stored as a secret binding.

**Zone configuration (hardcoded with env override):**
```typescript
function getZonesConfig(env) {
  // Allow override via ZONES_CONFIG env var
  if (env.ZONES_CONFIG) {
    try { return JSON.parse(env.ZONES_CONFIG); } catch {}
  }
  return {
    "dns4sats.xyz": "71009097e6f9ee0e65f4cd254f86e3f2",
    "funeralflowers.fun": "33f013f43c8b553619ce29302c8489ac",
    "thismoneydoesnotexist.com": "80cbc9e329788c286982eec029c09efd",
    "cashu.dev": "24edcd571bceeacffc15fcdfc7521c2a",
    "cashu.icu": "9d9b546fd842fe024d76906adbb79f32",
    "ecash.icu": "56e4a897418ffe5f640f29f1f1e70825",
    "evilcorp.icu": "c63725ab7b10b9819f65483253b74a6f",
    "tollgate.lat": "5dc2b384cf27c9cfe3b1014a8d2441a0"
  };
}
```

**Zone resolution from hostname:**
```typescript
function getZoneInfo(hostname, env) {
  const parts = hostname.split(".");
  const zoneName = parts.slice(-2).join(".");  // e.g. "cashu.dev"
  const zonesConfig = getZonesConfig(env);
  const zoneId = zonesConfig[zoneName];
  if (!zoneId) throw new Error(`Unsupported zone: ${zoneName}`);
  return { name: zoneName, id: zoneId };
}
```

**Per-zone API tokens (secret binding convention):**
```typescript
function getZoneToken(env, zoneId) {
  const secretName = `CF_API_TOKEN_${zoneId}`;
  const token = env[secretName];
  if (!token) throw new Error(`No API token for zone ID: ${zoneId}`);
  return token;
}
```

**DNS CRUD via Cloudflare API:**
```typescript
async function getDnsRecord(env, hostname, type) {
  const zoneInfo = getZoneInfo(hostname, env);
  const url = `${env.CF_API_BASE}/zones/${zoneInfo.id}/dns_records?type=${type}&name=${hostname}`;
  const res = await fetch(url, { headers: cfHeaders(env, zoneInfo.id) });
  return (await res.json()).result || [];
}

async function createDnsRecord(env, hostname, type, content, ttl) {
  const zoneInfo = getZoneInfo(hostname, env);
  const body = JSON.stringify({ type, name: hostname, content, ttl, proxied: false });
  const res = await fetch(`${env.CF_API_BASE}/zones/${zoneInfo.id}/dns_records`, {
    method: "POST", headers: cfHeaders(env, zoneInfo.id), body
  });
  return await res.json();
}

async function updateDnsRecord(env, recordId, hostname, type, content, ttl) {
  const zoneInfo = getZoneInfo(hostname, env);
  const body = JSON.stringify({ type, name: hostname, content, ttl, proxied: false });
  const res = await fetch(`${env.CF_API_BASE}/zones/${zoneInfo.id}/dns_records/${recordId}`, {
    method: "PUT", headers: cfHeaders(env, zoneInfo.id), body
  });
  return await res.json();
}
```

### What nodns could learn

nodns already has multi-zone support in its TOML config, but only runs `nodns.shop`. dns4sats shows:
- **Convention-based secret naming** (`CF_API_TOKEN_{zoneId}`) — clean pattern for many zones
- **Env var override** for zone config — deploy once, reconfigure without code change
- **Hostname → zone resolution** is simple (last 2 parts) but effective

**Relevance: LOW** — nodns uses Knot DNS with DDNS, not Cloudflare API. But the config pattern is portable.

---

## Feature 2: DynDNS-Compatible API (`/nic/update`)

> **STATUS: nodns ALREADY HAS THIS — and more.**
> nodns implements THREE backwards-compatible APIs: DynDNS v2, acme-dns, and RFC 2136 DNS UPDATE.
> See `docs/34-backwards-compatible-apis.md` for the full spec.

### How dns4sats does it

dns4sats implements the standard DynDNS protocol (compatible with routers, IoT devices, ddclient, etc.):

**Endpoints:**
- `GET /nic/update?hostname=X&myip=Y&token=cashuA...&password=Z&expiry=30`
- `POST /nic/update` with JSON body
- `GET /health` — health check
- `GET /admin` — wallet admin dashboard
- `GET /` — Swagger-style UI with usage instructions

**Full request handling flow:**
```
1. Validate User-Agent header (reject empty)
2. Rate limit check (per client IP)
3. Parse params: hostname, myip, myipv6, type, token, password, expiry, zapTarget
4. Validate hostname format + zone membership
5. Check premine list
6. Auto-detect IP from CF-Connecting-IP if myip not provided
7. Validate record content (IPv4, IPv6, TXT, CNAME)
8. If domain exists: authenticate with password, update DNS record
9. If domain is new: require Cashu token, verify payment, create DNS + register domain
```

**Supported record types:** A, AAAA, TXT, CNAME (MX mentioned in UI but not in validator)

**Standard DynDNS response codes:**
| Code | Meaning |
|---|---|
| `good {ip}` | DNS updated successfully |
| `nochg {ip}` | No change (same IP) |
| `badauth` | Wrong password |
| `badauth: domain expired` | Registration expired, needs renewal |
| `notfqdn` | Invalid hostname |
| `abuse` | Rate limited (429) |
| `911` | Server error |
| `402 + Pay-Info header` | Payment required for new domains |

**Payment-required response:**
```typescript
const paymentHeaders = { "Pay-Info": `cashu; amount="${env.PRICE_SATS} sats"` };
return text("payment required: Cashu token needed for new domains", 402, paymentHeaders);
```

**Dual-stack support:**
```typescript
// Auto-create AAAA alongside A if myipv6 provided
if (myipv6 && targetRecordType === "A") {
  await createDnsRecord(env, hostname, "AAAA", myipv6, DEFAULT_TTL);
}
```

### What nodns already has

**nodns is significantly ahead here.** It implements:

| API | Endpoint | Auth | Record Types | Use Case |
|---|---|---|---|---|
| **DynDNS v2** | `/nic/update` | HTTP Basic (npub/nsec) | A, AAAA | Routers, ddclient |
| **acme-dns** | `/register`, `/update` | UUID + API key | TXT | Let's Encrypt, ZeroSSL |
| **RFC 2136** | UDP listener | TSIG (HMAC-SHA256) | A, AAAA, CNAME, TXT, MX, SRV | nsupdate, BIND tools |

All three translate to Nostr events internally, so every change goes through the same validation pipeline.

dns4sats only has DynDNS v2. nodns has that PLUS acme-dns (for TLS cert automation) PLUS RFC 2136 (for standard DNS tooling). **No action needed.**

---

## Feature 3: Domain Expiry & Renewal

### How dns4sats does it

dns4sats tracks when each domain registration expires and prevents registration beyond the domain's own expiry date.

**Expiry config (bundled JSON):**
```json
{
  "dns4sats.xyz": { "expiresAt": "2026-03-19T23:59:59Z", "source": "configured" },
  "cashu.dev": { "expiresAt": "2026-08-29T23:59:59Z", "source": "configured" },
  "funeralflowers.fun": { "expiresAt": "2026-08-17T23:59:59Z", "source": "configured" },
  "cashu.exchange": { "expiresAt": "2026-09-03T23:59:59Z", "source": "configured" },
  "tollgate.lat": { "expiresAt": "2026-08-23T23:59:59Z", "source": "configured" },
  ...
}
```

**Expiry validation during registration:**
```typescript
// Prevent registration beyond domain expiry
const validation = await expiryDetector.validateRegistrationPeriod(hostname, expiryDays);
if (!validation.allowed) {
  return text(`badrequest: ${validation.reason}`, 400);
}
if (expiryDays > validation.maxAllowedDays) {
  expiryDays = validation.maxAllowedDays;
}
```

**Expired domain handling during updates:**
```typescript
if (updateResult.reason === "domain registration expired") {
  return text("badauth: domain expired, re-register with Cashu payment", 401);
}
```

**KV caching of expiry data:**
- Key format: `zone_expiry:{zoneName}`
- Cached to avoid re-fetching on every request
- Admin endpoint to update expiry dates

**Domain registry (KV-backed):**
Each registered domain stores:
- hostname, IP, password hash, Cashu token hash
- Registration date, expiry date
- Used for auth on subsequent updates

### What nodns could learn

**Relevance: MEDIUM** — nodns doesn't currently have any expiry/renewal concept. dns4sats shows:
- **Per-record TTL** is not enough — need per-domain registration expiry
- **Domain expiry caps** — don't let users register for longer than you own the domain
- **Renewal = new payment** — clean model where renewal is just re-registration
- **KV-backed domain registry** — simple persistence layer

nodns could add: records expire after N days unless renewed with a new Cashu payment.

---

## Feature 4: Embedded Cashu Wallet

### How dns4sats does it

dns4sats has a **full Cashu wallet** embedded in the Worker — not just token verification, but proof management, send/melt, and change tracking.

**WalletManager class:**
- Manages multiple wallet instances (one per mint URL)
- Proof states: `unspent`, `pending`, `spent`, `failed`
- Automatic restoration of `pending` → `unspent` on restart

**Payment flow (melt for Lightning):**
```
1. Receive Cashu token from user
2. Normalize token (handle cashuA/cashuB/cashu: prefixes)
3. Create wallet instance for the mint URL
4. Check token balance
5. Get melt quote from mint (amount + fee_reserve)
6. Select proofs using greedy algorithm (or wallet.send)
7. Execute melt → pays Lightning invoice
8. Receive change proofs back
9. Store remaining + change proofs
10. Log transaction with full audit trail
```

**Test token detection:**
```typescript
function isTestnutToken(token) {
  // Detects tokens from testnut mints and skips real Lightning payment
  // Logs: "Skipping Lightning payment forwarding for test tokens"
}
```

**Audit system:**
- Every transaction logged with: hostname, amount, mint, proofs, timestamps
- Daily summaries in KV
- Wallet export endpoint (`dns4sats_wallet_export_{date}.json`)
- Admin dashboard with cashu-logs viewer

### What nodns could learn

**Relevance: MEDIUM** — nodns currently just verifies tokens. dns4sats shows the value of:
- **Proof state machine** — don't just verify and discard, manage proofs as a wallet
- **Change handling** — overpayment produces change that can be reused
- **Audit trail** — every transaction logged with full context
- **Test vs production token handling** — skip real payments for test mints

nodns could evolve from "verify and discard" to "maintain a wallet balance" to fund infrastructure costs.

---

## Feature 5: Pre-Mined Subdomain Protection

### How dns4sats does it

dns4sats maintains a **blocklist of ~100 reserved subdomains** for the dns4sats.xyz zone only. If a user tries to register one, they get a snarky error message.

**Reserved names include:**
- Infrastructure: `www`, `mail`, `ftp`, `dns`, `ns1`, `ns2`, `cdn`, `api`, `admin`
- Services: `google`, `facebook`, `github`, `slack`, `discord`, `paypal`
- System: `test`, `dev`, `staging`, `beta`, `demo`, `example`
- Common: `shop`, `store`, `blog`, `forum`, `news`, `support`

**Implementation:**
```typescript
const DNS4SATS_PREMINE = ["www", "mail", "ftp", "admin", "api", ...];

function checkPremine(hostname) {
  if (!hostname.endsWith(".dns4sats.xyz")) return null;
  const subdomain = hostname.replace(".dns4sats.xyz", "");
  if (DNS4SATS_PREMINE.includes(subdomain.toLowerCase())) {
    return "🚨 CRYPTO SCAM ALERT! 🚨 ...";
  }
  return null;
}
```

### What nodns could learn

**Relevance: HIGH** — nodns should have reserved name protection. dns4sats shows:
- **Zone-specific blocklists** — different zones may have different reserved names
- **Comprehensive list** — don't forget infrastructure names (autodiscover, autoconfig, _dmarc, etc.)
- **Fun but functional** — the error message is tongue-in-cheek but the protection is real

nodns should add: a reserved names list in the policy config, checked before any record creation.

---

## Feature 6: Rate Limiting

### How dns4sats does it

dns4sats has a KV-backed rate limiter:

```typescript
const rateLimiter = new RateLimiter(env.KV);
const clientId = RateLimiter.getClientId(request);
const rateResult = await rateLimiter.checkRate(clientId);
if (!rateResult.allowed) {
  return text("abuse", 429, { "Retry-After": rateResult.retryAfter.toString() });
}
```

Client ID is derived from the request (likely IP + User-Agent combination).

### What nodns could learn

**Relevance: MEDIUM** — nodns has rate limiting in its config (`rate_limit = 5`) but the dns4sats pattern is simpler (KV-backed, per-client). nodns already does per-npub rate limiting via the Rust bot.

---

## Feature 7: Nostr Zap Integration

### How dns4sats does it

dns4sats can forward Cashu payments as Nostr zaps:

1. User provides optional `zapTarget` (a Nostr event nevent)
2. dns4sats melts the Cashu token to pay a Lightning invoice
3. The Lightning invoice is obtained via LNURL-zap to the recipient
4. A zap request event is signed with a service key

This means payments for DNS registration are **forwarded as zaps** to specified Nostr users, making the service itself revenue-neutral (it's a payment router, not a payment collector).

### What nodns could learn

**Relevance: LOW** — nodns already has Cashu payment verification built into its protocol. The zap-forwarding pattern is interesting but not critical.

---

## Summary: Recommended Features for nodns

### High Priority

| Feature | Why | Effort |
|---|---|---|
| **DynDNS API** (`/nic/update`) | Opens nodns to routers, IoT, existing tools | Medium — add HTTP endpoint to Rust bot |
| **Reserved names blocklist** | Prevent squatting on infrastructure names | Low — config entry + validation check |

### Medium Priority

| Feature | Why | Effort |
|---|---|---|
| **Domain expiry & renewal** | Records shouldn't live forever | Medium — SQLite tracking + renewal flow |
| **Domain expiry cap** | Don't register beyond domain ownership | Low — check zone expiry before accepting |
| **Enhanced Cashu wallet** | Manage proof states, change, audit | High — significant new module |

### Low Priority

| Feature | Why | Effort |
|---|---|---|
| **Multi-zone config pattern** | nodns already supports this | N/A — already done |
| **Zap forwarding** | Interesting but not critical | Medium |
| **KV-based rate limiting** | nodns has its own approach | N/A — already done |

---

## Appendix: dns4sats Architecture

```
User (curl/router/IoT)
  │
  ├─ GET /nic/update?hostname=X.dns4sats.xyz&myip=1.2.3.4&token=cashuA...
  │
  ▼
Cloudflare Worker (dns4sats-prod)
  ├─ Rate limiter (KV-backed)
  ├─ Hostname validation + premine check
  ├─ Auth: password (existing) or Cashu token (new)
  ├─ Cashu verification (amount check)
  │   └─ Optional: melt → LNURL-zap (forward payment)
  ├─ DNS CRUD via Cloudflare API
  │   ├─ getZoneInfo() → resolve zone from hostname
  │   ├─ CF_API_TOKEN_{zoneId} → per-zone auth
  │   └─ create/update DNS record
  ├─ Domain registry (KV)
  │   ├─ registerDomain(hostname, ip, token, password, expiryDays)
  │   └─ updateDomain(hostname, ip, password)
  ├─ Domain expiry detector
  │   ├─ CONFIGURED_ZONE_EXPIRY (static JSON)
  │   └─ KV cache with TTL
  └─ Admin dashboard
      ├─ Wallet balance + proof management
      ├─ Cashu transaction logs
      ├─ Domain expiry management
      └─ Wallet export (JSON)
```
