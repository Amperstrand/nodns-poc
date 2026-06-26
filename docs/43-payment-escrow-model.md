# 43 — Payment Escrow Model: Public Bid + P2PK Refund

> **Status**: DRAFT. Novel payment model for trustless namespace registration.

## Overview

Every nodns custom name registration is a **public bid** — the Cashu payment token is visible on the relay, P2PK-locked to the zone owner, with a time-limited refund condition. The zone owner must actively claim the token to confirm the registration. If they don't claim within the refund window, the user gets their sats back.

This creates a **trustless escrow** without any custodian or smart contract — just Cashu P2PK conditions and the passage of time.

## How It Works

```
1. User publishes kind 31111 event with:
   - Record tags (A, TXT, etc.)
   - ["cashu", "<p2pk-token>", "testnut.cashu.space", "2"]
   The Cashu token is P2PK-locked to the zone owner's npub with a 7-day refund.

2. Zone owner (bot) sees the event:
   a. Validates signature, authority, zone opt-in
   b. Verifies the Cashu token is P2PK-locked to their npub
   c. Verifies the refund conditions (7 days, correct refund key)
   d. Checks the name is available
   e. Pushes DDNS update → record goes live
   f. Claims the Cashu token (signs with zone nsec) → ACK

3. The claim is the acknowledgment:
   - Zone owner received payment = they honor the registration
   - The claim is visible on-chain (mint swap/melt operation)
   - Registration is confirmed

4. If zone owner IGNORES the event:
   - Record may go live (bot processes it)
   - But the token remains unclaimed
   - After 7 days, user reclaims via refund key
   - User gets sats back → registration may be reversed (bot policy)
```

## NUT-11 P2PK Conditions

The Cashu token's proof secrets contain P2PK spending conditions:

```json
[
  "P2PK",
  {
    "nonce": "<random-32-bytes-hex>",
    "data": "<zone-owner-compressed-pubkey>",
    "tags": [
      ["sigflag", "SIG_INPUTS"],
      ["locktime", "<unix-timestamp-7-days-from-now>"],
      ["refund", "<user-compressed-pubkey>"]
    ]
  }
]
```

| Field | Value | Meaning |
|---|---|---|
| `data` | `02{zone-npub-hex}` | Zone owner's compressed secp256k1 pubkey. Only they can claim before locktime. |
| `locktime` | `now + 604800` | Unix timestamp 7 days from now. After this, refund key can claim. |
| `refund` | `02{user-npub-hex}` | User's compressed secp256k1 pubkey. Can claim after locktime. |
| `sigflag` | `SIG_INPUTS` | Each proof must be individually signed. |

### Claim Windows

```
Day 0-7:   Zone owner can claim (sign with their nsec) → ACK
Day 7+:    User can reclaim (sign with their nsec) → REFUND
Never:     Anyone else can claim → NOTHING (they don't have either key)
```

## Why This Is Better Than Regular Payment

### Problem: Trust in Zone Operator

With regular (unlocked) Cashu tokens:
1. User includes token in event
2. Bot verifies token via checkstate (unspent)
3. Bot processes the record
4. Bot may or may not claim the token later
5. If the bot never claims, the token sits unclaimed forever
6. User has no recourse — sats are locked but not claimed

**The user must trust the zone operator to claim (honor) the payment.** There's no timeout, no refund.

### Solution: P2PK + Locktime Refund

With P2PK-locked tokens + refund:
1. User includes P2PK-locked token in event
2. Bot verifies P2PK lock targets zone npub
3. Bot processes the record
4. Bot claims the token (signs with zone nsec) = explicit ACK
5. If bot ignores: user reclaims after 7 days
6. **No trust needed** — either the zone operator claims (honors) or the user gets a refund

## Public Bidding Model

Because the Cashu token is visible on the relay (inside the kind 31111 event), every registration is a **public bid**:

```
Relay event:
  kind: 31111
  pubkey: user-npub
  tags:
    ["d", "A:alice.nodns.shop"]
    ["record", "A", "alice", "3600", "1.2.3.4"]
    ["cashu", "cashuA...", "testnut.cashu.space", "2"]
```

Anyone monitoring the relay can see:
- Who bid for "alice.nodns.shop"
- How much they paid (2 sats)
- The P2PK lock conditions (locked to zone owner, refund after 7 days)
- Whether the zone owner claimed it (check mint for swap operation)

### Competitive Bidding

If two users want "alice.nodns.shop":
1. Both publish kind 31111 events with their bids
2. Zone owner processes the first one (first-come-first-serve at bot level)
3. Zone owner claims the first bid's token = ACK
4. Second bid: name is taken, bot rejects
5. Second bidder reclaims their sats after locktime

Or, for a premium name:
1. User A bids 2 sats (minimum price)
2. User B bids 100 sats (premium bid)
3. Zone owner sees both, processes the higher bid
4. Claims User B's token, ignores User A's
5. User A reclaims after 7 days

This turns namespace registration into an **open auction** — visible, trustless, non-custodial.

### Anti-Squatting

The refund condition naturally prevents squatting:
1. Squatter locks 2 sats for thousands of names
2. Zone owner processes legitimate registrations first
3. Squatter's tokens expire after 7 days
4. Squatter reclaims all their sats → no cost, but also no squatting damage
5. Zone owner can prioritize legitimate registrations

