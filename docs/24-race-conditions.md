# 24 — Race Conditions and Registration Conflicts

> **Status**: DRAFT. This is an **open research area**. The current approach is a simplification for the PoC.

## The Challenge

What happens when two people try to register the same domain name at the same time?

```
User A: publishes claim for alice.nodns.shop at created_at=1749168001
User B: publishes claim for alice.nodns.shop at created_at=1749168001
```

Both events arrive at relays. Both include valid payment. Who gets the domain?

This is the **double-spend problem** applied to namespace registration. In distributed systems, this is fundamentally hard.

## Why This Is Hard

1. **Nostr relays are not synchronized**: Events propagate at different speeds to different relays. There is no global clock.
2. **`created_at` is client-supplied**: A malicious user can set any timestamp. It's part of the event hash but not independently witnessed.
3. **The operator sees events at different times**: Depending on which relays the bot subscribes to, events may arrive out of order.
4. **Domain squatters**: A bad actor could watch for registration events and immediately publish competing claims, hoping to win the race.

## Approaches

### 1. First-Come-First-Served (Operator as Clock) — **Current Choice for PoC**

The operator's bot is the arbiter. The first valid claim it processes wins.

**Tiebreaker**: If two events have the same `created_at`, the one with the lower event ID wins (deterministic, since event IDs are SHA256 hashes).

**Pros**: Simple, no extra protocol complexity, works for PoC.
**Cons**: Depends on relay ordering, operator could theoretically favor one claim over another, not "fair" in a distributed sense.

### 2. Commit-Reveal (Handshake Style)

Handshake uses a commit-reveal auction:
1. **Commit phase**: Users submit hashed bids (commitments)
2. **Reveal phase**: Users reveal their bids
3. **Winner**: Highest bid wins

This prevents front-running because bids are hidden during the commit phase.

**Pros**: Cryptographically fair, prevents front-running, battle-tested in Handshake.
**Cons**: Adds latency (commit + reveal phases), complex UX, overkill for 2-sat PoC domains.

### 3. Dispute Window

After a claim is published, there's a window (e.g., 24 hours) where competing claims can be submitted. After the window closes, the claim is final.

**Pros**: Gives time for conflicts to surface.
**Cons**: Slow (24h delay), relay sync issues (message might be published to a relay the operator doesn't follow), doesn't prevent squatting.

### 4. Auction for Conflicting Names

If two valid claims arrive within a time window, trigger a Dutch or sealed-bid auction.

**Pros**: Market-based resolution.
**Cons**: Complex, bad UX for cheap domains, incentivizes squatting.

### 5. Reputation-Weighted Priority

Users with longer Nostr history, more zap activity, or other reputation signals get priority in conflicts.

**Pros**: Sybil-resistant.
**Cons**: Subjective, excludes new users, not aligned with cypherpunk ethos.

## What We Landed On (For Now)

**First-come-first-served with deterministic tiebreaker.**

For the nodns.shop PoC:
- Domains cost 2 sats in worthless test tokens
- The attack surface (squatting for profit) is minimal
- The operator's bot is the clock
- Lower event ID wins ties (deterministic, can't be gamed)

This is explicitly a simplification for the PoC. Race condition handling is a **future research area** that becomes important when:
- Domains have real economic value
- Multiple operators compete for the same namespace
- Users need strong guarantees about registration finality

## Why This Is Deferred

1. **nodns.shop is a PoC**: Domains are cheap and denominated in worthless test tokens. The incentive for squatting is minimal.
2. **No real money at stake**: Commit-reveal adds significant UX complexity that isn't justified for 2-sat test domains.
3. **Operator reputation**: If the operator mishandles registration conflicts, it's visible in the event log.
4. **Can be upgraded later**: The registration protocol can be extended with commit-reveal without breaking existing registrations.

## The Domain Squatter Problem

A related concern: what if someone registers every short domain name as soon as they become available?

**Mitigations**:
- Price tiers: short names (1-3 chars) cost more, reducing blanket squatting
- Usage requirements: if a domain has no DNS records after N days, it goes back to the pool (future option)
- Reputation: serial squatters develop a visible pattern in the event log
- Takeover mechanism: existing domains can be challenged by paying a premium (see [Open Questions](26-open-questions.md))

For the PoC, this is a minor concern. For a production system, it would need real thought.

## What a Future Protocol Might Look Like

```
Phase 1: Commit    → user publishes hash(commitment) 
Phase 2: Reveal    → user publishes commitment (must match hash)
Phase 3: Finalize  → highest valid bid wins, others get refund
Phase 4: Delegate  → operator signs delegation to winner
```

This is how Handshake works and is a well-understood pattern. The question is whether the added complexity is justified for the use case.
