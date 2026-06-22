# 36 — Anti-Spam Research

> **Status**: DRAFT. Research notes, tabled for later. No implementation planned yet.

## Problem

NoDNS faces two spam surfaces:

1. **Custom names** (`alice.nodns.shop`) — already protected by Cashu payments. Spammer must pay per registration. Solved.

2. **npub-derived names** (`npub1xxx.nodns.shop`) — currently free (`npub_names_free = true`). A spammer can generate unlimited npubs, each publishing unlimited events (within rate limits). Not solved.

The rate limiter (5 events/window per npub) and record cap (20 records per subdomain) slow spammers but don't stop them — generating fresh npubs is free.

## Current Mechanisms

| Mechanism | Scope | Effectiveness |
|---|---|---|
| Rate limiting (5 events/npub/window) | Per-npub | Weak — trivially bypassed by rotating npubs |
| Record cap (20 records/subdomain) | Per-subdomain | Moderate — limits damage per name |
| Private IP blocking | A/AAAA records | Strong for IP squatting, irrelevant for TXT spam |
| Record type whitelist | All events | Strong — limits attack surface to A/AAAA/CNAME/TXT/MX |
| TXT length cap (512 chars) | TXT records | Moderate — limits payload size |
| Cashu payment for custom names | Custom names only | Strong — real economic cost |
| Cashu payment for npub names | Configurable (`npub_names_free`) | Disabled — npub names are free by design |

## Approaches Considered

### 1. NIP-13: Proof of Work

**How it works:** The event publisher computes SHA-256 hashes varying a nonce tag until the event ID has N leading zero bits. The bot verifies by counting leading zero bits — trivially cheap.

**NIP-13 tag format:**
```json
["nonce", "<target_difficulty>", "<attempts_tried>"]
```

**nostr-tools support:** `nip13.minePow(event, difficulty)` computes client-side. `nip13.getPow(eventId)` returns the difficulty of an event.

**Pros:**
- Zero economic cost for legitimate users (one-time computation per event, seconds at difficulty 16-20)
- No third-party dependency (no mint, no Lightning)
- Already standardized (NIP-13)
- Client-side computation, bot-side verification is instant

**Cons:**
- ASIC/GPU asymmetry — miners can compute PoW orders of magnitude faster than browsers
- Doesn't scale with inflation — today's "hard" difficulty is trivial in 5 years
- User experience: browser freezes during mining (mitigated with Web Workers)
- No economic finality — PoW is temporary, not permanently costly like burning value

**Implementation sketch:**
- Bot: reject kind 11111 events with PoW below threshold (e.g., 16 bits for npub names, 0 for paid names)
- Registrar: compute PoW client-side in a Web Worker before publishing
- Config: `policy.min_pow_npub = 16`, `policy.min_pow_custom = 0` (custom names already pay)

### 2. Proof of Burn (ThomasV + origami74)

**Who:** ThomasV (Thomas Voegtlin, creator of Electrum wallet) and origami74 designed and implemented a proof-of-burn anti-spam mechanism for Nostr.

**How it works:** The user sends a small amount of bitcoin to a provably unspendable address (OP_RETURN, `1HuML...` etc.). The transaction permanently destroys the value. The user attaches the transaction proof to their Nostr event as a tag. The verifier checks that:
1. The transaction exists on-chain
2. It sends to a known burn address (or uses OP_RETURN)
3. The burned amount meets the threshold
4. The transaction was made by the same pubkey (or links to the npub)

**Proof structure (conceptual):**
```json
["burn", "<tx_hash>", "<amount_sats>", "<block_height>", "<merkle_proof>"]
```

**Pros:**
- Real economic cost — burned BTC is permanently destroyed, not just temporarily locked
- No ASIC asymmetry — burning costs the same for everyone (1 sat = 1 sat)
- SPV-verifiable — the bot doesn't need a full node, just an SPV proof or an Electrum server query
- Scales with Bitcoin value — as BTC appreciates, the anti-spam cost automatically increases
- ThomasV's Electrum infrastructure can serve as verification backend

**Cons:**
- Requires on-chain Bitcoin transaction — slow (confirmations), expensive (fees) for small amounts
- Not practical for micro-burns (< 1000 sats) due to transaction fees
- User experience: needs a Bitcoin wallet, not just a Cashu wallet
- Verification latency — bot must wait for block confirmation or trust 0-conf
- Irreversible — user can't get burned sats back even if registration fails

**Relationship to Cashu:** Cashu already provides sybil resistance with better UX (instant, free test sats, no on-chain interaction). Proof of burn would be a stronger guarantee for high-value namespaces (e.g., `.cv` domains) where real economic commitment matters.