## Implementation

### CLI (TypeScript) — Create P2PK Token

The `add` command automatically creates a P2PK-locked token when a custom name requires payment:

```bash
nodns add --type A --data 1.2.3.4 --name alice --sec nsec1... --refund-days 7
```

Internally, `src/lib/p2pk.ts` creates the token:

```typescript
const { token, refundDate, p2pk } = await createP2pkTokenWithRefund({
  zonePubkeyHex: zoneInfo.npub,     // from _nodns.{zone} TXT
  userPubkeyHex: kp.pubkey,         // from user's nsec
  refundAfterDays: refundDays,      // configurable via --refund-days (default: 7)
  amountSats: pricing.createPrice,  // from zone TXT
  mintUrl: zoneInfo.mintUrl,        // testnut.cashu.space
});

tags.push(["cashu", token, mintUrl, String(price)]);
```

The function checks the mint for NUT-11 support. If unsupported, it falls back to a regular unlocked token with a warning.

### CLI — Check/Reclaim Refund

```bash
# Check refund eligibility
nodns refund "cashuB..."

# Reclaim after locktime expires
nodns refund "cashuB..." --claim --sec nsec1...
```

The `refund` command:
1. Decodes the token and parses P2PK conditions
2. Shows locktime, locked-to pubkey, refund keys
3. If `--claim` and locktime expired: signs with user's nsec and receives the proofs back

### Bot (Rust) — Verify and Claim

```rust
// 1. Verify P2PK lock targets zone npub
let secret: Secret = parse_proof_secret(&proof.secret)?;
match secret.spending_conditions {
    P2PKConditions { data, conditions } => {
        if data.to_string() != zone_npub_compressed {
            return Err("P2PK lock doesn't target zone npub");
        }
        // Verify refund conditions
        if let Some(refund_key) = conditions.refund_keys.first() {
            // Refund key must be the event signer's pubkey
            if refund_key != event_signer_compressed {
                return Err("Refund key doesn't match event signer");
            }
        }
    }
    _ => return Err("Expected P2PK conditions"),
}

// 2. Process the record (DDNS update)
updater.update_record(&fqdn, ttl, rt, &rdata).await?;

// 3. Claim the token (sign proofs with zone nsec)
let signed_proofs = sign_proofs_p2pk(proofs, zone_nsec);
wallet.melt_or_swap(signed_proofs).await?;
// Token claimed = ACK = registration confirmed
```

### Refund (TypeScript) — Reclaim After Locktime

The `refund` command in `src/commands/refund.ts` handles both checking and claiming:

```typescript
const result = checkRefundEligibility(token, userPubkeyHex);

if (!result.eligible) {
  console.error(`Refund not yet available. Eligible in ~${days} day(s).`);
} else if (wantClaim) {
  const newToken = await reclaimExpiredToken(token, userPrivkeyHex, mintUrl);
  console.log(newToken);
}
```

## Configuration

### Zone TXT Extension

The zone TXT record can optionally advertise the refund window:

```
_nodns.nodns.shop TXT "v=2;npub=7eff...;create=2;refund_days=7;..."
```

This tells clients the zone owner's expected refund window. Clients can use a shorter or longer window if they prefer.

### CLI Options

```
nodns add --type A --data 1.2.3.4 --name alice \
  --refund-days 7       # Refund after 7 days (default)
  --sec nsec1...         # User's secret key

nodns refund "cashuB..."                # Check refund eligibility
nodns refund "cashuB..." --claim        # Reclaim after locktime
nodns refund "cashuB..." --claim --sec nsec1...
```

## Security Properties

1. **Non-custodial**: No third party holds the sats. They're locked in the Cashu token.
2. **Time-bounded**: The locktime ensures the user can always recover their funds.
3. **Publicly verifiable**: The bid, lock conditions, and claim are all on-chain/on-relay.
4. **Zone owner sovereignty**: The zone owner decides which bids to accept (claim).
5. **User protection**: If the zone operator disappears, the user gets a full refund.
6. **Anti-spam**: Locking sats is a cost — spamming bids locks up the spammer's funds.

## Prior Art

| System | Mechanism | nodns improvement |
|---|---|---|
| ENS (Ethereum) | Smart contract escrow + gas | No gas, no smart contract — Cashu P2PK |
| Traditional domains | Registrar holds funds | No custodian — P2PK conditions |
| Lightning hold invoices | Payment held until settlement | Cashu tokens are stateless, no channel needed |

## Future Extensions

1. **Dynamic pricing**: Zone owner adjusts `create_price` based on demand
2. **Lease renewal**: Annual P2PK payments with escalating prices for premium names
3. **Multi-sig zones**: Multiple zone operators must all claim (MuSig2)
4. **Burn instead of refund**: Instead of refunding, burn the sats (proves commitment)
5. **Proof-of-burn gating**: Only accept bids from pubkeys with prior burns

## References

- [NUT-11: P2PK](https://cashubtc.github.io/nuts/11/) — Cashu P2PK specification
- [docs/36-anti-spam-research.md](36-anti-spam-research.md) — Anti-spam research
- [docs/39-protocol-v2-design.md](39-protocol-v2-design.md) — Protocol v2 with P2PK design
