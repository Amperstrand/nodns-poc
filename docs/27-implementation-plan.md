# 27 — v1 Implementation Plan

> **Status**: ACTIVE. This is the implementation plan for the v1 payment and registration system.

## Scope

v1 implements two flows for nodns.shop:
1. **`$npub.nodns.shop`**: Free name claim, 2-sat Cashu antispam per DNS record creation
2. **`$string.nodns.shop`**: Paid registration via Cashu, auto-delegation, price locking, lease tracking

**No real value transfer**: Only accept Cashu tokens from mints with "testnut" in the URL.

## Dependency Graph

```
Phase 1: Per-zone pricing config + testnut filter
    │
    ├── Phase 2: Enable $npub antispam (record creation payment)
    │       │
    │       └── Phase 3: Frontend dynamic pricing
    │
    └── Phase 4: $string registration (claim + auto-delegation + price lock)
            │
            ├── Phase 5: Lease expiry + grace period
            │
            └── Phase 6: Renewal events
```

## Phase 1: Per-Zone Pricing Config + Testnut Filter

**Goal**: Replace global `PaymentConfig` with per-zone pricing. Add mint URL filter.

**Files to modify**:
- `src/config.rs` — Add `ZonePaymentConfig` to `ZoneConfig`, deprecate global `PaymentConfig`
- `src/payment.rs` — Make `Verifier` zone-aware, add `mint_filter` (accept only testnut mints)
- `src/event_processor.rs` — Pass per-zone payment config instead of global verifier
- `src/main.rs` — Create per-zone verifiers from zone configs

**Changes**:

### config.rs
```rust
// New struct inside ZoneConfig
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ZonePaymentConfig {
    pub enabled: bool,
    pub create_price: u64,      // sats per new record
    pub update_price: u64,      // sats per update
    pub delete_price: u64,      // sats per delete
    pub npub_names_free: bool,  // $npub names don't need payment for claim
    pub mint_url: String,       // accepted Cashu mint URL
    pub mint_filter: String,    // only accept mints matching this substring (e.g., "testnut")
}

// Defaults for nodns.shop:
// enabled=true, create_price=2, update_price=0, delete_price=0,
// npub_names_free=true, mint_url="https://testnut.cashu.space", mint_filter="testnut"
```

### payment.rs
```rust
pub struct Verifier {
    mint_url: String,
    create_price: u64,
    update_price: u64,
    delete_price: u64,
    npub_names_free: bool,
    mint_filter: Option<String>,  // If set, reject tokens from mints not matching
}

impl Verifier {
    pub fn new(zone_payment: &ZonePaymentConfig) -> Self { ... }

    // New: check if token mint matches filter
    fn mint_allowed(&self, token_mint: &str) -> bool {
        match &self.mint_filter {
            Some(filter) => token_mint.contains(filter),
            None => true,
        }
    }
}
```

### event_processor.rs
- Change from `verifier: Option<&Verifier>` to passing zone-specific verifier per zone iteration
- Each zone uses its own verifier (or None if payment disabled for that zone)

**Tests**:
- Config parsing: multi-zone with different payment configs
- Mint filter: accept testnut, reject non-testnut
- Backward compat: global PaymentConfig still works as fallback

---

## Phase 2: Enable $npub Antispam

**Goal**: Wire up payment verification for DNS record creation using per-zone pricing.

**Files to modify**:
- `src/payment.rs` — `check_event_payment` uses `create_price` instead of flat `required_sats`
- `src/event_processor.rs` — Pass zone-specific verifier per zone

**Key changes**:
- `check_event_payment` takes zone-specific pricing
- New records: charge `create_price` per new record
- Updates: charge `update_price` per updated existing record (0 for nodns.shop)
- Deletes: charge `delete_price` per delete (0 for nodns.shop)
- npub names: if `npub_names_free=true`, no payment needed for the name claim itself (but record creation still needs payment)

**Tests**:
- 2 sats per new record, 0 for update, 0 for delete
- Reject insufficient payment
- Reject tokens from non-testnut mints
- Accept tokens from testnut mints

---

## Phase 3: Frontend Dynamic Pricing

**Goal**: Frontend shows dynamic pricing, removes hardcoded mint URL and amount.

**Files to modify**:
- `nodns-frontend/src/lib/nostr.ts` — Remove hardcoded `testnut.cashu.space` and `250`
- `nodns-frontend/src/lib/constants.ts` — Add pricing defaults from API
- `nodns-frontend/src/components/dashboard.tsx` — Show dynamic price
- `src/handlers.rs` — New `/api/zones/{zone}/pricing` endpoint

