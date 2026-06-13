# 22 — Pricing and Payments

> **Status**: DRAFT. Active experimentation. All pricing is experimental and subject to change.

## The Challenge

Domain pricing needs to balance several concerns:
- **Anti-spam**: Making it expensive enough to prevent abuse
- **Accessibility**: Making it cheap enough that anyone can use it
- **Operator sustainability**: Covering infrastructure costs (DNS, DNSSEC, storage)
- **Price stability**: Users need to know what they'll pay for renewals
- **Flexibility**: Different operators should be able to set different prices

## Per-Operator Pricing Policies

NoDNS is designed so that **each zone operator sets their own pricing**. The operator for `nodns.shop` can run a cheap PoC with test tokens, while a TLD operator could charge real Bitcoin for domain leases.

### What's Configurable Per Zone

| Parameter | Description | Example (nodns.shop PoC) |
|---|---|---|
| `create_price` | Sats per new DNS record creation | 2 sats |
| `update_price` | Sats per record update | 0 sats (free) |
| `delete_price` | Sats per record deletion | 0 sats (free) |
| `npub_names_free` | Whether `$npub.tld` names are free | true |
| `lease_price` | Sats per time period for delegated names | 2 sats/month |
| `mint_url` | Which Cashu mint to accept | `testnut.cashu.space` |
| `accept_testnut` | Whether to accept worthless test tokens | true |

### How Pricing Is Published

The operator publishes their pricing as a Nostr event:

```
["price_schedule", ZONE, JSON_PRICING, VALID_UNTIL]
```

Where `VALID_UNTIL` is bounded by the operator's own domain lease expiry. The operator cannot promise pricing beyond their own lease.

## Price Locking at Registration

**The registration event IS the contract.** When a user registers `alice.nodns.shop`, the event records:
- The lease price
- The renewal price
- The lease duration
- The maximum renewal date (bounded by operator's own lease)

The operator **cannot change the renewal price after registration**. They can change pricing for new registrations, but existing registrations keep their locked price.

This is enforced by the event log: anyone can verify that a renewal payment matches the price locked in the original registration event.

### Why Price Locking Matters

Without price locking, the operator could:
1. Offer cheap registration (2 sats/month)
2. Wait until the domain is established and valuable
3. Raise the renewal price to 10,000 sats/month
4. Effectively hold the domain hostage

With price locking, the registration event becomes an immutable contract. The operator promised a price, and the Nostr event proves it.

## Payment Methods

### Cashu (Current — For PoC)

Cashu (Chaumian e-cash) is used for the proof-of-concept:
- **Mint**: `testnut.cashu.space` (worthless test tokens, but proves the flow)
- **Token amounts**: 1-10 sats for antispam, higher for leases
- **Verification**: Bot checks token against mint's `/v1/checkstate` endpoint
- **NUT-20**: Can bind mint quote to a pubkey (prevents quote theft)

Cashu economics for `nodns.shop` PoC:
- `testnut.cashu.space` charges `input_fee_ppk: 100` (roughly 1 sat per transaction)
- A 2-sat payment costs 1 sat in fees = net 1 sat to operator
- Acceptable for a PoC with worthless tokens

### Lightning Zaps (Not Used)

NIP-57 zap receipts are **explicitly not proof of payment** (stated in NIP-57 spec). They only prove someone fetched an invoice. Cashu tokens provide stronger verification because the mint confirms the tokens are unspent.

### Proof of Burn (Future)

ThomasV (Electrum creator) built a notary service that converts Lightning zaps into on-chain Bitcoin miner fees via CLTV-locked outputs. This is philosophically aligned with NoDNS's Bitcoin-native approach but adds significant complexity (Lightning + on-chain transactions).

**Decision**: Deferred to future research. Cashu is simpler and already wired up.

### NIP-13 Proof of Work (Future — Anti-Spam Only)

NIP-13 defines a `nonce` tag for Nostr events that allows clients to mine computational difficulty before publishing. The event ID must have a configurable number of leading zero bits.

```json
{
  "tags": [["nonce", "776797", "20"]],
  "content": "..."
}
```

**Properties**:
- No payments, no infrastructure, no third parties
- CPU cost deters spam (difficulty 20 = ~1M hashes per event)
- Verifiable by anyone (count leading zeros in event ID)
- Complementary to Cashu — could require PoW for free-tier, Cashu for paid tier

**Limitations**:
- Not a payment mechanism — no value transferred
- ASIC/GPU miners have advantage over browser-based miners
- Difficulty arms race (what's expensive for a spammer today?)

**Decision**: Deferred. Cashu antispam is the primary mechanism for v1. PoW could complement it as a free-tier filter in a future iteration where we want frictionless onboarding (no wallet needed, just CPU work).

## How Payment Proofs Work

1. User publishes a Nostr event with a `["cashu", TOKEN, MINT_URL, AMOUNT]` tag
2. The bot decodes the token, verifies it against the configured mint
3. The bot calls `/v1/checkstate` to confirm all proofs are unspent
4. The bot verifies the token amount covers the required price
5. If valid, the event is processed. If not, it's rejected.

The payment proof is part of the public Nostr event. Anyone can independently verify that:
- The payment was made
- The amount was correct
- The mint confirmed it was unspent at the time

## nodns.shop Testnut-Only Policy

For the nodns.shop PoC, **no real value is transferred**. We only accept Cashu tokens from mints that have "testnut" in their URL. This means:

- `testnut.cashu.space` ✓ accepted (test tokens, no real value)
- `nofees.testnut.cashu.space` ✓ accepted (test tokens, no fees)
- `stablenut.cashu.network` ✗ rejected (real value)
- `mint.minibits.cash` ✗ rejected (real value)

This is enforced in the bot's mint URL filter. The purpose is to demo the full payment flow without requiring users to obtain real Bitcoin.

## What We Landed On (For Now)

1. **Cashu-only** for the PoC. Lightning/PoB as future options.
2. **Per-zone pricing** — each operator configures their own.
3. **Price locking** — registration event locks renewal price.
4. **Testnut-only** for nodns.shop — only accept tokens from mints with "testnut" in the URL. No real value transfer.
5. **2 sats per record** for nodns.shop PoC (cheap enough to be accessible, expensive enough to deter spam).
6. **Free updates and deletes** — encourage corrections and cleanup.
7. **Two flows for v1**:
   - **`$npub.nodns.shop`**: Free to claim, 2 sats per DNS record creation (antispam), free updates/deletes
   - **`$string.nodns.shop`**: Paid registration via Cashu (legacy-style), price locked at registration, renewal at locked price

## Alternatives Considered

| Alternative | Pros | Cons | Decision |
|---|---|---|---|
| Flat global pricing | Simple | Doesn't scale to multiple operators | Rejected |
| NIP-57 Zaps | Familiar Nostr pattern | Not proof of payment (per spec) | Rejected |
| Proof of Burn | Philosophically pure | Requires Lightning + on-chain | Deferred |
| Fixed pricing (no per-zone) | Simple | Can't adapt to different markets | Rejected |
| Subscription-based | Predictable revenue | UX complexity | Future option |
