# NoDNS — Project Overview

## What is NoDNS?

NoDNS is a protocol that resolves DNS records from Nostr events. Instead of registering domains through a traditional registrar and configuring DNS through a control panel, users publish cryptographically-signed events to Nostr relays. A NoDNS-compatible nameserver reads these events and serves them as standard DNS responses.

**Live at [nodns.shop](https://nodns.shop)** — DNSSEC-validated, Rust-powered, production-ready.

Core event type: **kind 11111** — DNS record events with a fixed 11-element `record` tag.

Protocol spec: [11-protocol-spec-v0.1.md](11-protocol-spec-v0.1.md)

## Current Status

| Component | Status |
|---|---|
| **Rust bot** | Deployed, running as systemd service |
| **Knot DNS** | Serving `nodns.shop` zone on `46.224.104.12` |
| **DNSSEC** | Fully validated — `ad` flag confirmed on Google, Cloudflare, Quad9 |
| **Frontend** | Monolithic HTML at nodns.shop (Next.js rebuild in progress) |
| **Cashu payments** | Code complete, gated off pending mint configuration |
| **Secondary DNS** | `puck.nether.net` (Michigan, USA) via AXFR/IXFR |

## Infrastructure

| Server | IP | Software | Role |
|---|---|---|---|
| `ns1.nodns.shop` | 46.224.104.12 | Knot DNS 3.3.4 | Primary authoritative |
| `puck.nether.net` | 204.42.254.5 | BIND | Secondary (AXFR) |

Knot DNS uses RCU-based lock-free zone updates and automatic DNSSEC re-signing after every DDNS update.

## Project Goals

1. **Production PoC**: `nodns.shop` as a live, DNSSEC-signed, Nostr-driven DNS zone
2. **Polished demos**: Interactive web UI + CLI demos for mixed audiences
3. **Protocol compliance**: Kind 11111 events with record, delegation, registrar, and payment tags
4. **DNSSEC**: Full chain of trust (Root → .shop → nodns.shop) with `ad` flag
5. **Open source**: Clean codebase, comprehensive docs, reproducible deployment

## Future

- `.cv` integration remains aspirational — see [10-cv-integration.md](10-cv-integration.md)
- Custom name registration with Cashu payments — see [09-custom-names.md](09-custom-names.md)
- SLIP-10 nsec→DNSSEC key derivation — see [15-nsec-to-dnssec-analysis.md](15-nsec-to-dnssec-analysis.md)

## Document Index

| Document | Content |
|---|---|
| [01-overview.md](01-overview.md) | This file — project context and goals |
| [02-architecture.md](02-architecture.md) | System design, Knot DNS analysis, DDNS mechanism |
| [03-bot-spec.md](03-bot-spec.md) | Bot specification (Go — archived; see Rust source for current spec) |
| [08-deployment-status.md](08-deployment-status.md) | VPS setup, zone file, propagation verification |
| [09-custom-names.md](09-custom-names.md) | Custom name registration with Cashu payments |
| [11-protocol-spec-v0.1.md](11-protocol-spec-v0.1.md) | Protocol specification v0.1 |
| [12-dnssec-setup.md](12-dnssec-setup.md) | Production DNSSEC deployment reference |
| [13-nostr-dnssec-derivation.md](13-nostr-dnssec-derivation.md) | SLIP-10 key derivation research |
| [14-demo-recipes.md](14-demo-recipes.md) | 7 demo scripts with exact commands |
| [15-nsec-to-dnssec-analysis.md](15-nsec-to-dnssec-analysis.md) | 5-approach nsec→DNSSEC tradeoff analysis |
