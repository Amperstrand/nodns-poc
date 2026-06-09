# 28 — Wallet Integration & Registrar Redesign

> **Status**: ACTIVE. Complete frontend redesign to add Cashu wallet (coco) and registrar-style UX.

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Wallet SDK | coco (coco-cashu) | IndexedDB, React hooks, event bus — wraps cashu-ts |
| UX model | Porkbun-inspired multi-page | Clean, transparent, best registrar UX from research |
| Mint | Hardcoded `testnut.cashu.space` | Single mint, zero config, matches testnet PoC |
| Identity | Generate nsec, store in localStorage | Simple, familiar. User exports for backup. |
| Nostr keys | Persistent across sessions | Required for wallet + domain ownership |

## Architecture

### Wallet Layer (coco)
```
coco-cashu-core     → Manager, APIs, event bus, proof management
coco-cashu-indexeddb → Browser storage for proofs, quotes, counters
coco-cashu-react     → React hooks + context providers
```

Initialization:
1. App loads → check localStorage for existing nsec
2. If none → generate new nsec, store encrypted in localStorage
3. Initialize coco Manager with IndexedDB repos + seed from nsec
4. Register `testnut.cashu.space` mint
5. WalletProvider wraps app — all pages have access to wallet state

### Page Structure (Porkbun-inspired)

```
/                     → Landing page with big search bar
/search?q=alice       → Search results: availability + pricing
/register?name=alice  → Checkout: confirm name + Cashu payment
/dashboard            → My Domains: table with status/expiry/actions
/domain/[name]        → Domain detail: DNS records + renewal
/wallet               → Wallet: balance, history, receive, send
```

### Component Architecture

```
app/layout.tsx
├── WalletProvider (coco Manager + React context)
├── IdentityProvider (nsec generation + localStorage persistence)
├── SiteHeader (nav: Search | Dashboard | Wallet)
└── pages/
    ├── Landing (search bar + hero)
    ├── SearchResults (availability table + register CTA)
    ├── Register (name + price + Cashu send flow)
    ├── Dashboard (domain table: name | status | expires | actions)
    ├── DomainDetail (DNS record editor + renewal + delegation info)
    └── Wallet (balance, history, receive token, send token)
```

## Phase Breakdown

### Phase A: Foundation — coco integration + identity persistence
- Install coco packages (core, indexeddb, react)
- Create WalletProvider + IdentityProvider
- Initialize coco Manager with testnut.cashu.space
- Generate/persist nsec in localStorage
- Show wallet balance in a debug widget

### Phase B: Registrar shell — routing + layout + search
- Add multi-page routes (/, /search, /register, /dashboard, /domain/[name], /wallet)
- Build SiteHeader with nav
- Landing page with big search bar
- Search results page: name availability check + pricing display
- Register page: name confirmation + Cashu payment flow

### Phase C: Dashboard — domain list + DNS editor
- Dashboard page: table of owned domains (from Nostr events + backend API)
- Domain detail page: DNS record table with inline edit
- Add/delete/update records with Cashu payment
- Record type selector (TXT, A, AAAA, CNAME, etc.)

### Phase D: Wallet page — full wallet UX
- Balance display with mint info
- Receive: paste cashuB token → decode → add to wallet
- Send: create token for specified amount → copy to clipboard
- Transaction history: all mints, melts, sends, receives
- Auto-redeem watchers via coco

### Phase E: Polish — renewal + notifications + edge cases
- Renewal flow on domain detail page
- Expiry warnings in dashboard
- Grace period status display
- Error handling for insufficient balance
- Mobile-responsive layout

## Key Flows

### Domain Search & Registration
1. User types "alice" in search bar
2. Frontend checks availability via `/api/records?domain=alice.nodns.shop` or a new endpoint
3. Shows: "alice.nodns.shop is available! 20 sats/year"
4. User clicks "Register"
5. Register page shows: name, price, wallet balance
6. If balance sufficient → auto-send Cashu token via coco
7. Publish claim event to Nostr relay with payment tag
8. Wait for bot to process → show success
9. Redirect to domain detail page

