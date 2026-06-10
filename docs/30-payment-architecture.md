# Payment Architecture: Cashu P2PK to Registrar

> **Status**: Design doc. Not yet implemented.

## Problem

Users register DNS records via Nostr events. To prevent spam and fund operations, registrations require payment. We need a trust-minimized flow where:

1. The user pays the registrar (nodns.shop) directly
2. Payment is verifiable on Nostr
3. The registrar cryptographically confirms the registration
4. The link between Nostr identity and DNSSEC is provable

## Architecture Overview

```
User (browser)                    Nostr Relays                   Registrar (bot)
     │                                │                              │
     │  1. Publish kind:11111         │                              │
     │  with P2PK-locked cashu        │                              │
     │───────────────────────────────>│                              │
     │                                │  2. Bot subscribes           │
     │                                │  to kind:11111               │
     │                                │<─────────────────────────────│
     │                                │                              │
     │                                │  3. Bot receives event       │
     │                                │─────────────────────────────>│
     │                                │                              │
     │                                │              4. Verify P2PK  │
     │                                │                 lock to our   │
     │                                │                 pubkey        │
     │                                │                              │
     │                                │              5. Spend tokens │
     │                                │                 (unlock)      │
     │                                │                              │
     │                                │              6. DDNS UPDATE  │
     │                                │                 (create DNS)  │
     │                                │                              │
     │                                │  7. Publish kind:11111       │
     │                                │     confirmation              │
     │                                │<─────────────────────────────│
     │  8. See confirmation           │                              │
     │<───────────────────────────────│                              │
```

## Component 1: Cashu P2PK Payment (NUT-11)

### What is P2PK?

Pay-to-Public-Key (NUT-11) locks Cashu ecash tokens to a specific ECC public key. Only the holder of the corresponding private key can spend (redeem) the tokens. The mint enforces this at the protocol level.

### Why P2PK?

| Property | P2PK | Regular token |
|---|---|---|
| Who can spend | Only registrar (key holder) | Anyone who has the token |
| Token posted publicly | Safe — locked | Dangerous — stealable |
| Verifiable on Nostr | Yes — lock is in the proof | No |
| Mint enforces | Yes — rejects unauthorized spend | N/A |

This means users can post payment tokens in Nostr events without fear of theft. Only the registrar can redeem them.

### P2PK Secret Structure

The token's `Proof.secret` contains a NUT-10 JSON structure:

```json
[
  "P2PK",
  {
    "nonce": "<random-32-bytes-hex>",
    "data": "<registrar-compressed-pubkey-33-bytes-hex>",
    "tags": [["sigflag", "SIG_INPUTS"]]
  }
]
```

The `data` field is the registrar's secp256k1 compressed public key (33 bytes, `02` or `03` prefix). The mint verifies a Schnorr signature on `Proof.secret` before allowing the spend.

### Creating P2PK-Locked Tokens (Frontend)

Using `cashu-ts` `P2PKBuilder`:

```typescript
import { P2PKBuilder } from '@cashu/cashu-ts';

const p2pk = new P2PKBuilder()
  .addLockPubkey(registrarPubkey)  // 33-byte compressed hex
  .toOptions();

const { keep, send } = await wallet.ops
  .send(250, userProofs)
  .asP2PK(p2pk)
  .run();

const token = getEncodedToken({ proofs: send, mint: MINT_URL });
```

### Spending P2PK-Locked Tokens (Bot / Rust)

The bot uses its nsec (private key) to sign the proof secret, providing the witness:

```rust
// Bot holds the nsec corresponding to registrarPubkey
// When processing a registration event:
// 1. Extract proofs from the cashu tag
// 2. Check that proof.secret contains P2PK lock to our pubkey
// 3. Sign each proof.secret with our nsec (Schnorr signature)
// 4. Swap/melt the tokens at the mint
```

The CDK crate (already used in `payment.rs`) supports P2PK witness signing.

### Mint Compatibility

The mint must support NUT-11 (indicated via NUT-06 info endpoint):

```json
{
  "11": { "supported": true }
}
```

`nofee.testnut.cashu.space` must be checked for NUT-11 support. If not supported, we need a mint that does, or we fall back to regular tokens (less secure — token must not be posted publicly).

## Component 2: Nostr Confirmation Event

After the bot successfully processes a paid registration, it publishes a confirmation event on Nostr. This creates a verifiable, timestamped receipt.

### Event Format

