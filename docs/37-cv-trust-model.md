# 37 — .cv Trust Model and Front-Running Protection

> **Status**: DRAFT. Design document for when the .cv pilot goes live.

## Problem

When the .cv pilot launches, a scammer could:

1. Set up a fake "official .cv registrar" website
2. Accept real Cashu sats for .cv domain registrations
3. Users believe they're buying from the official operator
4. Scammer keeps the sats; domains never work

Users have no way to verify that the bot processing their registration is authorized by the .cv registry operator. Without trust anchors, any party can claim to be the official bridge.

## Three Layers of Protection

### Layer 1: Testnut Only (Enforced)

**Status**: Already in config (`mint_filter = "testnut"` for .cv zone).

The bot rejects any Cashu token not from `testnut.cashu.space`. No real economic value can flow through the system during pilot. A scammer collecting real sats gets value, but the domains they "sell" would never process through our bot — users would see their registration silently fail.

**Config** (already deployed):
```toml
[dns.zones.payment]
mint_url = "https://testnut.cashu.space"
mint_filter = "testnut"
```

**Limitation**: This doesn't prevent a scammer from accepting real sats and delivering nothing. It just ensures our infrastructure never processes real-value payments for .cv during pilot.

### Layer 2: Domain Attestation via TXT Record

**Status**: Design — needs operator coordination.

The .cv registry operator publishes their npub as a DNS TXT record on their authoritative zone:

```
_nodns.cv.  IN  TXT  "v=1; npub=npub1xxx..."
```

This proves:
1. The .cv registry has opted in to nodns bridging
2. The npub is the authorized bridge operator
3. Only the actual registry operator can publish this record (they control the zone)

**Bot verification**: Before processing any .cv registration, the bot queries `_nodns.cv` TXT. If the record doesn't exist or doesn't match the configured registrar pubkey, the bot refuses to process.

**User verification**: The registrar UI fetches the TXT record and displays "Verified: authorized by .cv registry" or "Warning: no registry attestation found."

**Protocol extension** (kind 11111 tag):
```
["registrar", "cv", "<pubkey-from-txt-record>"]
```

The bot cross-checks: does the registrar pubkey in the event match the TXT record on the zone? If not, reject.

### Layer 3: P2PK Payment Locking (Cashu NUT-10)

**Status**: Design — needs Cashu protocol support.

Cashu tokens in kind 11111 events for .cv are locked to the registry's published npub using NUT-10 P2PK:

```json
{
  "nut10": {
    "k": "P2PK",
    "d": "<registry-npub-hex>"
  }
}
```

Only the registry (or their authorized bot holding the nsec) can claim these tokens. A scammer who intercepts the event can read the token but cannot spend it — the P2PK lock requires the registry's signature.

**Wallet support**: The registrar creates P2PK-locked outputs when generating payment tokens for .cv registrations. The bot verifies the lock before accepting the token.

**Limitation**: NUT-10 P2PK requires wallet support on both sides. coco-cashu-core and @cashu/cashu-ts support P2PK, but the integration needs testing.

## Attack Vectors and Mitigations

| Attack | Layer 1 | Layer 2 | Layer 3 |
|---|---|---|---|
| Fake registrar website collecting real sats | ✅ Bot won't process real sats | ✅ No TXT record to verify | N/A — scammer never gets tokens |
| Fake registrar using test sats to register via our bot | ✅ Allowed (test sats only) | ❌ Scammer's npub ≠ registry npub | ❌ Tokens locked to registry npub |
| Man-in-the-middle intercepting real registrations | ✅ Bot rejects real mint | ✅ TXT record verification | ✅ P2PK prevents token theft |
| Fake TXT record on compromised DNS | N/A | ❌ Compromised DNS = full attack | ✅ P2PK still protects tokens |

## Implementation Roadmap

### Phase 1: Pilot Launch (Testnut Only)
- ✅ `mint_filter = "testnut"` enforced in bot config
- ✅ Registrar displays "BETA — Test sats only" warning
- No additional work needed

### Phase 2: Registry Attestation
- [ ] Operator publishes `_nodns.cv` TXT record with their npub
- [ ] Bot code: query TXT before processing .cv registrations
- [ ] Registrar UI: display verification status
- [ ] Config: `registrar_pubkey` in zone config

### Phase 3: P2PK Payment Locking
- [ ] Bot: verify NUT-10 P2PK lock targets registry npub
- [ ] Registrar: create P2PK-locked Cashu outputs
- [ ] Testing: end-to-end P2PK verification with real test tokens

## Generalization: Pledge Mirroring for Any Zone

The TXT attestation pattern generalizes beyond .cv. Any zone operator can publish:

```
_nodns.<zone>.  IN  TXT  "v=1; npub=<operator-npub>"
```

This creates an explicit opt-in signal: "I authorize this npub to bridge DNS records for my zone via the nodns protocol." Without it, a mirror operator is claiming authority they may not have.

Users (and bots) can verify:
1. Does `_nodns.<zone>` have a TXT record?
2. Does the npub match the bot's configured registrar?
3. If not, the mirror is unverified — use at your own risk

This mirrors the DNSSEC trust model: the zone owner publishes a record (DS record for DNSSEC, TXT for nodns) that anchors trust in the DNS hierarchy itself.

## References

- [36-anti-spam-research.md](36-anti-spam-research.md) — NIP-13 PoW, proof of burn, Cashu micro-payment comparison
- [11-protocol-experimental-draft.md](11-protocol-experimental-draft.md) — kind 11111 protocol, registrar tags
- NUT-10 (P2PK): https://github.com/cashubtc/nuts/blob/main/10.md
- Cashu NUT-18 (Payment Requests): https://github.com/cashubtc/nuts/blob/main/18.md
- hackathon-tooling pattern: `patterns/cashu/nut18-nostr-embedded-payments.md`
