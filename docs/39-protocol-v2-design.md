# 39 — Protocol v2 Design: Zone Attestation, Discovery, and P2PK

> **Status**: DRAFT. Protocol design for the next evolution of nodns.

## Overview

Protocol v2 adds:
- **Zone attestation** via self-referential DNS↔Nostr cross-signing
- **Nostr-native zone discovery** — registration page discovers available zones from Nostr events
- **P2PK-locked Cashu payments** — tokens locked to the zone operator's npub
- **Testnet flag** — zones can signal they're testing, not fully committed
- **Protocol versioning** — events before a cutoff timestamp can be ignored

## Zone Attestation

### TXT Record Format

Published at `_nodns.{zone}` in the zone's DNS:

```
_nodns.nodns.shop.  IN  TXT  "v=2;npub=bbb5dda0...;create=2;update=0;delete=0;mint=testnut.cashu.space;npub_free=true;testnet=1"
```

Fields:
- `v` — protocol version (currently 2)
- `npub` — zone operator's Nostr public key (hex). This is the P2PK lock target.
- `create`, `update`, `delete` — pricing in sats
- `mint` — accepted Cashu mint URL
- `npub_free` — whether npub-derived names bypass payment
- `testnet` — 1 if zone is testing (not fully committed), 0 or absent for production

### Signed Nostr Event (kind 31990 handler)

Published by the zone's npub:

```json
{
  "kind": 31990,
  "pubkey": "bbb5dda0...",
  "content": "{\"name\":\"nodns.shop\",\"about\":\"Decentralized DNS zone operator\",\"website\":\"https://nodns.shop\",\"nip05\":\"_nodns@nodns.shop\"}",
  "tags": [
    ["d", "nodns-registrar"],
    ["k", "11111"],
    ["zone", "nodns.shop"],
    ["dnskey_hash", "aca4c1968ae4ecda5f2eaf245207b7a3a36a55a620d631573abaddd3c7449d01"],
    ["dnskey_alg", "ECDSAP256SHA256"],
    ["pricing", "create=2", "update=0", "delete=0"],
    ["mint", "testnut.cashu.space"],
    ["testnet"],
    ["status", "testing", "Best-effort pilot - not yet fully implemented"],
    ["web", "https://nodns-registrar.pages.dev/"]
  ]
}
```

This is a NIP-89 handler event that simultaneously:
1. Announces the service to Nostr clients (standard NIP-89)
2. Cross-attests the DNS key via `dnskey_hash`
3. Publishes pricing and policy
4. Flags testnet status and granular service status

### Status Tag

The `["status", VALUE, REASON?]` tag signals the zone's operational maturity:

| Value | Meaning | Client Behavior |
|---|---|---|
| `testing` | Best-effort pilot, not fully implemented, records may be temporary | Show amber "TESTING MODE" banner prominently |
| `preview` | Feature-complete but pre-launch, expect breaking changes | Show "PREVIEW" badge |
| `production` | Fully live and operational | Normal display |

The `REASON` field (index 2) is optional human-readable context shown in the UI.

Relay-filterable: `{"#status": ["production"]}` queries return only production zones.

When transitioning from testing to production: publish a new kind:31990 event (same `d` tag) with `["status", "production"]` and remove the `["testnet"]` tag. The old event is auto-superseded (parameterized replaceable).

### Self-Referential Verification

```
DNS side:                              Nostr side:
_nodns.zone TXT "npub=XXX"             kind 31990 signed by XXX
  ↑ signed by DNSSEC KSK                 ↑ contains dnskey_hash of KSK
  
DNS says: "my Nostr key is XXX"        Nostr says: "my DNS key hash is YYY"
```

Both must verify. Attacker needs BOTH the DNSSEC KSK AND the Nostr nsec to forge.

## Zone Discovery via Nostr

### Flow

