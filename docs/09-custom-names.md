# 09 — Custom Name Registration

> **Status**: DRAFT. Research into `$string.tld` registration flows.

## Overview

Free npub-based domains (`npub1...nodns.shop`) are unlimited. Custom human-readable names (`alice.nodns.shop`) require a small payment to deter spam and squatting.

**Price**: 250 sats per name registration (lease, not permanent ownership).

## Payment Architecture

### Recommended: Cashu (Primary Rail)

Cashu is the simplest programmatic payment primitive for a server-side Go app:

- **Bearer tokens**: user sends a Cashu token, we redeem it — no wallet pairing needed
- **Server-controlled**: we run a merchant wallet, accept tokens, verify on our terms
- **No custody complexity**: tokens are either valid or they aren't
- **Test mint**: `https://testnut.cashu.space` provides fake ecash for PoC testing

#### Go Libraries

| Library | Type | Notes |
|---------|------|-------|
| `cdk-go` (github.com/cashubtc/cdk-go) | FFI bindings to Rust CDK | Full wallet features, requires CGO |
| `gonuts` (github.com/elnosh/gonuts) | Pure Go | Direct protocol control, early/unreviewed |

**Recommendation**: Start with `gonuts` for pure Go (no CGo, simpler build). Migrate to `cdk-go` if we need production wallet features.

#### Token Format

- **V4** (`cashuB...`): Base64url CBOR, preferred — groups proofs by mint URL + keyset ID
- **V3** (`cashuA...`): Base64url JSON, deprecated but still supported

### NWC (Optional Secondary Rail)

NIP-47 (Nostr Wallet Connect) lets users pay from their existing Lightning wallet:

- **User experience**: scan QR code or click a link, approve in wallet
- **Wallet support**: Alby, Mutiny, Primal, Phoenix
- **Go library**: `github.com/untreu2/go-nwc`
- **Downside**: requires wallet pairing per user, more complex flow

### Zaps (Social Proof Only)

NIP-57 zaps are NOT suitable as a primary payment mechanism:
- Zap receipt is **not proof of payment** (spec explicitly says so)
- Complex verification (LNURL callback, description hash matching)
- Best used as optional public social proof after payment

## Registration Flow

### High-Level

```
User                     NoDNS Service                  Cashu Mint
 │                           │                              │
 │  1. POST /register        │                              │
 │  {name: "alice"}          │                              │
 │──────────────────────────▶│                              │
 │                           │                              │
 │  2. {order_id, amount,    │                              │
 │     cashu_token_request}  │                              │
 │◀──────────────────────────│                              │
 │                           │                              │
 │  3. POST /pay             │                              │
 │  {order_id, cashu_token}  │                              │
 │──────────────────────────▶│                              │
 │                           │  4. Redeem/verify token      │
 │                           │─────────────────────────────▶│
 │                           │                              │
 │                           │  5. Token valid (250 sats)   │
 │                           │◀─────────────────────────────│
 │                           │                              │
 │  6. {status: "registered",│                              │
 │     name: "alice.nodns.shop"}                            │
 │◀──────────────────────────│                              │
 │                           │                              │
 │  7. Publish kind 11111    │                              │
 │     with DNS records      │                              │
 │──────────────────────────▶│                              │
 │                           │  8. Bot validates + DDNS     │
 │                           │      update to Knot DNS      │
```

### Step-by-Step

1. **Name check**: User requests `alice.nodns.shop`. Service checks:
   - Name matches `[a-z0-9]([a-z0-9-]*[a-z0-9])?` (lowercase, no leading/trailing hyphens)
   - Length 3-63 characters
   - Not already registered
   - Not a reserved name (`ns1`, `ns2`, `www`, `admin`, etc.)

2. **Create order**: Service generates an order with status `pending`, returns order ID + payment amount (250 sats)

3. **User pays**: User sends Cashu token worth 250 sats. Token must:
   - Be from an allowed mint (testnut for PoC, production mints later)
   - Sum to exactly 250 sats (accounting for mint fees via NUT-02 `input_fee_ppk`)
   - Contain valid, unspent proofs

4. **Verify payment**: Service redeems the token with the mint:
   - Split/recombine if needed to get exact amount
   - Verify proofs are valid (mint confirms)
   - Mark order as `paid`

5. **Register name**: Service creates a mapping:
   ```
   alice.nodns.shop → npub1abc123... (user's npub)
   ```
   Stored in SQLite. The bot now knows that DNS events from this npub should be served under `alice.nodns.shop` instead of `npub1abc123...nodns.shop`.

6. **DNS propagation**: When user publishes kind 11111 events, the bot:
   - Sees the npub has a custom name mapping
   - Pushes DDNS records for `alice.nodns.shop` instead of `npub1...nodns.shop`
   - Or both, if desired

### SQLite Schema Addition

