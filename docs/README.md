# NoDNS Documentation

> **`docs/` is the single source of truth.** `content/` contains compiled/curated JSON derived from these docs for the website and GitHub Pages. If something in `content/` contradicts a doc here, the doc wins.

## Status Definitions

| Status | Meaning |
|---|---|
| **ACTIVE** | Current, authoritative, being maintained |
| **DRAFT** | In-progress thinking, subject to change |
| **ARCHIVED** | Historical reference only, not current |
| **SUPERSEDED** | Replaced by a newer doc (linked) |

## Document Index

### Project & Infrastructure

| Doc | Status | Description |
|---|---|---|
| [01-overview.md](01-overview.md) | ACTIVE | Project context, goals, infrastructure status |
| [02-architecture.md](02-architecture.md) | ACTIVE | System design, Knot DNS integration, DDNS mechanism |
| [04-demo-setup.md](04-demo-setup.md) | ACTIVE | VPS setup guide for a NoDNS zone |
| [08-deployment-status.md](08-deployment-status.md) | ACTIVE | Production deployment details for nodns.shop |
| [29-beta-deployment.md](29-beta-deployment.md) | ACTIVE | beta.nodns.shop deployment alongside main site |
| [32-demo-infrastructure.md](32-demo-infrastructure.md) | ACTIVE | dns4sats dual-resolution demo, DoH resolver, .nostr TLD setup |
| [33-faq.md](33-faq.md) | ACTIVE | Common questions, gotchas, and misconceptions |
| [34-backwards-compatible-apis.md](34-backwards-compatible-apis.md) | ACTIVE | DynDNS v2, acme-dns, and RFC 2136 backwards-compatible DNS APIs |
| [35-bot-deployment-runbook.md](35-bot-deployment-runbook.md) | ACTIVE | Bot deployment runbook: cross-compile, upload, restart, verify |

### Protocol

| Doc | Status | Description |
|---|---|---|
| [11-protocol-experimental-draft.md](11-protocol-experimental-draft.md) | ACTIVE (experimental draft) | Wire format: kind 11111, record/delegation/registrar/payment tags |

### Design Philosophy

These docs are the authoritative source for reasoning. `content/consensus.json` is the curated public-facing summary compiled from them.

| Doc | Status | Description |
|---|---|---|
| [20-design-philosophy.md](20-design-philosophy.md) | DRAFT | Three layers (Truth/Consensus/Convenience), dual-consensus model, "Why this matters" |
| [21-name-classes.md](21-name-classes.md) | DRAFT | `$npub.tld` vs `$string.tld` — cryptographic vs delegated ownership |
| [22-pricing-and-payments.md](22-pricing-and-payments.md) | DRAFT | Per-operator pricing, Cashu payment model |
| [23-lease-and-renewal.md](23-lease-and-renewal.md) | DRAFT | Lease lifecycle, operator-independent renewal, grace periods |
| [24-race-conditions.md](24-race-conditions.md) | DRAFT | Registration conflict resolution (open research) |
| [25-censorship-resistance.md](25-censorship-resistance.md) | DRAFT | How resolution works under censorship, home router scenario |
| [26-open-questions.md](26-open-questions.md) | DRAFT | PoB, auctions, OTS, MuSig, multi-operator — future research |

### Implementation

| Doc | Status | Description |
|---|---|---|
| [03-bot-spec.md](03-bot-spec.md) | ARCHIVED | Original Go bot spec. Rust source (`nodns-bot-rs/src/`) is now authoritative. |
| [27-implementation-plan.md](27-implementation-plan.md) | ACTIVE | v1 payment and registration implementation plan |
| [28-wallet-and-registrar-redesign.md](28-wallet-and-registrar-redesign.md) | ACTIVE | Frontend redesign: Cashu wallet, Porkbun-inspired UX |
| [30-payment-architecture.md](30-payment-architecture.md) | DRAFT | Cashu P2PK to registrar payment flow |

### Security & DNSSEC

| Doc | Status | Description |
|---|---|---|
| [12-dnssec-setup.md](12-dnssec-setup.md) | ACTIVE | Production DNSSEC deployment reference |
| [13-nostr-dnssec-derivation.md](13-nostr-dnssec-derivation.md) | ACTIVE | SLIP-10 key derivation — IMPLEMENTED, LIVE in production |
| [15-nsec-to-dnssec-analysis.md](15-nsec-to-dnssec-analysis.md) | ACTIVE | 5-approach nsec→DNSSEC tradeoff analysis |
| [16-crypto-key-cert-patterns.md](16-crypto-key-cert-patterns.md) | ACTIVE | Cryptographic key and certificate patterns |
| [17-acme-dns01-trust-analysis.md](17-acme-dns01-trust-analysis.md) | ACTIVE | ACME DNS-01 challenge trust analysis |
| [18-client-side-tls-security.md](18-client-side-tls-security.md) | ACTIVE | Client-side TLS security model |
| [19-audit-findings.md](19-audit-findings.md) | ACTIVE | Security audit findings and mitigations |
| [31-dnssec-trust-architecture.md](31-dnssec-trust-architecture.md) | ACTIVE | Three-layer trust model, soft fork, attestation, dual-KSK |

### Operations & Outreach

| Doc | Status | Description |
|---|---|---|
| [07-abuse-philosophy.md](07-abuse-philosophy.md) | ACTIVE | DNS-as-mirror principle, abuse handling, accountability |
| [08-implementation-plan.md](08-implementation-plan.md) | SUPERSEDED by [27](27-implementation-plan.md) | Earlier implementation plan |
| [09-custom-names.md](09-custom-names.md) | DRAFT | Custom name registration research |
| [14-demo-recipes.md](14-demo-recipes.md) | ACTIVE | 7 demo scripts with exact commands |

### Competitive

| Doc | Status | Description |
|---|---|---|
| [competitive/01-competitive-analysis.md](competitive/01-competitive-analysis.md) | ACTIVE | ENS, Handshake, Unstoppable Domains comparison |
| [competitive/nip97-vs-nodns.md](competitive/nip97-vs-nodns.md) | ACTIVE | NIP-97 (kind 30053) vs NoDNS comparison |

## Relationship to `content/`

```
docs/                           content/
├── 20-design-philosophy.md ──┐
├── 21-name-classes.md ───────┤
├── 22-pricing-and-payments.md ┤──► consensus.json (curated public summary)
├── 25-censorship-resistance.md ┤
└── 26-open-questions.md ─────┘
```

- **`docs/`** = authoritative source with full reasoning, alternatives considered, open questions
- **`content/consensus.json`** = curated JSON for websites (comparison table, 9 models, status badges, principle)
- **If they conflict**: docs win. The JSON is a summary, not the source.

## Adding a New Doc

1. Create `docs/NN-title.md` with a status header: `> **Status**: ACTIVE/DRAFT/ARCHIVED/SUPERSEDED`
2. Add it to this README index
3. If it contains public-facing content, update `content/consensus.json` (or create a new JSON file in `content/`)
