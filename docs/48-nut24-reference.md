> **Status**: ACTIVE
> **Created**: 2026-07-11

# NUT-24 Reference Implementation: Cashu-Gated DNS

nodns is one of the first production NUT-24 (HTTP 402 Payment Required) services
and the **only subscription-based** implementation in the Cashu ecosystem. This
document explains the pattern for other builders who want to gate HTTP services
behind Cashu ecash payments.

## The subscription model (nodns innovation)

All other NUT-24 services (cashu-proxy, proxnut, otrta, deez-cashus) use
**per-request** payments — each HTTP request requires a fresh Cashu token. This
works for pay-per-article or pay-per-API-call but is impractical for services
with high query volume (like DNS, where a single page load triggers 20-50
queries).

nodns introduces a **subscription model**:
1. User pays once (10 testnut sats)
2. Server verifies the Cashu token via CDK `checkstate`
3. Server issues an opaque subscription token (UUID, stored in SQLite)
4. Client uses the subscription token for all subsequent requests (30 days)
5. Per-subscription rate limit (10,000 queries/day) prevents abuse

This separates the **payment event** (one Cashu token, one verification) from
the **service usage** (many requests, lightweight token validation). The payment
happens once; the service runs for the subscription period.

## Payment flow

### Step 1: Challenge (HTTP 402)

```
POST /api/resolver/subscribe
(no X-Cashu header)
```

Server responds:
```
HTTP/1.1 402 Payment Required
X-Cashu: creqA<Base64(CBOR({a: 10, u: "sat", m: ["https://testnut.cashu.space"], d: "nodns resolver subscription"}))>
Content-Type: application/json

{
  "error": "payment required",
  "accepts": {
    "cashu": { "mint": "https://testnut.cashu.space", "amount": 10, "unit": "sat" }
  },
  "instructions": "Retry with X-Cashu header containing a valid Cashu token"
}
```

The `X-Cashu` header contains a NUT-18 payment request encoded as `creqA`
(CBOR + base64_urlsafe). The fields:
- `a`: amount in sats (10)
- `u`: unit ("sat")
- `m`: accepted mint URLs (testnut only — enforced by `mint_filter`)
- `d`: human-readable description

### Step 2: Payment (client retries with Cashu token)

```
POST /api/resolver/subscribe
X-Cashu: cashuB<Base64(CBOR(Token with proofs worth 16 sats))>
```

### Step 3: Verification + subscription creation

Server:
1. Decodes the Cashu token (CDK `Token::from_str`)
2. Checks mint URL matches config (`mint_filter = "testnut"`)
3. Checks amount ≥ required price (10 sats)
4. Calls CDK `/v1/checkstate` on the mint — verifies all proofs are unspent
5. Creates a `resolver_subscriptions` row in SQLite (UUID token, 30-day expiry)
6. Returns the subscription token

```
HTTP/1.1 200 OK
Content-Type: application/json

{
  "token": "b630fa3e-2313-42ae-8748-c17b5376c5a6",
  "expires_at": 1786260939,
  "daily_query_limit": 10000,
  "doh_endpoint": "https://dns.nodns.shop/dns-query"
}
```

### Step 4: Service usage (subscription token, not Cashu)

All subsequent requests use the opaque subscription token, NOT a Cashu token:
```
POST /dns-query/premium
X-Subscription: b630fa3e-2313-42ae-8748-c17b5376c5a6
```

The server validates the subscription (exists, not expired, under daily limit)
and increments a counter. No Cashu verification per request — that already
happened at subscribe time.

## Why NUT-24 (not x402)

NUT-24 is the Cashu-native HTTP 402 standard. The Coinbase x402 spec uses
`PAYMENT-REQUIRED` / `PAYMENT-SIGNATURE` headers and is USDC/EVM-focused.
NUT-24 uses the simpler `X-Cashu` header and is Cashu-native.