```
Registration page (nsite or web app)
    │
    ├─ 1. Query relay.cashu.email for kind 31990 events with zone tag
    │     Filter: { kinds: [31990], "#zone": [...] }
    │
    ├─ 2. For each discovered zone:
    │     ├─ Parse npub, pricing, mint, testnet flag
    │     ├─ Fetch _nodns.{zone} TXT from DNS
    │     ├─ Verify TXT npub matches event signer
    │     ├─ Fetch DNSKEY for zone
    │     ├─ Verify DNSKEY hash matches event's dnskey_hash
    │     └─ Display: "✓ nodns.shop — verified, 2 sats/create, testnet"
    │        or: "⚠ example.com — TXT found but DNSKEY mismatch"
    │        or: "✗ unverified zone — no attestation"
    │
    └─ 3. User selects zone → search for names → register
```

### Debug Panel

The registration page shows a debug panel with verification steps:

```
Zone Discovery
├─ Querying relay.cashu.email for zone handlers...
├─ Found 2 zones:
│
├─ nodns.shop
│  ├─ TXT record: ✓ found
│  ├─ NIP-89 event: ✓ signed by npub bbb5dda0...
│  ├─ TXT npub matches event signer: ✓
│  ├─ DNSKEY hash matches: ✓ (aca4c196...)
│  ├─ Pricing: create=2, update=0, delete=0
│  ├─ Mint: testnut.cashu.space
│  └─ Status: ✓ VERIFIED (testnet)
│
└─ unverified.example
   ├─ TXT record: ✗ not found
   └─ Status: ✗ UNVERIFIED — zone has not opted in
```

## P2PK Payment Locking

### Flow

```
User registers alice.nodns.shop:
1. Client fetches _nodns.nodns.shop TXT → npub = bbb5dda0...
2. Client creates Cashu token via wallet.sendTokens(2)
3. Client wraps token with P2PK lock to bbb5dda0... (NUT-11)
4. Client publishes kind 11111 with:
   ["record", "A", "alice", "3600", "1.2.3.4"]
   ["cashu", "<p2pk-locked-token>", "testnut.cashu.space", "2"]
   ["alt", "DNS A record: alice.nodns.shop → 1.2.3.4 (via NoDNS)"]
5. Bot receives event:
   a. Verify Nostr signature
   b. Verify P2PK lock targets zone's npub (from TXT)
   c. Claim token using zone's nsec (Schnorr signature)
   d. Push DDNS update to Knot DNS
6. If scammer intercepts: cannot claim token (no nsec)
```

### NUT-11 P2PK Secret Format

```json
[
  "P2PK",
  {
    "nonce": "<random-32-bytes-hex>",
    "data": "<zone-npub-hex-32-bytes>",
    "tags": [["sigflag", "SIG_INPUTS"]]
  }
]
```

The zone operator unlocks by providing a Schnorr signature on the proof secret using their nsec.

## Testnet Flag

