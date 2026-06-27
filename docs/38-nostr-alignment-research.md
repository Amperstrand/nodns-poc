# 38 — Nostr Protocol Alignment and Trust Model Research

> **Status**: DRAFT. Research notes for protocol design decisions.

## Part 1: Nostr Service Discovery (NIP-89)

### What NIP-89 provides

NIP-89 defines two event kinds for application discoverability:

- **kind:31990** (Handler Information): Published by the application itself. Describes what kinds it handles and how to redirect users (web URLs, platform links). Includes optional metadata (name, picture, about).
- **kind:31989** (Recommendation): Published by users who recommend an application for a specific kind.

NIP-90 (Data Vending Machine) explicitly recommends NIP-89 for service provider discoverability:
> "Service Providers MAY use NIP-89 announcements to advertise their support for job kinds"

### What nodns should do

1. **Publish a kind:31990 handler event** for the zone's npub:
   ```json
   {
     "kind": 31990,
     "content": "{\"name\":\"NoDNS Registrar\",\"about\":\"Decentralized DNS from Nostr events\",\"website\":\"https://nodns.shop\"}",
     "tags": [
       ["d", "nodns-registrar"],
       ["k", "11111"],
       ["web", "https://nodns-registrar.pages.dev/"]
     ]
   }
   ```

2. **This makes nodns discoverable**: When a Nostr client sees an unknown kind:11111 event, it can query for kind:31990 handlers for kind 11111 and find the NoDNS Registrar.

3. **Publish a kind:0 profile** for the zone's npub (if content is empty in 31990, clients fall back to kind:0):
   ```json
   {
     "kind": 0,
     "content": "{\"name\":\"nodns.shop\",\"about\":\"Decentralized DNS zone operator\",\"website\":\"https://nodns.shop\",\"picture\":\"...\"}"
   }
   ```

## Part 2: Event Kind — Regular vs Addressable

### Current state

nodns uses **kind 11111** (regular range: 0-9999). Regular events are stored once per event ID — relays keep the full history. Re-publishing the same record creates a new event, not an update.

### Problem

No idempotency. If a user publishes 100 events for the same record, all 100 are stored and delivered. The bot must dedup client-side. Relay storage grows unbounded.

### Historical alternative: kind 31111 (addressable/parameterized replaceable)

Kind 31111 is in the addressable range (30000-39999). Addressable events keep only the latest per `(pubkey, kind, d-tag)`. The d-tag identifies which specific record this is:

```json
{
  "kind": 31111,
  "tags": [
    ["d", "alice.nodns.shop:A"],
    ["record", "A", "alice", "3600", "1.2.3.4"]
  ]
}
```

Benefits:
- Relay dedup: only latest event per (pubkey, "alice.nodns.shop:A")
- Natural idempotency: re-publishing replaces the old version
- Efficient storage: one event per record, not full history

Trade-offs:
- **Breaking change**: all existing kind 11111 events would need re-publishing as 31111
- **Relay support**: all relays support addressable events per NIP-01
- **Query pattern**: clients query by `#d` tag instead of scanning all events

### Recommendation

Do **not** migrate to kind 31111. Keep kind 11111 as the protocol. The rest of this section is retained only as a record of the trade-off analysis.

## Part 3: NIP-31 (alt tags for unknown kinds)

Custom event kinds should include an `alt` tag — a human-readable summary that social clients display when they encounter unknown kinds:

```json
{
  "kind": 11111,
  "tags": [
    ["record", "A", "alice", "3600", "1.2.3.4"],
    ["alt", "DNS A record: alice.nodns.shop → 1.2.3.4 (via NoDNS)"]
  ]
}
```

This is a trivial addition with high impact — makes events visible in social clients without special handling.

## Part 4: Self-Referential DNS ↔ Nostr Trust Model

### The problem

Anyone can publish kind 11111 events for any zone. Without verification, a scammer can:
1. Claim to operate a nodns bridge for `someTLD.com`
2. Collect Cashu payments for registrations
3. Deliver nothing (or register via our bot using free test sats)

### The self-referential cycle

```
DNS (P-256 / ECDSAP256SHA256)          Nostr (secp256k1 / Schnorr)
┌─────────────────────────┐           ┌─────────────────────────┐
│ _nodns.zone TXT:        │           │ kind 31111 event:       │
│ "npub=xxx;create=2;     │           │ ["zone-policy",         │
│  mint=testnut;          │           │  "zone", "npub",        │
│  npub_free=true"        │           │  "2","0","0",           │
│                         │           │  "testnut","true",      │
│ DNSKEY: 257 3 13 ...    │           │  "dnskey_hash=abc"]     │
│                         │           │                         │
│ Signed by DNSSEC KSK    │           │ Signed by nsec          │
│ (zone's P-256 key)      │           │ (zone's secp256k1 key)  │
└───────────┬─────────────┘           └─────────────┬───────────┘
            │                                       │
            │  DNS attests: "my Nostr key is npub"  │
            │  Nostr attests: "my DNS key is hash"  │
            │                                       │
            └──── mutual attestation cycle ─────────┘
```

### Why cross-signing, not derived keys