```sql
CREATE TABLE custom_names (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT    NOT NULL UNIQUE,       -- "alice" (without .nodns.shop)
    npub        TEXT    NOT NULL,              -- hex pubkey
    order_id    TEXT    NOT NULL REFERENCES orders(id),
    created_at  INTEGER NOT NULL DEFAULT (unixepoch()),
    expires_at  INTEGER,                       -- NULL = no expiry for now
    
    CONSTRAINT name_format CHECK (name GLOB '[a-z0-9]*[a-z0-9]')
);

CREATE TABLE orders (
    id          TEXT    PRIMARY KEY,           -- UUID
    npub        TEXT    NOT NULL,
    name        TEXT    NOT NULL,
    amount_sats INTEGER NOT NULL DEFAULT 250,
    status      TEXT    NOT NULL DEFAULT 'pending',  -- pending/paid/expired/refunded
    cashu_token TEXT,                           -- raw token received
    created_at  INTEGER NOT NULL DEFAULT (unixepoch()),
    paid_at     INTEGER
);
```

## Security Considerations

### Payment Security
- **Token verification**: Always verify tokens with the mint, never trust client claims
- **Idempotency**: Same order can't be paid twice; same name can't be registered twice
- **Mint whitelist**: Only accept tokens from approved mints
- **Fee accounting**: Use NUT-02 `input_fee_ppk` to calculate actual received amount

### Name Security
- **Input validation**: Strict regex on name format, reject anything ambiguous
- **Reserved names**: Block `ns1`, `ns2`, `www`, `admin`, `mail`, `mx`, `api`, `bot`, `_nostr`, `_acme-challenge`, etc.
- **Rate limiting**: Max 3 registration attempts per npub per hour
- **Binding**: Name is cryptographically bound to npub — only that npub's events control the DNS

### Anti-Abuse
- **250 sats minimum**: Low enough to not be a real barrier, high enough to deter mass squatting
- **No speculation**: Names are for use, not resale (we control the namespace)
- **Revocation**: We can revoke names used for abuse (phishing, malware C2, etc.) per abuse philosophy

## Test Mint Setup (PoC)

For the proof of concept, we use testnuts:

1. **Mint URL**: `https://testnut.cashu.space`
2. **Characteristics**: FakeWallet, all Lightning invoices auto-paid, no real sats
3. **Getting test tokens**: Visit `cashu.exchange` → "Get Testnet Tokens" → select testnut mint
4. **Service config**: Set `ALLOWED_MINTS=["https://testnut.cashu.space"]` in bot config

## Comparison with nodns-nameserver

The `nodns-nameserver` repo on relay.ngit.dev takes a different approach:

| Aspect | Our Approach | nodns-nameserver |
|--------|-------------|-----------------|
| DNS serving | Knot DNS (production-grade) | Custom Go DNS server (miekg/dns) |
| Updates | DDNS (RFC 2136) | Direct in-memory cache |
| Relay | Subscribes to external relays | Embedded Khatru relay + BoltDB |
| Tag format | 11-element (real events) | 5-element (code) |
| Validation | Planned in bot | Strict local signature verification |
| Custom names | Cashu payment (this doc) | Not implemented |

**What to reuse from nodns-nameserver**:
- `_nostr` NULL validation record pattern (query `alice._nostr.nodns.shop` → get raw signed event)
- Strict signature verification before caching
- Subscription manager with reconnection logic

**What to keep different**:
- Knot DNS for production-grade serving (DNSSEC, AXFR, NOTIFY, RCU)
- DDNS for atomic updates (no DNS server restart risk)
- External relay subscriptions (don't embed a relay)

## NUT References

| NUT | Purpose | Priority |
|-----|---------|----------|
| NUT-00 | Core ecash protocol (blinded signatures, proofs) | Required |
| NUT-01 | Mint public keys (keysets) | Required |
| NUT-02 | Keyset evolution, fee calculation (`input_fee_ppk`) | Required |
| NUT-08 | Overpaid Lightning fee change | Nice-to-have |
| NUT-09 | Restore proofs after crash | Nice-to-have |
| NUT-11 | Pay-to-pubkey (P2PK) for locked refunds | Future |
| NUT-17 | WebSocket state notifications | Nice-to-have |
| NUT-20 | Signed mint quotes (bind to pubkey) | Future |
| NUT-23 | Bolt11 integration | Future |

## Implementation Order

1. **Go wallet setup**: Initialize `gonuts` wallet with testnut mint
2. **Order API**: `/register` + `/pay` endpoints
3. **Token verification**: Receive → verify with mint → mark paid
4. **SQLite tables**: `orders` + `custom_names`
5. **Bot integration**: Custom name lookup when processing kind 11111 events
6. **DDNS push**: Map custom names to npub records
7. **Reserved name list**: Hardcoded blocklist
8. **NWC rail** (optional): Add Lightning wallet payment as alternative
