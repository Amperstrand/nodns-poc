# 25 — Censorship Resistance

> **Status**: DRAFT. This is a core design goal of NoDNS, but the mechanisms are still being explored.

## The Core Principle

**If you hold the nsec, you control the domain. No court order, no registrar policy, no institutional action can change that.**

This applies specifically to `$npub.tld` names. For `$string.tld` delegated names, censorship resistance depends on operator reputation (see [Lease and Renewal](23-lease-and-renewal.md)).

## The Threat Model

### Scenario 1: Court Orders Domain Seizure

A court orders the operator of `nodns.shop` to redirect `npub1abc.nodns.shop` to a government-controlled server.

**Traditional DNS**: The operator complies. The domain now resolves to the government server. The original owner loses control.

**NoDNS**: The operator can change the DNS records (Layer 3 — convenience). But:
- The npub holder's Nostr events still exist on relays
- The npub holder's signature cannot be forged
- Any NoDNS-compliant resolver sees the court-ordered DNS records are unsigned by the npub holder
- The NoDNS resolver returns the correct records from Nostr, ignoring the court order

The court can control DNS, but it can't control Nostr. As long as the npub holder can publish events, their domain resolves correctly through NoDNS.

### Scenario 2: Operator Refuses to Renew

The operator refuses to process a valid renewal for `alice.nodns.shop` because of political pressure.

**Traditional DNS**: The domain expires and becomes available. The owner loses it.

**NoDNS**: The renewal event with payment proof is on relays. Anyone can verify it's valid. The operator loses reputation. A competing operator could honor the same event. NoDNS resolvers that follow the event log would extend the lease regardless of the original operator's actions.

### Scenario 3: Relay Censorship

Relays refuse to carry events from certain npubs.

**Mitigation**: Nostr's decentralized relay architecture. The npub holder can publish to any relay. The NoDNS resolver can subscribe to multiple relays. There is no single point of censorship.

## How NoDNS Resolution Works Under Censorship

```
Normal flow:
  User → Traditional DNS → nodns.shop → Knot DNS → returns records
  User → NoDNS resolver → Nostr relays → verifies signatures → returns records

Under court order (DNS tampered):
  User → Traditional DNS → nodns.shop → Knot DNS → returns COURT-ORDERED records
  User → NoDNS resolver → Nostr relays → verifies signatures → returns CORRECT records ✓

Under operator boycott (DNS shut down):
  User → Traditional DNS → FAILS (no DNS server responding)
  User → NoDNS resolver → Nostr relays → verifies signatures → returns CORRECT records ✓
```

The NoDNS resolver always returns the cryptographic truth, regardless of what happens at the DNS layer.

## The Home Router Scenario

A user runs a NoDNS-compliant resolver on their home router. This resolver:
1. Subscribes to Nostr relays
2. Watches for kind 11111 events for configured zones
3. Verifies event signatures and payment proofs
4. Builds a local DNS cache from verified events
5. Resolves queries from this cache

