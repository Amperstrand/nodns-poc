# 21 — Name Classes: `$npub.tld` vs `$string.tld`

> **Status**: DRAFT. Active experimentation. The distinction between these two classes is fundamental to NoDNS.

## The Two Classes

NoDNS has two fundamentally different types of domain names, with different trust models, different economics, and different enforcement mechanisms.

### Class 1: `$npub.tld` — Cryptographic Ownership

Example: `npub1ykal2...pa3dl.nodns.shop`

| Property | Value |
|---|---|
| **Owner** | Whoever holds the nsec (private key) |
| **Trust anchor** | Cryptography (no one can forge the signature) |
| **Enforcement** | Mathematical — impossible to override |
| **Operator can seize?** | No |
| **Court can seize?** | No |
| **Cost** | Free to claim. Optional payment for DNS mirroring convenience. |

The npub-derived name is **inalienable**. No one — not the operator, not a court, not ICANN — can produce a valid event for this name without the nsec. The relationship between nsec and npub is mathematical.

The operator's only power here is **at the convenience layer**: they can choose not to mirror the npub's records to traditional DNS. But:
- NoDNS resolvers will still resolve it (they watch relays directly)
- The npub holder can publish their records to any relay
- Anyone running a NoDNS resolver (even on a home router) can resolve it

### Class 2: `$string.tld` — Delegated Ownership (Lease)

Example: `alice.nodns.shop`

| Property | Value |
|---|---|
| **Owner** | Whoever the operator delegates to (via signed Nostr event) |
| **Trust anchor** | Reputation (operator's honesty) |
| **Enforcement** | Social — operator loses reputation if they cheat |
| **Operator can seize?** | Technically yes, but they lose reputation |
| **Court can seize?** | At the DNS layer, yes. At the Nostr layer, the delegation event remains valid. |
| **Cost** | Paid lease. Price locked at registration time. |

Delegated names are a **lease**, not property. The operator agrees to delegate control for a period in exchange for payment. The enforcement mechanism is reputation: if the operator breaks their promise, the Nostr event log proves it, and users migrate to a different operator.

## Why This Distinction Matters

This isn't just a pricing difference — it's a **different trust model**:

```
$npub.tld:  "I control this because I have the private key, and no one can take it from me."
$string.tld: "I control this because the operator promised to delegate it to me for a period."
```

The first is a cryptographic fact. The second is a social contract made verifiable by public Nostr events.

## The Economic Model

### `$npub.tld` Economics

The operator pays real costs for DNS mirroring: storage, compute, DNSSEC signing, bandwidth. While the npub holder has an inalienable right to the name, the operator has no obligation to mirror it to traditional DNS for free.

This creates a natural model:
- **NoDNS resolution**: Always free. The npub holder publishes events, NoDNS resolvers pick them up.
- **Traditional DNS mirroring**: The operator may charge for the convenience. The npub holder pays the operator to keep their records in Knot DNS / traditional DNS infrastructure.

If the npub holder doesn't pay for mirroring, their domain still resolves — just not through traditional DNS. A NoDNS-aware resolver (or a browser extension, or a custom DNS server) would still find and resolve it.

### `$string.tld` Economics

Delegated names have a lease cost:
- The user pays the operator for the lease (e.g., 2 sats/month)
- The price is locked at registration time (the registration event is the contract)
- Renewals are at the locked price, not current market price
- The operator may also charge for DNS mirroring on top of the lease

## Challenge: Operator Incentive Alignment

The operator has costs: DNS infrastructure, DNSSEC signing, storage, compute. They need revenue to cover these costs. The two revenue streams are:

1. **Lease payments** from delegated names (`$string.tld`)
2. **Mirroring fees** from npub names (`$npub.tld`) who want traditional DNS resolution

If the operator charges too much, users will:
- Use NoDNS resolution only (bypassing the operator's DNS infrastructure)
- Migrate to a different operator
- Run their own NoDNS resolver

This creates a natural market equilibrium: the operator can't overcharge because users have alternatives.

## What We Landed On (For Now)

1. **`$npub.tld` is free to claim and inalienable**. The operator may charge for DNS mirroring convenience.
2. **`$string.tld` is a paid lease** with price locked at registration time.
3. **Two separate payment flows**: lease payments (for `$string.tld`) and mirroring fees (optional, for both).
4. **All of this is a draft**. The economics may need adjustment as we learn from the PoC.

### v1 Implementation for nodns.shop

Two concrete flows:

**Flow 1: `$npub.nodns.shop` (Cashu antispam for record creation)**
- Name claim: free (your npub IS your name)
- DNS record creation: 2 sats via Cashu (testnut tokens only)
- DNS record update: free
- DNS record delete: free
- Trust model: cryptographic (nsec controls the name, no one can override)

**Flow 2: `$string.nodns.shop` (Legacy-style registration via Cashu over Nostr)**
- Name registration: paid via Cashu (testnut tokens only), price depends on name
- DNS record creation/update/delete: follows same rules as `$npub` names
- Lease duration: bounded by operator's own domain expiry
- Renewal: at price locked in registration event, operator-independent
- Trust model: reputation-based (operator honors delegations or loses reputation)

## Alternatives Considered

| Alternative | Why Not (For Now) |
|---|---|
| All names are paid leases | Breaks the cryptographic ownership model for npub names |
| All names are free | No revenue for operators, unsustainable |
| No distinction between npub and string names | Different trust models require different economics |
| Names are permanent property, not leases | Operator's own domain lease is finite; can't promise permanence |
