# NoDNS — AI Agent Guide

> **You are an AI coding agent working on the NoDNS project. Read this file first, then read `AGENTS.md` for the full technical reference.**

## What is NoDNS?

NoDNS turns Nostr events into live DNS records. A user publishes a cryptographically-signed Nostr event (kind 11111) to any relay; a bot subscribes, validates authority and payment, then pushes DNS updates to an authoritative nameserver. Records are DNSSEC-signed and globally resolvable in ~3 seconds.

No registrar. No control panel. No account. Your Nostr key IS your authentication.

## Read these first (in order)

1. **`AGENTS.md`** — Full technical reference: architecture, config, security, data model, source layout, testing, key decisions. 672 lines. This is the single source of truth.
2. **`docs/README.md`** — Documentation index with status badges (ACTIVE/DRAFT/ARCHIVED).
3. **`docs/44-minimal-consensus-roadmap.md`** — Current decision roadmap and talk outline.
4. **`docs/45-architecture-direction.md`** — Two-component architecture direction (DNS connector + payment processor split).

## Locked decisions (DO NOT re-litigate)

These decisions are made. Do not propose alternatives unless explicitly asked.

| Decision | Rationale |
|---|---|
| **Kind 11111 is the protocol** | Not temporary, not legacy. 31111 migration is archived — see #59, `docs/42`. |
| **`$npub.tld` names are free** | Cryptographic ownership — no payment needed. Spam accepted for now; proof-of-burn is future if needed. |
| **`$string.tld` names are paid** | Lease model — zone operator sets price (Model A: fixed published rates). See #32. |
| **No per-record Cashu antispam** | Antispam = namespace access, not per-record taxation. See #34. |
| **No escrow / P2PK model for v1** | Keep it simple. Payment via NIP-17 DM to registrar. Trust-based for PoC. Escrow is future. |
| **Payment is out-of-band** | Cashu token sent via NIP-17 encrypted DM, NOT inside the kind 11111 event. See `docs/45`. |
| **Two-component architecture** | DNS connector (Knot/Cloudflare/EPP) + payment processor (NIP-17 listener). See `docs/45`. |
| **Rust** | All backend code is Rust. Frontend is Next.js static export. |
| **11111 events are zone-agnostic** | Events contain no zone info. The bot writes to whatever zone it's configured for. |
| **Payment disabled in production** | `[payment] enabled = false` in prod config. Infrastructure exists but is off. |

## Current architecture direction

The system is splitting into two components. See `docs/45-architecture-direction.md` for the full spec.

```
User publishes kind 11111 (record claim, no payment tag)
        │
        ▼
  User sends NIP-17 encrypted DM to registrar with Cashu token
        │
        ├── Payment Processor receives DM, verifies Cashu
        ├── DNS Connector pushes record to Knot/Cloudflare/EPP
        └── Registrar replies via DM: confirmed
```

**Key insight**: The current codebase already has most of the pieces:
- `DnsBackend` enum exists with `Ddns` (Knot) + `Cloudflare` variants — the connector abstraction is DONE
- `payment.rs` is already a separate module with `Verifier` — the payment processor exists
- `event_processor.rs` is monolithic — needs refactoring, not rewriting
- EPP bridge (`epp.rs`) exists but is not behind the `DnsBackend` abstraction yet

**Verdict: refactor in place. Do NOT rewrite from scratch.**

## Project structure (where things live)

```
nodns-bot-rs/src/          Rust bot (the core)
  main.rs                  Entry point — boots subscriber + HTTP server
  event_processor.rs       Pipeline: parse → auth → payment → store → DNS
  parser.rs                Kind 11111 tag parsing + validation
  auth.rs                  Authority: npub (free) vs custom (delegation)
  payment.rs               Cashu verification (CDK) — ALREADY MODULAR
  dns.rs                   Knot DDNS (RFC 2136 + TSIG)
  cloudflare_backend.rs    Cloudflare API + DnsBackend enum — ALREADY ABSTRACTED
  epp.rs                   EPP bridge to .cv registry — NOT behind DnsBackend yet
  store.rs                 SQLite (rusqlite, AES-256-GCM for ACME keys)
  subscriber.rs            Nostr relay subscription
  config.rs                TOML config, multi-zone, per-zone payment
  acme.rs                  ACME client (Let's Encrypt, ZeroSSL)
  handlers/                REST API endpoints

nodns-frontend/src/        Next.js static export (GitHub Pages)
nodns-cli/src/             TypeScript CLI
nodns-registrar/           Vite + Preact registrar UI (Cloudflare Pages)
nodns-explorer/            Vite + Preact DNS event explorer
deploy/                    Deployment scripts, configs, runbooks
docs/                      Single source of truth — see docs/README.md
```

## Open issues (as of last update)

See [GitHub Issues](https://github.com/Amperstrand/nodns-poc/issues). Key open items:

| # | Title | Status |
|---|---|---|
| #33 | `$string` registration via Cashu | Ready to implement (pricing decided) |
| #52 | Domain racing/sniping | Needs decision |
| #54 | KSK rollover bug | Operational risk |
| #55 | Refund mechanism | Needs decision |
| #57 | Event flooding (no 31111) | Needs mitigation plan |
| #58 | Relay SPOF | Architecture confirmation |

Closed: #21 (CAA), #32 (pricing decided), #41 (resolver solved), #59 (31111 killed).

## Build & test

```bash
# Bot
cd nodns-bot-rs && cargo build --release && cargo test

# Frontend
cd nodns-frontend && npm install && npm run build

# CLI
cd nodns-cli && npm install && npm run build

# E2E tests
npx playwright test
```

## Conventions

- **No inline comments** in source files (except module-level `//!` docs). Code is self-documenting.
- **Dark-only theme** for frontend. No light mode.
- **npm only** (pnpm-lock.yaml is gitignored).
- **`@base-ui/react`** for UI primitives (not Radix).
- **No secrets in git**. Pre-commit hook runs gitleaks. Config files with real secrets are gitignored.
- **Docs use status badges**: ACTIVE / DRAFT / ARCHIVED / SUPERSEDED. See `docs/README.md`.