```json
{
  "kind": 11111,
  "pubkey": "<registrar-hex-pubkey>",
  "created_at": 1718012345,
  "tags": [
    ["registrar", "confirm", "alice.nodns.shop", "1735689600"],
    ["payment", "verified", "250", "sat"],
    ["mint", "https://nofee.testnut.cashu.space"],
    ["p", "<user-hex-pubkey>"],
    ["e", "<original-registration-event-id>", "", "reply"]
  ],
  "content": "",
  "sig": "<registrar-signature>"
}
```

### Tag Breakdown

| Tag | Purpose |
|---|---|
| `["registrar", "confirm", domain, expiry]` | Confirms registration valid until Unix timestamp `expiry` |
| `["payment", "verified", amount, unit]` | Attests that payment was received and verified |
| `["mint", url]` | Which mint the payment was on |
| `["p", user-pubkey]` | The user who registered (Nostr mention) |
| `["e", event-id, relay, "reply"]` | References the original registration event |

### Why kind:11111?

We already use kind:11111 for all NoDNS protocol events. Adding a `["registrar", "confirm", ...]` tag creates a clean semantic: the registrar is publishing a protocol event that confirms a user's registration. Clients can filter for `{"kinds": [11111], "#registrar": ["confirm"]}` to find all confirmations.

### Alternative: NIP-58 Badges

NIP-58 defines `kind:30009` (Badge Definition) and `kind:8` (Badge Award). We could define a "nodns.shop Domain Holder" badge and award it upon registration. This is more visible in Nostr clients but overloads the badge metaphor. Better to keep it in our existing kind:11111 namespace.

### Alternative: NIP-61 Nutzaps

NIP-61 defines `kind:9321` (Nutzap) for sending ecash on Nostr and `kind:10019` for payment routing info. This is the standard Cashu-on-Nostr pattern. We could adopt it for the payment leg while using kind:11111 for the registration/confirmation leg. This separates the payment from the registration protocol.

**Recommendation**: Use kind:11111 for confirmation. Consider NIP-61 for the payment transport in a future iteration.

## Component 3: Registrar Identity

### The Triple-Purpose Key

The registrar generates a single secp256k1 keypair (nsec/npub) that serves three purposes:

```
Registrar nsec (32 bytes)
  │
  ├──→ Nostr identity (signing kind:11111 events)
  │     npub = nsec × G (secp256k1)
  │
  ├──→ Cashu P2PK spending (unlocking user payments)
  │     Sign proof secrets with Schnorr over secp256k1
  │
  └──→ DNSSEC key derivation (SLIP-10 → P-256)
        HMAC-SHA512("Nist256p1 seed", nsec) → P-256 private key
```

### Why a Single Key?

1. **Simplicity**: One key to manage, back up, and secure
2. **Verifiable link**: Anyone can derive the DNSSEC public key from the registrar's npub
3. **P2PK compatibility**: The secp256k1 key directly works with Cashu P2PK (no conversion needed)
4. **Nostr-native**: The key is already a valid Nostr identity

### Generating the Registrar Key

```bash
# Generate a new nsec for the registrar
nostreg-cli generate-key
# Or in the bot:
nostr_sdk::Keys::generate()
```

Store in bot config (TOML):

```toml
[registrar]
nsec = "nsec1..."  # or hex private key
npub = "npub1..."   # derived, for reference
p2pk_pubkey = "02..."  # compressed secp256k1 pubkey for P2PK locking
```

### Publishing Registrar Info (kind:10019)

The registrar publishes a NIP-61-style info event so clients know how to pay:

```json
{
  "kind": 10019,
  "pubkey": "<registrar-hex-pubkey>",
  "tags": [
    ["relay", "wss://relay.damus.io"],
    ["relay", "wss://nos.lol"],
    ["mint", "https://nofee.testnut.cashu.space", "sat"],
    ["pubkey", "<registrar-compressed-secp256k1-pubkey-for-p2pk>"]
  ]
}
```