**Implementation sketch:**
- Not practical for the nodns pilot (on-chain fees too high for test registrations)
- Could be layered on top for production registrations on `.cv` or custom premium names
- Verification via Electrum server (ThomasV's infrastructure) or a Bitcoin RPC node

### 3. Cashu Micro-Payment for npub Names

**How it works:** Even npub-derived names require a minimal Cashu token (1 testnut sat). The token is included in the event and verified by the bot via CDK checkstate.

**Pros:**
- Already implemented — the bot has full Cashu verification infrastructure
- Test sats are free — no real economic cost for legitimate users
- Requires Cashu wallet setup — automated spam bots need to implement Cashu minting
- Instant verification (CDK checkstate is a single HTTP call)

**Cons:**
- Test mints could be abused — if the mint has no rate limiting, a spammer can mint unlimited test sats
- Still free for spammers who set up their own Cashu wallet
- Doesn't distinguish between "legitimate test user" and "spammer using test sats"

**Implementation sketch:**
- Config: `npub_names_free = false` for nodns.shop zone
- Minimum payment: 1 sat for npub names, current price for custom names
- Registrar: always include a Cashu token in kind 11111 events

### 4. Relay-Level Moderation

**How it works:** The relay (relay.cashu.email) filters or rate-limits kind 11111 events at the relay level. Events that don't meet the relay's policy are rejected before reaching the bot.

**Pros:**
- No bot-side changes needed
- Relay can implement arbitrary policies (POW, payments, reputation, manual approval)
- Spam never reaches the Nostr network

**Cons:**
- Centralizes trust in the relay — defeats Nostr's decentralized model
- Users on other relays bypass the filter
- Only works if the bot subscribes exclusively to moderated relays

### 5. Reputation / Vouching

**How it works:** Established npubs vouch for new npubs. The bot accepts kind 11111 events only from npubs with sufficient vouches. Vouches are themselves Nostr events.

**Pros:**
- Social sybil resistance — based on trust network, not computation or money
- Aligns with Nostr's social model

**Cons:**
- Bootstrap problem — how do the first users get vouched?
- Complex to implement and maintain
- Can create exclusion / gatekeeping

## Comparison

| Approach | Cost to Spammer | Cost to User | UX Friction | Implementation Effort |
|---|---|---|---|---|
| NIP-13 PoW | Compute time (seconds/event) | CPU cycles | Browser mining | Low (nostr-tools has it) |
| Proof of Burn | Real BTC (permanent loss) | Small BTC amount | Bitcoin wallet + on-chain tx | High (Bitcoin RPC/Electrum) |
| Cashu micro-payment | Test sats (free but friction) | Free test sats | Wallet top-up | Zero (already built) |
| Relay moderation | None (if relay accepts) | None | None | Low (relay config) |
| Reputation | Social capital | None | Find a voucher | High (protocol design) |

## Recommendation (TABLED)

For the pilot phase, the existing Cashu payment for custom names plus rate limiting is sufficient. The spam surface (free npub-derived names) is low-risk because:
- npub-derived names are long and ugly (`npub1abc...xyz.nodns.shop`)
- They're not useful for phishing or typosquatting
- Rate limiting + record cap limits the blast radius

When anti-spam becomes necessary, the recommended path is:

1. **Short-term (easy win):** Require Cashu micro-payment (1 test sat) for ALL names, including npub-derived. Flip `npub_names_free = false`. Zero new code — just a config change.

2. **Medium-term (better UX):** Add NIP-13 PoW as an alternative to Cashu. Users can either pay 1 sat OR mine 16 bits of PoW. This gives UX choice: instant payment vs. free-but-slow.

3. **Long-term (for .cv and premium names):** Proof of Burn for high-value namespaces where real economic commitment is required. Verification via Electrum server or Bitcoin RPC.

## References

- NIP-13 (Proof of Work): https://github.com/nostr-protocol/nips/blob/master/13.md
- nostr-tools PoW: `nip13.minePow()`, `nip13.getPow()`
- ThomasV Electrum: https://electrum.org
- Proof of Burn concept: https://en.bitcoin.it/wiki/Proof_of_burn
- Existing nodns anti-spam: `event_processor.rs` rate limiting, `parser.rs` validation, `payment.rs` Cashu verification
- Related docs: [07-abuse-philosophy.md](07-abuse-philosophy.md), [11-protocol-experimental-draft.md](11-protocol-experimental-draft.md), [22-pricing-and-payments.md](22-pricing-and-payments.md)