Zones that are testing nodns without full commitment include `testnet=1` in their TXT record and a `testnet` tag in their NIP-89 event. The `["status", "testing", ...]` tag provides additional granularity (see [Status Tag](#status-tag) above).

Client behavior:
- Display a "TESTNET" badge next to the zone name
- Show warning: "This zone is testing nodns. Registrations may be temporary."
- Do NOT display testnet zones in production mode (configurable)

nodns.shop is currently `testnet=1` with `["status", "testing"]`.

## Protocol Versioning

### Cutoff Timestamp

Each zone can specify a protocol cutoff timestamp in its NIP-89 event:

```json
["valid_from", "1782196435"]
```

Events with `created_at < valid_from` are ignored by the zone's bot. This allows protocol upgrades without legacy event conflicts.

### Migration Strategy

1. **Phase 1 (now)**: Publish zone attestation TXT + NIP-89 event for nodns.shop
2. **Phase 2**: Add `alt` tags to all new kind 11111 events
3. **Phase 3**: Add P2PK locking to Cashu tokens
4. **Phase 4**: Consider kind 11111 → 31111 migration (breaking)
5. **Phase 5**: Registration page discovers zones from Nostr (self-contained nsite)

Events before Phase 1 cutoff are legacy and can be ignored.

## Known Limitations

### Domain Racing / Sniping

**Problem**: Two users publish registration events for the same name simultaneously. The bot processes them in arrival order — first wins, second pays but gets nothing.

**Mitigation options**:
- Bot checks if name already exists before processing → rejects duplicate
- Cashu token for the loser is NOT claimed (they keep their sats)
- But the loser's event is still on the relay — confusing

**Default**: Bot SHOULD reject duplicate registrations. Loser's Cashu token SHOULD NOT be claimed. Registration is first-come-first-serve at the bot level.

**Open question**: Should we implement a commit-reveal scheme? (Overkill for pilot.)

### Underpayment

**Problem**: User locks up less Cashu than the published price.

**Mitigation options**:
- Bot rejects: event not processed, token not claimed
- Bot accepts with prorated TTL: e.g., paid 1 sat for 2-sat name → record gets half TTL (1800s instead of 3600s)
- Bot accepts with reduced lease: full TTL but shorter renewal window

**Default**: Bot SHOULD reject underpayment. Token is NOT claimed. Event is logged but no DNS record is created.

**Future**: A zone MAY publish a prorating policy in its TXT record: `prorate=1`. Bot then applies prorated TTL.

### Key Rotation (DNSSEC KSK Rollover)

**Problem**: When the zone rotates its DNSSEC KSK, the `dnskey_hash` in the NIP-89 event becomes stale. Clients see a mismatch.

**Mitigation**:
- Zone operator re-publishes the NIP-89 event with new `dnskey_hash` after rollover
- During rollover transition (both old and new KSK active), either hash is valid
- Client SHOULD accept a match against any active KSK

**Default**: Zone operator MUST re-publish NIP-89 event after KSK rollover. Client SHOULD fetch current DNSKEY set and check against any active KSK.

### Relay Censorship

**Problem**: relay.cashu.email is the only relay. If it goes down, no events propagate.

**Mitigation**: relay.cashu.email is operated by Amperstrand and has high availability. For redundancy, zone operators MAY publish to additional relays. But spam-sensitive relays may block custom kinds.

**Default**: Publish to relay.cashu.email only. Future: add relay.cashu.email + one backup.

### Event Flooding

**Problem**: A malicious user publishes thousands of kind 11111 events to bloat relays.

**Mitigation**:
- Rate limiting at relay level
- Kind 31111 (addressable) would limit to one event per (pubkey, d-tag)
- Bot ignores events from rate-limited pububs

**Default**: Relay.cashu.email rate-limits per pubkey. Bot has its own rate limiter (5 events/window).

### No Refund Mechanism

**Problem**: If the bot fails to process a registration (DDNS error, policy violation), the Cashu token may already be claimed. User loses sats with no DNS record.

**Mitigation**:
- Bot claims token ONLY after successful DDNS update
- If DDNS fails, token is NOT claimed (user keeps sats)
- For hold-and-claim mode (pilot): token is never claimed until reconciliation

**Default**: Bot claims AFTER successful registration. Hold-and-claim for pilot.

## Experimental Status

nodns is explicitly experimental. The protocol may change at any time. Key caveats:

1. **Events before protocol cutoff may be ignored** — zones set `valid_from` in their NIP-89 event
2. **Testnet zones may disappear** — registrations on testnet zones are temporary
3. **P2PK locking is new** — Cashu P2PK implementation needs real-world testing
4. **No backward compatibility guarantee** — protocol v2 events may not be understood by v1 bots
5. **Specification is a draft** — `docs/11-protocol-experimental-draft.md` may change without notice

## References

- [11-protocol-experimental-draft.md](11-protocol-experimental-draft.md) — kind 11111 protocol specification
- [36-anti-spam-research.md](36-anti-spam-research.md) — NIP-13 PoW, proof of burn, Cashu micro-payment
- [37-cv-trust-model.md](37-cv-trust-model.md) — .cv front-running protection
- [38-nostr-alignment-research.md](38-nostr-alignment-research.md) — NIP-89, kind 11111 vs 31111, P2PK research
- NIP-89: https://nips.nostr.com/89
- NIP-31 (alt tags): https://nips.nostr.com/31
- NUT-11 (P2PK): https://cashubtc.github.io/nuts/11/