| Approach | Verifiable? | Key Rotation | Compromise | Standard |
|---|---|---|---|---|
| Cross-signing (DNS↔Nostr) | ✅ Yes — fetch TXT + verify sig | Independent | Need BOTH keys to forge | Uses existing DNSSEC + Nostr |
| Derived from shared seed | ❌ No — derivation is one-way, can't verify without seed | Must rotate both | Seed leak = total compromise | Non-standard |
| Same key | ❌ Impossible — DNSSEC uses P-256, Nostr uses secp256k1 | N/A | N/A | Different curves |

### Client verification flow

```
1. Fetch _nodns.{zone} TXT
   → Not found? UNVERIFIED — zone has not opted in
   → Found? Parse npub, pricing, mint

2. Validate DNSSEC on response
   → No DNSSEC? WEAK — TXT could be forged by MITM
   → Valid DNSSEC? STRONG — TXT is authentic

3. Fetch zone-policy Nostr event
   → Query: { kinds: [31111], authors: [npub_from_txt], "#d": ["zone-policy:{zone}"] }
   → Verify signature matches npub_from_txt
   → Extract dnskey_hash from event

4. Cross-check DNSKEY
   → Fetch DNSKEY for zone
   → SHA-256 hash it
   → Compare to event's dnskey_hash

5. Result:
   → All pass: VERIFIED ✓
   → TXT + sig match but DNSKEY hash doesn't: DNS key rotated, event stale
   → TXT missing: zone not opted in
```

### Cashu P2PK integration

The npub from the TXT record is the lock target. **Always fetched from DNS, never hardcoded.**

```
User wants to register alice.nodns.shop:
1. Client fetches _nodns.nodns.shop TXT → npub = npub1xxx
2. Client creates Cashu token P2PK-locked to npub1xxx (NUT-11)
3. Client publishes the existing kind 11111 with:
   - record tag: ["record", "A", "alice", "3600", "1.2.3.4"]
   - cashu tag: ["cashu", "<p2pk-locked-token>", "testnut.cashu.space", "2"]
4. Bot receives event:
   - Verifies Nostr signature
   - Verifies P2PK lock targets the zone's npub
   - Claims token using zone's nsec (Schnorr signature)
   - Pushes DDNS update
5. Scammer scenario:
   - Scammer intercepts the event
   - Reads the P2PK-locked token
   - Cannot claim it — needs zone's nsec to produce Schnorr signature
   - Token is useless to scammer
```

NUT-11 P2PK supports:
- Basic: lock to single pubkey, one signature to unlock
- Multisig: lock to N pubkeys, M-of-N signatures required
- Locktime: lock expires after timestamp, refund keys can spend
- SIG_ALL: signature covers all inputs + outputs (strongest)

For nodns, basic P2PK is sufficient. The zone operator's nsec produces the unlock signature.

## Part 5: Relay Strategy

### Current state
- Registrar publishes to: relay.cashu.email, relay.tollgate.me
- Bot subscribes to: relay.cashu.email, nos.lol (was damus.io)

### Issue
Other relays (damus.io, nos.lol) may consider kind 11111 events as spam since they're custom and potentially high-volume. We've already been blocked by tollgate.me ("blocked: not on white-list").

### Recommendation
- **Publish only to relay.cashu.email** — this is the Cashu ecosystem relay
- Bot subscribes to relay.cashu.email only (remove nos.lol)
- relay.cashu.email is operated by Amperstrand and won't block our events
- For zone attestation events (NIP-89, kind:0), also publish to nos.lol and relay.damus.io — these are standard kinds that won't be flagged

## Part 6: Alignment with Amperstrand Ecosystem Patterns

(To be filled with explore agent results)

### Comparison table

| Project | Event Kinds | NIP-89 | Kind:0 Profile | Signing | Relays |
|---|---|---|---|---|---|
| nodns | 11111 (custom) | ❌ No | ❌ No | local nsec | relay.cashu.email |
| nomail | NIP-42 auth (27235) | ❌ No | ✅ Yes | extension + nsec | relay.cashu.email + others |
| blossomflare | 24242 (BUD-11) | ❌ No | ❌ No | NIP-98 header | own relay |
| tollgate | ? | ❌ No | ❌ No | ? | ? |

### Gaps identified

1. **No project in the ecosystem uses NIP-89** — all rely on out-of-band discovery (web links, documentation)
2. **nomail is the only project with a kind:0 profile** for the service identity
3. **No project uses P2PK locking** — all Cashu tokens are anyone-can-spend
4. **nodns is the only project with a custom protocol kind** — nomail uses NIP-42 auth, blossomflare uses BUD-11

## References

- NIP-89: https://nips.nostr.com/89
- NIP-31 (alt tags): https://nips.nostr.com/31
- NIP-01 (addressable events): https://nips.nostr.com/1
- NUT-11 (P2PK): https://cashubtc.github.io/nuts/11/
- NUT-18 (Payment Requests): https://cashubtc.github.io/nuts/18/
- DNSSEC: RFC 4033, 4034, 4035
- nodns protocol spec: docs/11-protocol-experimental-draft.md
- Trust model draft: docs/37-cv-trust-model.md