This resolver would:
- Ignore court-ordered DNS changes (they're not signed by the npub holder)
- Continue resolving even if the operator's DNS infrastructure is shut down
- Honor renewals that the operator refused to process
- Work independently of any institutional control

**This is the censorship resistance endgame**: a router-level resolver that follows cryptographic truth over institutional authority.

## What About `$string.tld` Names?

Delegated names (`alice.nodns.shop`) have weaker censorship resistance because they depend on the operator:

- **The operator controls the delegation**: They signed it, but they could also sign a new delegation to someone else.
- **Court can order the operator**: "Revoke alice.nodns.shop and delegate it to the government."
- **The operator might comply**: They have the registrar key.

However:
- The original delegation event is still on Nostr, verifiable by anyone
- The original owner can prove they had a valid lease with payment proof
- The operator's compliance with the court order is visible in the event log
- The operator loses reputation

This is **social censorship resistance**, not cryptographic. It relies on the operator valuing their reputation more than compliance with coercion.

## Comparison to Other Systems

| System | `$npub` equivalent | `$string` equivalent | Court-order resistance |
|---|---|---|---|
| **Traditional DNS** | N/A | All names are delegated | None — registrar can be ordered |
| **ENS** | N/A | All names are on-chain | Smart contract can't be court-ordered, but registrar controls the oracle |
| **Handshake** | N/A | All names are auctioned on-chain | Fully on-chain, but expensive |
| **Unstoppable Domains** | N/A | One-time purchase | "Permanent" but depends on blockchain |
| **NoDNS** | `$npub.tld` = cryptographic, `$string.tld` = reputation | | `$npub`: absolute. `$string`: social. |

## What We Landed On (For Now)

1. **`$npub.tld` names are censorship-resistant by design**: The nsec holder controls the name, period.
2. **`$string.tld` names are reputation-protected**: The operator's honesty is enforced by social consequences.
3. **NoDNS resolvers follow Nostr events, not DNS records**: Any conflict is resolved in favor of cryptographic truth.
4. **Home router scenario is the aspiration**: Anyone can run a NoDNS resolver that follows the truth.

## Still Open

- How to make NoDNS resolver software easy to deploy (router firmware? Docker? Browser extension?)
- Whether to add cryptographic enforcement for `$string.tld` (e.g., timelocked delegation that can't be revoked)
- How to handle DNSSEC when the resolver overrides DNS records (the DS chain would break)
- Whether relays themselves can be made censorship-resistant (Nostr's existing relay ecosystem helps)

---

## DNS-as-Relay-Cache (TXT-as-Event)

> **Status**: LIVE since 2026-06-10. Deployed on nodns.shop.

### What It Does

Every TXT record update from a kind:11111 event also gets a compact Nostr event embedded as an additional TXT record at the same name. This turns DNS itself into a relay cache — a nodns-aware resolver can extract event data directly from DNS without connecting to Nostr.

### Live Example

```
$ dig @8.8.8.8 test.npub1gxz8rj...nodns.shop TXT +short
"registered via nodns.shop"
"nostr:k=11111;i=082174ed...;p=418471cb..."
```

Two TXT records: the user's data, and the compact event.

### Compact Format

The bot uses a tiered fallback to fit within the 255-byte DNS TXT string limit:

| Tier | Format | Fits When |
|---|---|---|
| Full | `nostr:k=11111;i=<hex>;p=<hex>;s=<hex>;t=<tags>` | Small events (~279 bytes min — rarely fits) |
| No-sig | `nostr:k=11111;i=<hex>;p=<hex>;t=<tags>` | Medium events (~150 bytes + tags) |
| Minimal | `nostr:k=11111;i=<hex>;p=<hex>` | Always fits (142 bytes) |

The full format includes the 128-char hex signature. For most events, even the no-sig format exceeds 255 bytes due to tag data. The minimal format (event ID + pubkey) always fits and is sufficient for a resolver to fetch the full event from relays by ID.

### Implementation Details

- **Only for TXT records**: A/AAAA/CNAME/MX records don't get compact event embedding
- **Only for kind:11111**: The bot only processes these events anyway
- **Non-fatal**: If the compact TXT append fails, the user's record is still written
- **Secondary sync**: Compact TXT records sync to secondary DNS (puck) via AXFR. Knot DNS has no record-level transfer filtering. Negligible at current scale; fixable with a separate internal zone if needed
- **Code**: `nodns-bot-rs/src/event_processor.rs` — `build_compact_event_txt()` + `append_record()` call after each TXT `update_record()`

### Censorship Resistance Implications

TXT-as-event means DNS responses carry verifiable Nostr event data. A home router resolver can:

1. Query DNS for a name
2. Find the compact event TXT
3. Verify event signature (if included in full format) or fetch full event from relays by ID
4. Compare DNS record with the event — detect tampering
5. Resolve using cryptographic truth, not DNS convenience

This bridges the gap between the "convenience layer" (DNS) and the "truth layer" (Nostr) without requiring the user to run a full Nostr client.
