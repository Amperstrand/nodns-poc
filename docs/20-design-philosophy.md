# 20 — Design Philosophy

> **Status**: DRAFT. Active experimentation. Everything in this document is subject to change.
> This is a proof-of-concept exploring whether Nostr can serve as the source of truth for DNS.

## The Core Idea

NoDNS inverts the traditional DNS trust model. Instead of DNS being the authority and everything else being a reflection of it, **Nostr events are the authority** and DNS is a cache.

```
Traditional DNS:  Operator controls DNS → users trust DNS → DNS IS the truth
NoDNS:            Users publish events → events ARE the truth → DNS mirrors events
```

This is a fundamental shift. The signed Nostr event is the deed, not the DNS record.

## Why This Matters

DNS is a centralized system. Domain registrars, registries, and ICANN control the namespace. A court order can seize a domain. A registrar can refuse to renew. The DNS hierarchy means someone above you always has power over your name.

NoDNS asks: **what if the authority lived in cryptographic proofs instead of institutional hierarchies?**

## The Three Layers

NoDNS separates DNS into three distinct layers:

| Layer | What | Trust Model | Cost |
|---|---|---|---|
| **1. Truth** | Nostr events (signed, append-only) | Cryptographic (signatures) | Free to publish |
| **2. Consensus** | NoDNS resolvers (watch relays, verify events) | Deterministic (anyone can run one) | Free to resolve |
| **3. Convenience** | Traditional DNS mirroring (Knot DNS, etc.) | Operational (someone runs the server) | Paid service |

A domain name is **always resolvable** at Layer 2 by anyone running a NoDNS-compliant resolver. Layer 3 (traditional DNS) is a convenience that the domain operator may charge for.

## The Dual-Consensus Model

NoDNS operates under two simultaneous consensus systems:

1. **Traditional DNS consensus**: The operator controls DNS records. This is what the existing DNS hierarchy sees.
2. **NoDNS consensus**: Signed Nostr events establish truth. This is what NoDNS-compliant resolvers see.

When these conflict (e.g., a court orders the operator to change records), NoDNS resolvers honor the Nostr events, not the DNS records. The operator can comply with the court order at the DNS layer while the Nostr layer remains intact.

## What NoDNS Is Not

- **Not a replacement for DNS**: NoDNS augments DNS, it doesn't replace it. Traditional DNS resolution still works.
- **Not a blockchain**: There is no chain, no consensus mechanism, no mining. Nostr relays are the storage layer.
- **Not fully trustless**: Delegated names (`alice.nodns.shop`) still trust the operator. Only `$npub.tld` names are trustless.
- **Not production-ready**: This is a proof-of-concept. Everything is experimental.

## What NoDNS Is Trying to Explore

1. Can Nostr events serve as a verifiable, append-only log for DNS changes?
2. Can domain ownership be provable and censorship-resistant?
3. Can TLD operators adopt Bitcoin-native payments for domain leases?
4. Can we make DNS resolution that doesn't depend on institutional trust?
5. Can the economics work: operators earn from leases and convenience, users get provable ownership?

## The Baseline: Deliberately Simple

NoDNS does not try to solve naming consensus for the whole world. The protocol provides a framework with two simple, uncontroversial rules:

1. **`$npub.tld` resolves as determined by the nsec.** Cryptographic ownership, inalienable, no consensus needed.
2. **`$string.tld` resolves as per agreement on Nostr with the owner of the parent domain.** Delegated ownership, verifiable terms, reputation-enforced.

Everything beyond this — Proof of Burn, auctions, takeovers, commit-reveal, multi-operator consensus — is an **operator-level policy choice**, not a protocol requirement. Different operators can experiment with different policies. The protocol doesn't mandate complexity.

This is intentional. Naming consensus is one of the hard problems in distributed systems. NoDNS takes the pragmatic view: provide the simplest possible baseline that works, and let operators innovate on top.

### The v1 Implementation Scope

For nodns.shop, v1 is:

- **`$npub.nodns.shop`**: Free to claim, controlled by nsec, Cashu antispam for DNS record creation (testnut tokens only)
- **`$string.nodns.shop`**: Legacy-style DNS registration via Cashu payment over Nostr (testnut tokens only)
- **No real value transfer**: Only accept Cashu tokens from mints with "testnut" in the name
- **Everything else is deferred**: PoB, auctions, takeovers, multi-operator, commit-reveal — all future research

### Why Not Proof of Burn (For Now)

Proof of Burn is philosophically elegant (ThomasV's model sends sats to miners, contributing to Bitcoin security) and avoids creating a shitcoin. However:

- **The operator gets nothing**: If sats are burned to miners, the operator has no revenue to cover infrastructure costs. Cashu payments give the operator income.
- **Doesn't solve spam better than Cashu**: Both require economic cost per action. Cashu is simpler and already implemented.
- **Adds Lightning dependency**: PoB requires Lightning → notary → on-chain. Cashu is token-based.
- **"Whoever burned the most" creates deep-pockets problem**: Continuous auction means legitimate owners get outbid by wealthier actors.

PoB may be revisited as an **optional policy** for specific operators who want it. But it's not the v1 default.

## Non-Commercial Focus

NoDNS is primarily a research project and experiment. The goal is not to build a business, but to explore:

- How decentralized naming systems could work
- What consensus rules make sense for DNS ownership
- How Bitcoin micropayments (Cashu, Lightning) can replace traditional billing
- Whether reputation-based enforcement is sufficient for honest operator behavior
- How TLD operators (e.g., `.cv`) could adopt Bitcoin-native domain payments

If a TLD operator wants to use these ideas, great. If not, nodns.shop serves as the playground.

## Relationship to Existing Protocol Spec

The [Protocol Spec v0.1](11-protocol-spec-v0.1.md) defines the current wire format (kind 11111 events, record/delegation/registrar/payment tags). The documents in this series (20-26) describe the **design philosophy and reasoning** behind the protocol — why things are the way they are, what alternatives were considered, and what's still undecided.