### DNS Record Management
1. User navigates to domain detail for "alice.nodns.shop"
2. Page shows current DNS records in a table
3. User clicks "Add Record" → inline row appears
4. Select type (TXT), enter name/value
5. If payment required → auto-send from wallet
6. Publish kind 11111 event with record tag + payment tag
7. Wait for DNS propagation → show record in table

### Wallet: Receive Tokens
1. User clicks "Receive" on wallet page
2. Pastes cashuB token string
3. coco decodes and validates token
4. Proofs stored in IndexedDB
5. Balance updates via event bus

### Wallet: Send Tokens (for payment)
1. Registration/record creation triggers payment
2. coco `send.prepareSend(mintUrl, amount)` → creates pending operation
3. `send.executePreparedSend()` → generates token
4. Token attached to Nostr event as payment tag
5. `send.finalize()` after bot confirms acceptance
6. Balance updates via event bus

## Technical Notes

### coco Manager Initialization
```typescript
import { initializeCoco, MemoryRepositories } from 'coco-cashu-core';
import { IndexedDBRepositories } from 'coco-cashu-indexeddb';

const MINT_URL = 'https://testnut.cashu.space';

const seedGetter = async () => {
  // Derive from stored nsec
  const nsec = localStorage.getItem('nodns-nsec');
  // ... decode to bytes
  return seedBytes;
};

const repos = new IndexedDBRepositories();
const manager = await initializeCoco({
  repo: repos,
  seedGetter,
  logger: new ConsoleLogger('nodns', { level: 'info' }),
});

await manager.mint.addMint(MINT_URL);
```

### Identity Persistence
```typescript
// Generate
const sk = generateSecretKey(); // from nostr-tools
const nsec = nip19.nsecEncode(sk);
localStorage.setItem('nodns-nsec', encrypt(nsec, password));

// Load
const encrypted = localStorage.getItem('nodns-nsec');
const nsec = decrypt(encrypted, password);
const sk = nip19.decode(nsec).data;
```

### Payment Flow (auto from wallet)
```typescript
// When payment needed for registration
const prepared = await manager.send.prepareSend(MINT_URL, priceSats);
const { token } = await manager.send.executePreparedSend(prepared.operationId);

// Attach token to Nostr event
const tags = [
  ['claim', name, zone, validUntil],
  ['cashu', getEncodedToken(token), MINT_URL, priceSats.toString()],
];

// After bot accepts → finalize
await manager.send.finalize(prepared.operationId);
```

## Migration from Current Frontend

### Keep
- `lib/nostr.ts` — Nostr event publishing/subscribing (modify to use persistent keys)
- `lib/dns.ts` — Cloudflare DoH client
- `lib/validation.ts` — Record validation
- `lib/api.ts` — Backend API client (extend)
- `lib/tls-derivation.ts` — TLS key derivation
- `lib/csr-generator.ts` — CSR generation
- `components/ui/*` — Base UI components

### Replace
- `app/page.tsx` → multi-page routing
- `components/dashboard.tsx` → split into multiple pages
- `components/hero.tsx` → new landing with search
- `components/record-browser.tsx` → part of domain detail page
- `components/live-feed.tsx` → optional, keep or remove

### Add
- WalletProvider, IdentityProvider (contexts)
- Search page + results
- Register/checkout page
- Dashboard page (domain list)
- Domain detail page (DNS editor)
- Wallet page (balance, history, send, receive)
- coco packages (core, indexeddb, react)

## Dependencies to Add

```json
{
  "coco-cashu-core": "^1.0.1",
  "coco-cashu-indexeddb": "^1.0.1",
  "coco-cashu-react": "^1.0.1"
}
```

Note: Check exact package names on npm — coco may use scoped package names.