nodns follows NUT-24 because:
- Cashu-native (the payment system nodns already uses for DNS records)
- Simpler protocol (two headers, one round-trip)
- Supported by cashu-ts (the reference Cashu TypeScript library)
- Compatible with Nutpay (Chrome extension for auto-paying 402 responses)

## Wallet compatibility

### Nutpay (Chrome extension)

**nodns is Nutpay-compatible with zero code changes.** Nutpay intercepts all
`fetch()` calls, detects HTTP 402 responses with `X-Cashu` headers, and
auto-pays using the user's Cashu wallet. The flow:

1. User installs Nutpay Chrome extension
2. Loads it with testnut tokens (from faucet.cashu.email)
3. Visits dns.nodns.shop, clicks "Subscribe"
4. Nutpay detects the 402 + `X-Cashu: creqA...` response
5. Parses the NUT-18 payment request (amount, mint, unit)
6. Pays with testnut tokens, retries with `X-Cashu: cashuB...`
7. Server verifies and returns subscription token

### Manual flow (no extension)

Users without Nutpay manually:
1. Get tokens from faucet.cashu.email
2. Paste the token into the dns.nodns.shop subscribe form
3. The form sends it as the `X-Cashu` header

### Other wallets

Cashu wallet libraries (`@cashu/cashu-ts`, `coco-cashu-core`) provide the
primitives for NUT-24 but don't auto-pay. Developers building custom clients
can use these libraries to parse the `creqA` challenge and construct the
`cashuB` payment token.

## Comparison with other NUT-24 services

| Feature | nodns | cashu-proxy | nutpay | otrta |
|---|---|---|---|---|
| Model | **Subscription** (30 days) | Per-request | Per-request (auto) | Per-request |
| Payment | Cashu (NUT-24) | Cashu (NUT-24) | Cashu (NUT-24) | Cashu (NUT-24) |
| X-Cashu header | ✅ | ✅ | ✅ | ✅ |
| creqA format | ✅ | ✅ | ✅ | ✅ |
| P2PK locking | Optional | ✅ | ✅ | ❌ |
| CDK checkstate | ✅ | ✅ | N/A (client) | ✅ |
| Use case | DNS resolution | API gateway | Web paywall | AI APIs |

## Implementation reference

### Server-side (Rust, using CDK)

```rust
// 1. Build the NUT-18 payment request for the 402 challenge
fn build_creq_a(amount: i64, mint_url: &str, description: &str) -> String {
    // CBOR-encode {a: amount, u: "sat", m: [mint_url], d: description}
    // Base64-urlsafe encode with "creqA" prefix
}

// 2. Verify the client's Cashu token (on retry)
let verifier = Verifier::new(mint_url, mint_filter, price_sats);
match verifier.verify_payment(cashu_token, price_sats).await {
    Ok(amount) => {
        // Token is valid, proofs are unspent
        // Create subscription, return opaque token
    }
    Err(e) => {
        // Invalid token, wrong mint, insufficient amount, or double-spent
        return (StatusCode::BAD_REQUEST, Json(json!({"error": e}))).into_response();
    }
}
```

### The subscription token (not Cashu)

After Cashu verification, the subscription token is a random UUID stored in
SQLite. It is NOT a Cashu token — it's an opaque credential. This means:
- No per-request Cashu verification (fast)
- No per-request mint round-trip (no latency)
- Rate limiting per subscription (counter in SQLite)
- Expiry checked per request (timestamp comparison)

## References

- [NUT-24: HTTP 402 Payment Required](https://github.com/cashubtc/nuts/blob/main/24.md)
- [NUT-18: Payment Requests](https://cashubtc.github.io/nuts/18/)
- [Nutpay Chrome Extension](https://github.com/babdbtc/nutpay)
- [cashu-proxy](https://github.com/thesimplekid/cashu-proxy)
- [cashu-ts](https://cashu-ts.dev/)
- [coco-cashu](https://cashubtc.github.io/coco/)
- [402 for Dummies](https://402fordummies.dev/)
