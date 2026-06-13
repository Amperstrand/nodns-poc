# 23 — Lease and Renewal

> **Status**: DRAFT. Active experimentation. The lease model is fundamental to how NoDNS handles domain ownership.

## The Challenge

Domain names in the traditional system are leased, not owned. You register a domain for a period, and if you don't renew, you lose it. NoDNS embraces this model but asks: **how do we make lease renewal censorship-resistant?**

The key requirements:
1. **Operator-independent renewal**: If the user pays on time, the renewal is valid. The operator cannot refuse.
2. **Verifiable lease terms**: Anyone can see when a lease expires and what the renewal price is.
3. **Grace periods**: If a lease expires, there should be a window where the owner can still renew.
4. **Bounded promises**: The operator can only promise renewals up to their own domain lease expiry.

## Lease Lifecycle

```
                   registration event
                         │
                         ▼
    ┌─────────────────────────────────────┐
    │              ACTIVE                  │
    │  Owner can create/update/delete DNS  │
    │  records for this name               │
    │                                      │
    │  Renewals accepted at locked price   │
    └──────────────┬──────────────────────┘
                   │ lease expires
                   ▼
    ┌─────────────────────────────────────┐
    │              GRACE                   │
    │  DNS records still resolving         │
    │  Owner can ONLY renew (no changes)   │
    │                                      │
    │  Renewals accepted at locked price   │
    └──────────────┬──────────────────────┘
                   │ grace period expires
                   ▼
    ┌─────────────────────────────────────┐
    │             EXPIRED                  │
    │  DNS records removed                 │
    │  Name available for re-registration  │
    │                                      │
    │  No more renewals                    │
    └─────────────────────────────────────┘
```

Inspired by:
- **ENS**: 90-day grace period where names still resolve but only renewals are allowed
- **Traditional DNS (ICANN)**: Auto-renew grace → redemption → pending delete
- **NoDNS simplification**: Active → Grace → Expired. Three states, clear transitions.

## Censorship-Resistant Renewal

This is the core design challenge. How do we ensure the operator can't refuse a valid renewal?

### The Model: Deterministic Renewal

The renewal is **not** something the operator "accepts." It's a **deterministic state transition** that anyone can verify:

1. **User publishes renewal event**: A kind 11111 event with `["renewal", NAME, ZONE, NEW_VALID_UNTIL]` + Cashu payment tag
2. **Anyone can verify**: The event is on relays, the payment is provable, the price matches the locked price in the original registration event
3. **Operator's bot auto-processes**: If payment matches advertised price and name is currently owned by this npub, renewal is accepted. No human in the loop.
4. **If bot is down**: The renewal event sits on relays. When the bot comes back, it processes the backlog. The `created_at` on the renewal event is what matters, not when the bot processes it.

### The Critical Design Choice

**Renewal validity is determined by the event's `created_at`, not by when the bot processes it.** If the renewal event was published before the lease expired, it's valid — even if the bot was down for a week.

This means:
- The user's renewal event on relays IS the proof of renewal
- The operator's processing is just a convenience (writing to DNS)
- A NoDNS resolver would see the renewal event and extend the lease regardless of what the operator's bot did

### What Prevents the Operator From Cheating?

The operator controls the bot. They could program it to reject certain renewals. But:

1. **The event log is public**: Anyone can see the renewal event + payment proof
2. **The pricing is locked**: The registration event proves the agreed price
3. **Anyone can audit**: A third party can verify the operator is honoring renewals
4. **Reputation consequence**: If the operator rejects valid renewals, they become unreliable

This is **reputation-based enforcement**, not cryptographic enforcement. It's the same model that certificate authorities operate under: they're trusted, and if they misbehave, they get removed from trust stores.

## Operator's Own Lease as Upper Bound

The operator can only promise renewals up to their own domain lease expiry. If `nodns.shop` is registered until 2027-06-08:

- They can offer a 12-month lease to `alice.nodns.shop` starting 2026-06-08
- They can promise renewal at the locked price until 2027-06-08
- If they renew `nodns.shop` itself, they can extend the promise

If the operator fails to renew their own domain:
- They can't honor their promises
- They lose reputation
- Users migrate to a different operator

The registration event should include the operator's own lease expiry as a `VALID_UNTIL` ceiling on the promise.

## Renewal Event Format (Proposed)