Clients fetch this event to determine:
- Which mint to use (must match one of the registrar's listed mints)
- Which pubkey to P2PK-lock to
- Which relays to publish the payment event to

## Component 4: SLIP-10 DNSSEC Key Derivation

### Already Researched

Full analysis in `docs/13-nostr-dnssec-derivation.md` and `docs/15-nsec-to-dnssec-analysis.md`.

### Summary

```
nsec (32 bytes)
  → HMAC-SHA512(key="Nist256p1 seed", data=nsec)
  → I_L (first 32 bytes) = P-256 private key
  → Import into Knot DNS as KSK (algorithm 13, ECDSAP256SHA256)
```

### Three-Layer Trust Model

```
Layer 1: Key Derivation (mathematical)
  nsec → SLIP-10 → P-256 DNSSEC KSK → signs the zone

Layer 2: Nostr Attestation (protocol)
  Registrar publishes kind:11111 with dnskey tag → verifiable on Nostr

Layer 3: Standard DNSSEC Chain (internet)
  Root → .shop → nodns.shop (DS → KSK → ZSK → records)
```

### Nostr DNSKEY Attestation Event

The registrar publishes this once (or whenever the KSK changes):

```json
{
  "kind": 11111,
  "pubkey": "<registrar-hex-pubkey>",
  "tags": [
    ["dnskey", "nodns.shop", "<key-tag>", "13", "<base64-DNSKEY>"],
    ["dnskey-derivation", "slip10", "Nist256p1 seed"]
  ],
  "content": ""
}
```

Anyone can verify: derive P-256 pubkey from npub → compare with DNSKEY in DNS → confirm the link.

## Implementation Plan

### Phase 1: Registrar Identity (Low Effort)

- [ ] Generate registrar nsec/npub
- [ ] Add to bot config
- [ ] Publish kind:10019 event with mint + P2PK pubkey
- [ ] Add P2PK pubkey to `/api/zones/{zone}/pricing` response so frontend knows where to lock

### Phase 2: Frontend P2PK Send (Medium Effort)

- [ ] Integrate P2PKBuilder into registration flow
- [ ] Fetch registrar's kind:10019 to get P2PK pubkey + trusted mints
- [ ] Create P2PK-locked tokens for registration payment
- [ ] Include locked token in kind:11111 event (cashu tag)

### Phase 3: Bot P2PK Verification + Spending (Medium Effort)

- [ ] Verify incoming cashu tokens are P2PK-locked to registrar's pubkey
- [ ] Sign proof secrets with registrar's nsec (Schnorr signatures)
- [ ] Swap/melt tokens at the mint
- [ ] Mark payment as verified in the database

### Phase 4: Confirmation Events (Low Effort)

- [ ] Bot publishes kind:11111 confirmation after successful registration
- [ ] Include domain, expiry, payment amount, user pubkey
- [ ] Frontend displays confirmation status on dashboard

### Phase 5: SLIP-10 DNSSEC (Optional, Future)

- [ ] Implement `derive_p256_key()` in Rust
- [ ] Import derived key into Knot DNS
- [ ] Publish DNSKEY attestation event
- [ ] This is independent of the payment flow — can be done anytime

## Open Questions

1. **Does `nofee.testnut.cashu.space` support NUT-11?** Must check `/v1/info` endpoint. If not, we need a different mint or fall back to non-P2PK tokens.

2. **Does coco-cashu-core support P2PK?** The `cashu-ts` SDK has `P2PKBuilder`. The coco SDK may or may not expose this. If not, we may need to use `cashu-ts` for the send operation specifically.

3. **Token in event vs. separate payment?** Two options:
   - **Embedded**: Cashu token in the `["cashu", ...]` tag of the registration event (simpler, one event)
   - **Separate**: NIP-61 nutzap event separate from registration event (cleaner, standard)

4. **Refund mechanism?** If the bot rejects a registration (invalid record, domain taken), the P2PK-locked tokens need to be returned. Options:
   - Time-lock with refund (NUT-11 `locktime` + `refund` tags)
   - Bot publishes a refund event with unlocked tokens
   - User's tokens expire and are lost (simplest, acceptable for small amounts)

5. **Price discovery?** How does the frontend know the price? Already solved: `/api/zones/{zone}/pricing` returns `create_price`, `update_price`, `delete_price`.

## Security Considerations

- **Registrar nsec is high-value**: Compromise = ability to spend all P2PK-locked payments + re-derive DNSSEC key. Must be stored securely (config file for PoC, NIP-46 remote signing for production).
- **P2PK lock is enforced by mint**: Even if the token is public in a Nostr event, only the registrar can spend it.
- **Confirmation events are signed**: Anyone can verify the registrar's attestation using the registrar's npub.
- **No custodial risk**: The bot doesn't hold user funds. It only spends P2PK-locked tokens that were explicitly sent to it.

## References

- NUT-10: Spending Conditions — https://github.com/cashubtc/nuts/blob/main/10.md
- NUT-11: Pay-to-Public-Key (P2PK) — https://github.com/cashubtc/nuts/blob/main/11.md
- NIP-61: Nutzaps — https://nips.nostr.com/61
- NIP-06: Key Derivation from Mnemonic — https://nips.nostr.com/6
- SLIP-0010: Universal Key Derivation — https://github.com/satoshilabs/slips/blob/master/slip-0010.md
- docs/13: Nostr DNSSEC Derivation Research
- docs/15: nsec-to-DNSSEC Analysis
- docs/22: Pricing and Payments
- docs/23: Lease and Renewal
