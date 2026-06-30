# ADR-006: nodns SDK — Dual-Language Resolution Library

> **Status**: ACCEPTED

## Context

nodns has 4 TypeScript apps (frontend, CLI, explorer, registrar) and 1 Rust bot, all with duplicated resolution code:

- DNS lookup: 3 copies (frontend, CLI, explorer)
- Nostr event fetching: 3 copies (frontend, CLI, explorer)
- Zone discovery: 3 copies (CLI, explorer, registrar)
- Record parsing: 2 copies (frontend, explorer)
- Tripartite comparison: 3 copies (frontend, CLI, explorer)

13 duplicate code paths need consolidation. Additionally, third-party integrations (Nostr clients, Lightning wallets, IPFS gateways) need a simple way to resolve nodns names.

## Decision

Build two SDKs with the same API surface:

1. **`@nodns/resolver`** (TypeScript npm package) — extracts and deduplicates existing TS code
2. **`nodns-resolver-rs`** (Rust workspace crate) — used by the bot and connectors

### API Design

Inspired by ENS naming conventions, but with Nostr-native internals:

```typescript
const resolver = createResolver({ mode: 'tripartite', relays: [...] });
const records = await resolver.resolve('npub1abc....nodns.shop', 'A');
const result = await resolver.resolveVerified('alice.nodns.shop', 'TXT');
const names = await resolver.reverse('npub1abc...');
const zones = await resolver.discoverZones();
```

### Resolution Modes

| Mode | Sources | Verification | Use case |
|---|---|---|---|
| `dns` | DoH only | None | Fast lookup, works everywhere |
| `nostr` | Relay events | Signature verification | Trustless resolution |
| `tripartite` | DNS + Nostr + API | Cross-verification | Maximum trust |

### Module Structure

```
nodns-resolver/ (TS)           nodns-resolver-rs/ (Rust)
├── package.json               ├── Cargo.toml
├── src/                       ├── src/
│   ├── dns.ts                 │   ├── dns.rs       DoH/raw DNS lookup
│   ├── nostr.ts               │   ├── nostr.rs     Relay event fetch
│   ├── verify.ts              │   ├── verify.rs    Tripartite comparison
│   ├── zones.ts               │   ├── zones.rs     Zone discovery
│   ├── parse.ts               │   ├── parse.rs     Event → records
│   ├── types.ts               │   ├── types.rs     Shared types
│   ├── resolver.ts            │   └── lib.rs       Public API
│   └── index.ts               └── ...
└── tsconfig.json
```

### What Gets Extracted

| SDK module | Source files consolidated |
|---|---|
| `dns` | Frontend `lib/dns.ts`, CLI `lib/dns.ts`, Explorer `lib/dns-lookup.ts` |
| `nostr` | Frontend `lib/nostr.ts` (query), CLI `lib/nostr.ts` (fetch), Explorer `lib/nostr.ts` |
| `verify` | Frontend `lib/sources.ts`, Explorer `zone-monitor.tsx` helpers, CLI `zone-file.ts` diff |
| `zones` | CLI `lib/zones.ts`, Explorer `lib/zones.ts`, Registrar `lib/zones.ts` |
| `parse` | Frontend `lib/nostr.ts` (parse), Explorer `lib/event-analysis.ts` |

### Package Structure

Single package per language. Tree-shakeable. No sub-packages.

- TypeScript: `@nodns/resolver` — one import, mode-configurable
- Rust: `nodns-resolver` crate — workspace member alongside `nodns-bot-rs`, `nodns-connectors`

### Bot Integration

The Rust bot will use `nodns-resolver-rs` for:
- DNS queries (replaces `dns.rs` query functions)
- Event parsing (replaces inline parsing in `parser.rs`)
- Zone discovery (if needed)

The bot keeps its own write/update logic (DDNS, event processing) — the SDK is read-only.

### Not Forking ENS

ENS SDKs (ethers.js, viem, ensjs) are deeply tied to Ethereum RPC. The internals are fundamentally different. The API naming is inspired by ENS (`resolve`, `reverse`) but the implementation is built from scratch for Nostr + DNS.

## Consequences

- **Positive**: Eliminates 13 duplicate code paths. Third-party apps can resolve nodns names with one import. Bot shares resolution code with the TS ecosystem.
- **Negative**: Two codebases to maintain (TS + Rust). API surface must stay in sync between languages.
- **Risk**: If the API diverges between TS and Rust, consumers get inconsistent behavior. Mitigated by keeping the API surface minimal (5 functions).