```json
{
  "kind": 11111,
  "pubkey": "<owner-npub-hex>",
  "tags": [
    ["renewal", "alice", "nodns.shop", "1780704000"],
    ["cashu", "token...", "https://testnut.cashu.space", "24"]
  ]
}
```

Verification rules:
1. Signer must be the current owner of `alice.nodns.shop`
2. Payment must match the price locked in the original registration event
3. `created_at` must be before the current lease expiry (or within grace period)
4. New `VALID_UNTIL` must not exceed the operator's own lease expiry
5. Payment token must be unspent at the configured mint

## What We Landed On (For Now)

1. **Three-state lifecycle**: Active → Grace → Expired
2. **Deterministic renewal**: Operator's bot auto-accepts valid renewals. No human in the loop.
3. **`created_at` is the timestamp**: Renewal validity is based on when the event was published, not when processed.
4. **Price locked at registration**: Renewal price can't change after registration.
5. **Operator's own lease as upper bound**: Can't promise beyond your own domain expiry. Enforced via `operator_lease_expires` config field.
6. **Grace period**: Records still resolve during grace, only renewals allowed.
7. **Reputation enforcement**: Operator loses reputation if they reject valid renewals.
8. **DNS cleanup on expiry**: When a delegation expires, DNS records are removed from Knot via DDNS delete-rrset and soft-deleted in the SQLite store.
9. **Background expiry task**: Hourly task transitions Active → Grace (past `valid_until`) and Grace → Expired (past grace deadline).

## Implementation

### Configuration (`config.rs`)

```toml
[dns.zones.'nodns.shop'.lease]
grace_period_days = 30       # records still resolve, only renewals accepted
max_lease_days = 365         # max extension per renewal
operator_lease_expires = "2027-06-08"  # absolute ceiling on all leases
```

### Data Model (`types.rs`, `store.rs`)

- `DelegationState` enum: `Active`, `Grace`, `Expired`
- `DelegationRecord` stores: `valid_from`, `valid_until`, `renew_by`, `renewal_price`, `status`
- Store methods: `save_delegation_with_price`, `get_active_delegation`, `renew_delegation`, `mark_delegation_grace`, `mark_delegation_expired`, `get_delegations_past_valid_until`, `soft_delete_records_by_npub_zone`

### Event Processing (`event_processor.rs`)

- **Claim** (`["claim", NAME, ZONE, VALID_UNTIL]`): validates name availability, payment, `valid_until > now`, `valid_until <= operator_lease_expires`
- **Renewal** (`["renewal", NAME, ZONE, NEW_VALID_UNTIL]`): validates ownership, grace deadline, payment at locked price, extension limits, `new_valid_until <= operator_lease_expires`

### Background Expiry Task (`main.rs`)

Runs hourly. For each delegation past `valid_until`:
1. If past grace deadline → remove DNS records via DDNS, soft-delete in DB, mark expired
2. If still within grace and status is Active → mark grace

### Auth Enforcement (`auth.rs`)

- Expired delegations: DNS update requests rejected
- Grace delegations: only renewal events accepted, no DNS record changes

## Alternatives Considered

| Alternative | Pros | Cons | Decision |
|---|---|---|---|
| Operator manually approves renewals | Simple | Not censorship-resistant | Rejected |
| Smart contract (on-chain) | Cryptographic enforcement | Requires blockchain, complexity | Future option |
| Operator must cosign renewal | Extra assurance | Operator can refuse to cosign | Future option (optional) |
| Renewal via NIP-57 Zap | Public payment proof | Zap receipts aren't proof of payment | Rejected |
| OpenTimestamps for proof of timing | Strong time proof | Overkill for PoC | Deferred |

## Still Open

- ~~Whether operator cosigning is part of v1 or optional~~ → Deferred (not in v1)
- ~~How to handle renewals when operator's bot is down for extended period~~ → Resolved: `created_at` on the renewal event is authoritative; bot processes backlog on recovery
- ~~Whether renewal events need a separate kind or stay as kind 11111 tags~~ → Resolved: kind 11111 with `["renewal", ...]` tag
- Exact length of grace period — currently configurable per zone (default 30 days); needs real-world testing to tune
- OpenTimestamps integration for cryptographic proof of renewal timing — deferred
- On-chain (smart contract) enforcement — future option if demand warrants