**Backend endpoint**:
```
GET /api/zones/nodns.shop/pricing
→ { "create_price": 2, "update_price": 0, "delete_price": 0,
    "npub_names_free": true, "mint_url": "https://testnut.cashu.space",
    "mint_filter": "testnut" }
```

**Frontend changes**:
- Fetch pricing from API on load
- Show "2 sats required for new records" dynamically
- Use `mint_url` from API instead of hardcoded value
- Show "free" for updates and deletes

---

## Phase 4: $string Registration — Claim + Auto-Delegation + Price Lock

**Goal**: Users can register `alice.nodns.shop` by publishing a claim event with Cashu payment. Bot auto-signs delegation.

**New tag format**:
```
["claim", NAME, ZONE, VALID_UNTIL, RENEWAL_PRICE, RENEWAL_UNIT]
```

**Files to modify**:
- `src/types.rs` — New `ClaimRequest` struct
- `src/parser.rs` — Parse `claim` tags, validate name format
- `src/event_processor.rs` — New `process_claim` function
- `src/store.rs` — Check name availability, store registration with price lock

**Processing logic**:
1. Parse `claim` tag from kind 11111 event
2. Check name is available (no active delegation for that name+zone)
3. Verify Cashu payment matches required price (length-based: short names cost more)
4. Sign delegation event: `["delegation", "alice.nodns.shop", npub, NOW, VALID_UNTIL, RENEW_BY]`
5. Store delegation with locked pricing
6. Return success

**Price function** (for `$string` names):
```rust
fn registration_price(name: &str, zone_config: &ZoneConfig) -> u64 {
    match name.len() {
        1..=3 => zone_config.create_price * 100,  // premium short names
        4..=6 => zone_config.create_price * 10,   // moderate
        _ => zone_config.create_price * 2,         // standard lease
    }
}
```

**Tests**:
- Claim parsing and validation
- Name availability check
- Payment verification for different name lengths
- Price locking in stored delegation
- Reject claim for already-taken name
- Reject claim with insufficient payment

---

## Phase 5: Lease Expiry + Grace Period

**Goal**: Expired delegations enter grace period, then become available.

**Files to modify**:
- `src/store.rs` — Add `expires_at` tracking, query for expired delegations
- `src/main.rs` — Background task to check expiry periodically
- `src/auth.rs` — Grace period logic: during grace, only renewals allowed
- `src/event_processor.rs` — Reject DNS changes during grace, allow renewal

**Grace period**: 30 days (configurable per zone)

**Tests**:
- Delegation with future `valid_until` is active
- Delegation with past `valid_until` but within grace: records still resolve, only renewals
- Delegation past grace period: expired, name available

---

## Phase 6: Renewal Events

**Goal**: Users renew leases without operator intervention.

**New tag format**:
```
["renewal", NAME, ZONE, NEW_VALID_UNTIL]
```

**Files to modify**:
- `src/types.rs` — New `RenewalRequest` struct
- `src/parser.rs` — Parse `renewal` tags
- `src/event_processor.rs` — `process_renewal` function
- `src/auth.rs` — Verify renewal signer owns the name

**Processing logic**:
1. Parse `renewal` tag
2. Verify signer is current owner of `name.zone`
3. Verify `created_at` is before current lease expiry (or within grace)
4. Verify Cashu payment matches locked renewal price
5. Extend `valid_until` (bounded by operator's own lease)
6. Update delegation in store

**Tests**:
- Valid renewal extends lease
- Reject renewal from non-owner
- Reject renewal after grace period
- Reject renewal with wrong price
- Accept renewal during grace period

---

## Test Strategy

After each phase:
1. `cargo test` — all unit tests pass
2. `cargo build --release` — clean build
3. LSP diagnostics on changed files — no errors

After all phases:
- Full integration test: claim name → create records → verify → update → delete → renew → expire → re-register

## nodns.shop Production Config (when deployed)

```toml
[[dns.zones]]
zone = "nodns.shop"
knot_address = "127.0.0.1:53"
# ... TSIG config ...

[dns.zones.payment]
enabled = true
create_price = 2
update_price = 0
delete_price = 0
npub_names_free = true
mint_url = "https://testnut.cashu.space"
mint_filter = "testnut"

[dns.zones.lease]
grace_period_days = 30
operator_lease_expires = 2027-06-08  # when nodns.shop expires
```
