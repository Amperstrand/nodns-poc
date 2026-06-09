# 26 — Open Questions and Future Research

> **Status**: DRAFT. These are areas that need more exploration. Nothing here is decided.

## Namespace Takeover / Auction

### The Idea

If someone wants `alice.nodns.shop` and it's already taken, they could:
- Pay 2x the current total paid amount (lease payments) to become the new owner
- Participate in a Dutch auction when the name expires
- Submit a sealed-bid auction where the highest bid wins

### Why It's Interesting

- Creates a market for desirable names
- Prevents permanent squatting
- Gives the original owner a payout (if they choose to sell)

### Why It's Deferred

- Complex protocol design (auction state machine, bid/reveal, refunds)
- `$npub.tld` names can NEVER be taken over (inalienable) — this only applies to `$string.tld`
- The nodns.shop PoC uses worthless test tokens — no real economic incentive
- Needs careful design to prevent abuse (e.g., harassment via constant takeover attempts)

### What to Research

- Handshake's commit-reveal auction mechanism
- ENS's premium price decay for expired names
- Unstoppable Domains' marketplace overlay
- Whether takeover should be operator-mediated or protocol-mediated

---

## OpenTimestamps Integration

### The Idea

Use OpenTimestamps to prove that a payment/registration event existed before a specific Bitcoin block. This would provide strong, independently-verifiable time proof.

### Why It's Interesting

- `created_at` is signer-supplied (weak time proof)
- OpenTimestamps anchors to Bitcoin (strong time proof, ~10 min resolution)
- Could be critical for dispute resolution: "I published my renewal BEFORE the deadline"

### Why It's Deferred

- The Rust `opentimestamps` crate exists but is basic (parse/serialize, no stamping)
- For the PoC, relay publication time + event signature is probably sufficient
- Adds an external dependency (Bitcoin node or calendar server)
- The 10-minute resolution might not be precise enough for close deadlines

### What to Research

- Whether `created_at` + relay-level timestamps are sufficient for most disputes
- How to integrate OTS stamping into the event processing pipeline
- Whether we need a full Bitcoin node or can use public calendar servers
- The trust model: calendars are convenience services, Bitcoin is the trust anchor

---

## MuSig / Co-Signing

### The Idea

The operator co-signs registration/renewal events as a "stamp of approval." This provides an explicit attestation from the operator that they accept the claim.

### Why It's Interesting

- Gives users explicit confirmation (not just silence = acceptance)
- Creates a verifiable audit trail of operator approvals
- MuSig would produce a single combined signature (compact)

### Why It's Deferred

- Adds complexity to the event signing flow
- The operator already processes events (implicit approval via processing)
- MuSig requires interactive signing (both parties must be online)
- For the PoC, implicit approval (operator processes the event) is sufficient

### What to Research

- Whether MuSig2 (non-interactive) could work here
- Whether a simple co-signature (two separate sigs) is simpler and good enough
- How co-signing interacts with censorship resistance (operator could refuse to co-sign)

---

## Relay Synchronization

### The Problem

Events propagate to different relays at different speeds. The operator's bot might see events in a different order than a third-party verifier.

### Why It Matters

- Registration race conditions depend on event ordering
- Renewal deadlines depend on when the event was "published"
- Dispute resolution needs a common view of event history

### What to Research

- How to define "publication time" in a relay-distributed system
- Whether relay-level timestamps (seen-at) could supplement `created_at`
- How NIP-77 (eventual consistency) or similar proposals address this
- Whether we need a "canonical relay" for each zone (centralization risk)

---

## DNSSEC Under Censorship Overrides

### The Problem

If a NoDNS resolver overrides DNS records (because the Nostr events disagree), the DNSSEC signatures break. The resolver would need to either:
1. Re-sign the zone with its own key (requires the zone key)
2. Return unsigned responses (loses DNSSEC protection)
3. Use a different trust anchor for NoDNS-verified records

### Why It Matters

DNSSEC is a core feature of NoDNS. If censorship resistance breaks DNSSEC, we need a solution.

### What to Research

- Whether the npub holder's Nostr signature can serve as a replacement trust anchor
- Whether a separate "NoDNS trust anchor" (like a DNSSEC root key) could work
- How browsers and resolvers handle unsigned or differently-signed responses
- Whether DANE/TLSA records could bridge the gap

---

## Recurring Payments / Subscriptions

### The Idea

Instead of per-record payments, support recurring payments (e.g., monthly lease payments via Cashu or Lightning).

### Why It's Interesting

- Aligns with the lease model (pay monthly, not per-record)
- More predictable revenue for operators
- Simpler UX (set up once, auto-renew)

### Why It's Deferred

- Cashu doesn't have a native subscription mechanism
- Lightning subscriptions require channel management
- For the PoC, per-event payments are simpler and sufficient

### What to Research

- Whether Cashu could support a "standing order" pattern (pre-authorized tokens)
- BOLT 12 recurring payments
- Whether renewal events with fresh payment proofs can simulate subscriptions

---

## Multi-Operator Namespace

### The Idea

The same namespace (e.g., `nodns.shop`) could be served by multiple operators. Users choose which operator to trust.

### Why It's Interesting

- Reduces single-operator risk
- Creates competition (better pricing, better reliability)
- Makes censorship harder (which operator do you coerce?)

### Why It's Deferred

- Requires consensus between operators (which events are valid?)
- The protocol currently assumes a single operator per zone
- Significant protocol complexity

### What to Research

- How federated/distributed registry systems handle multi-operator consensus
- Whether CRDTs or operational transforms could help synchronize event processing
- How to handle conflicting delegation events from different operators

---

## Integration with Traditional DNS Infrastructure

### The Idea

Make it easy for existing TLD operators (like `.cv`) to adopt NoDNS patterns:
- Accept Bitcoin payments for domain registrations
- Publish delegation events on Nostr
- Allow NoDNS resolvers to verify their DNS records

### Why It's Interesting

- The real goal of NoDNS is adoption by TLD operators
- `.cv` and similar ccTLDs could benefit from Bitcoin-native payments
- Bypasses traditional registrar infrastructure and fees

### What to Research

- How to present NoDNS to TLD operators (technical requirements, economic model)
- Whether NoDNS can work alongside existing DNS infrastructure (not replacing it)
- What the minimum viable integration looks like for a TLD operator
- Legal and regulatory considerations for Bitcoin-denominated domain leases

---

## Summary of Open Questions by Priority

| Priority | Question | Blocks v1? |
|---|---|---|
| High | Race conditions for registration | No (first-come-first-served for PoC) |
| High | Renewal without operator intervention | Yes (core feature) |
| Medium | DNSSEC under censorship overrides | No (future concern) |
| Medium | Relay synchronization | No (acceptable for PoC) |
| Low | OpenTimestamps integration | No |
| Low | MuSig co-signing | No |
| Low | Namespace takeover/auction | No |
| Low | Recurring payments | No |
| Low | Multi-operator namespace | No |
| Low | TLD operator integration | No (separate effort) |
