# nodns — Project Documentation

## Architecture

nodns is a decentralized DNS system that turns Nostr events into live DNS records. There is no registrar, no control panel, and no account. A user publishes a cryptographically-signed Nostr event (kind 11111) to any relay; a Rust bot subscribes to those relays, validates authority and payment, then pushes TSIG-signed dynamic DNS updates to an authoritative Knot DNS nameserver. The record is DNSSEC-signed and resolvable globally within seconds.

The system has four independently-deployable components: the **Rust bot** (the core — subscribes to relays, enforces all policy, pushes DDNS updates, serves a REST API behind Caddy), the **frontend** (a Next.js static export hosted on GitHub Pages — key generation, record publishing, live feed, wallet), a **Rust CLI** (local record management), and the **nameserver** (a separate Go repo by Arjen — the `$npub.nostr` reference implementation using Khatru).

```
User publishes kind 11111 Nostr event to relay
        │
        ▼
  nodns-bot (Rust) ── subscribes to relays (nostr-sdk)
        │
        ├─ parser.rs    parse tags, validate record shape
        ├─ auth.rs      verify npub owns the subdomain
        │                   npub-derived names → free
        │                   custom names → NIP-26 delegation required
        ├─ payment.rs   verify Cashu token (CDK) if pricing enabled
        │                   npub_names_free → updates always free
        ├─ event_processor.rs  orchestrates the full pipeline
        │
        ▼
  dns.rs + dns_update_server.rs ── DDNS UPDATE (RFC 2136 + TSIG)
        │
        ▼
  Knot DNS 3.3.4 ── authoritative, DNSSEC-signed (ECDSAP256SHA256)
        │
        ▼
  Record live + globally resolvable (~3 seconds)

  Caddy (reverse proxy) ── /api/* → bot (127.0.0.1:9090)
                           *.nodns.shop → GitHub Pages

  Frontend (Next.js static export) ── GitHub Pages
        publishes events directly from browser via nostr-tools
        Cashu wallet via coco-cashu-core + IndexedDB
```

### Event processing pipeline

`event_processor.rs` is the orchestrator. For every incoming kind 11111 event:

```
subscriber.rs receives event
        │
        ▼
  event_processor::process(event)
        │
        ├─ 1. parser::parse(event)          → ParsedEvent or reject
        │     - extract record / delegation / registrar / payment tags
        │     - block_private_ip: reject RFC 1918, loopback, link-local in A/AAAA
        │     - validate record type against policy.allowed_types
        │     - cap TXT length at policy.max_txt_length
        │
        ├─ 2. auth::check_authority(...)    → Authorized or reject
        │     - npub-derived name (npub prefix) → always authorized
        │     - custom name → requires active NIP-26 delegation tag
        │     - registrar operations → bootstrap registrar_keys or event-published
        │
        ├─ 3. payment::verify(...)          → Paid or reject
        │     - if zone payment disabled → skip
        │     - if npub_names_free and name is npub-derived → skip
        │     - else: verify Cashu token via CDK checkstate
        │     - create_price for new names, update_price for updates
        │
        ├─ 4. store::persist(...)           → SQLite write (events + meta tables)
        │
        └─ 5. dns::send_update(...)         → TSIG-signed DDNS UPDATE to Knot DNS
```

### DNS record lifecycle

```
CREATE:  publish ["record", "A", "", "1.2.3.4", ...]  → DDNS ADD
UPDATE:  publish ["record", "A", "", "5.6.7.8", ...]  → DDNS replace (same name+type)
DELETE:  publish ["record", "A", "", "", ...]          → DDNS delete
CUSTOM:  publish ["record", "A", "alice", "1.2.3.4"]   → alice.<zone> (needs delegation)
NPUB:    publish ["record", "A", "", "1.2.3.4"]        → <npub>.<zone> (free)
```

The empty name string means "use the npub-derived subdomain". A non-empty name is a custom subdomain that requires a delegation tag proving the npub is authorized by the name owner.

### ACME certificate issuance

`acme.rs` implements a full ACME client (Let's Encrypt staging/production + ZeroSSL with EAB). When a user requests a certificate via the frontend:

```
Frontend: POST /api/acme/order { domain, csr_der?, environment?, ca? }
        │  header: X-Nostr-Npub: npub1...
        ▼
  handlers/acme_order.rs
        ├─ create ACME order + authz
        ├─ publish _acme-challenge TXT via dns.rs (DDNS to Knot)
        ├─ poll for validation
        ├─ finalize with CSR (user-provided or server-generated)
        ├─ store cert + encrypted private key in SQLite
        └─ return order_id

Frontend polls: GET /api/acme/order/:id
        → status: pending → challenge_published → verifying → issued | failed
        → certificate_pem + private_key_pem (when issued)
```

Private keys are encrypted at rest with AES-256-GCM (`store.rs`). If `acme.encryption_key` is set in config, that key persists across restarts; otherwise a random key is generated at startup (keys become unreadable after restart).

## Deployment Architecture

### Production VPS

| Component | Detail |
|---|---|
| **Host** | `46.224.104.12` (Ubuntu 24.04 VPS) |
| **Bot** | `nodns-bot-rs` binary, systemd service `nodns-bot`, binds `127.0.0.1:9090` |
| **DNS** | Knot DNS 3.3.4, listens on `127.0.0.1:5353`, authoritative for zones |
| **Proxy** | Caddy 2.11.4, reverse-proxies `/api/*` → bot, `*.nodns.shop` → GitHub Pages |
| **Store** | SQLite at `/opt/nodns-bot/records.db` (rusqlite, `Mutex<Connection>`) |
| **Config** | `/opt/nodns-bot/config.toml` (deployed from `deploy/config-multi-zone.toml`) |

### Zones

- **`nodns.shop`** — production zone. `operator_lease_expires = "2027-06-04"` (RDAP-verified domain expiry, CI-checked monthly via `check-expiry.sh` + `expiry-check.yml` workflow).
- **`beta.nodns.shop`** — beta zone for testing.

Each zone has its own TSIG key, pricing, mint, and lease config. Multi-zone is configured via `[[dns.zones]]` arrays in TOML. Single-zone configs use flat `[dns]` fields (backward compat — `apply_defaults()` synthesizes a zone entry).

### Frontend deployment

Static export to GitHub Pages via the `deploy-pages.yml` workflow on push to `main`:

```
cd nodns-frontend
GITHUB_PAGES=1 NEXT_PUBLIC_API_BASE=https://nodns.shop npm run build
# outputs to out/ → published to amperstrand.github.io/nodns-poc/
# custom domain nodns.shop points at GitHub Pages
```

### Bot cross-compilation

Built on macOS, deployed to Linux VPS:

```bash
deploy/deploy.sh --push
# runs: cargo zigbuild --release --target x86_64-unknown-linux-gnu
# scp's binary to VPS, swaps, restarts systemd service
```

`cargo-zigbuild` is used instead of native cross-compile because it links against glibc without needing a Linux VM or Docker.

## Security Architecture

### Nostr signature verification

Every event is signature-verified by `nostr-sdk` before any processing. Forged events with invalid signatures are dropped at the subscriber level — they never reach `event_processor.rs`. This is the foundational trust layer: no API key, no password, no session. Your Nostr key IS your authentication.

### Authority model (`auth.rs`)

Two name classes with different authority rules:

| Name type | Example | Authority | Cost |
|---|---|---|---|
| **npub-derived** | `npub1abc....nodns.shop` | Event signer (always) | Free (if `npub_names_free`) |
| **custom** | `alice.nodns.shop` | NIP-26 delegation tag required | Paid (if pricing enabled) |

NIP-26 delegation tags prove the zone/name owner authorized this npub to manage the subdomain. The delegation must be active (within `from`/`until` timestamps).

### Registrar keys

Bootstrap trust before any events are seen. Configured in `[registrar_keys]` (zone → hex pubkey) or published via `["registrar", zone, pubkey]` events. Registrar operations (delegation claims, renewals) require a registrar signature.

### Cashu payment verification (`payment.rs`)

When zone payment is enabled, record creation requires a Cashu token:

- Token verified via CDK (Cashu Development Kit) `checkstate` against the zone's mint
- `mint_filter` restricts which mints are accepted (e.g., `"testnut"` for test sats)
- `npub_names_free = true` → npub-derived names bypass payment entirely
- `update_free = true` (default) → updates to existing records are free
- Dynamic pricing by name length and operation type (create/update/delete)

### TSIG-signed DDNS updates

All dynamic DNS updates to Knot DNS are authenticated via HMAC-SHA256 TSIG:

- Bot connects to `127.0.0.1:5353` (localhost only — never exposed externally)
- Each zone has its own `tsig_key_name` + `tsig_key_secret` (base64)
- Config files with real TSIG secrets are gitignored; `config-multi-zone.toml` contains `REPLACE_WITH_REAL_SECRET` placeholders

### DNSSEC

Zone signed with ECDSAP256SHA256, NSEC3 with 0 iterations (per RFC 9276 — tight NSEC3 iterations are pointless and waste CPU). DS record published at the registrar for chain-of-trust.

### Input validation

- **Private IP blocking** (`block_private_ip = true` in production): RFC 1918 (10.x, 172.16-31.x, 192.168.x), loopback (127.x), link-local (169.254.x), and IPv6 equivalents rejected for A/AAAA records
- **Record type whitelist**: only `policy.allowed_types` accepted (A, AAAA, CNAME, TXT, MX by default)
- **TXT length cap**: `max_txt_length = 512` characters
- **Record count cap**: `max_records = 20` per subdomain
- **Rate limiting**: `rate_limit = 5` events per npub per window

### ACME key encryption

ACME private keys are encrypted at rest with AES-256-GCM in `store.rs`. The encryption key comes from `acme.encryption_key` in config (hex-encoded 32-byte key). If unset, a random key is generated at startup — certs work while the bot runs but become unreadable after a restart.

### Secrets hygiene

- Pre-commit hook (`.githooks/pre-commit`) runs **gitleaks** on every commit — blocks commits containing TSIG secrets, ACME keys, or Cashu mnemonics
- Pre-push hook (`.githooks/pre-push`) runs `cargo test` + `next build`
- Config files with real secrets are gitignored
- `.gitleaks.toml` extends the default ruleset

## Data Architecture

### SQLite schema (`store.rs` SCHEMA constant)

The bot uses a single SQLite database (`records.db`) accessed via `rusqlite` wrapped in `Mutex<Connection>` (serialized access — no concurrent writes).

| Table | Purpose |
|---|---|
| `events` | Processed Nostr events (id, npub, kind, created_at, raw JSON) |
| `meta` | Key-value metadata (zone state, counters) |
| `delegations` | Active delegations (domain, npub, from, until, renewal_price, status) |
| `acme_orders` | ACME certificate orders (order_id, domain, status, cert, encrypted key) |
| `acme_order_logs` | Per-order step logs (created_at, stage, message) for debugging |
| `acme_dns_registrations` | DNS-01 challenge TXT records published during ACME |
| `registrar_keys` | Registrar public keys per zone (bootstrap + event-published) |
| `operator_leases` | Domain lease tracking (zone, operator, expiry) |

### ACME key encryption in storage

`acme_orders.private_key_pem` is stored encrypted (AES-256-GCM). The nonce + ciphertext are stored together. Decryption happens on-demand when the frontend fetches an issued certificate.

### No migrations

The schema is defined as a single `SCHEMA` constant in `store.rs` and applied via `CREATE TABLE IF NOT EXISTS` on startup. There is no migration framework — schema changes require code changes and a restart. This is acceptable because the dataset is small and rebuildable from Nostr events.

## Frontend Architecture

### Stack

Next.js 16.2.7 (App Router, static export `output: 'export'`), React 19, Tailwind CSS v4, TypeScript strict. Hosted as a static site on GitHub Pages — no server runtime.

### Design system

Dark-only theme (no light mode). Design tokens defined as CSS custom properties in `globals.css`:

| Token | Value | Usage |
|---|---|---|
| `--background` | `#0a0a0a` | Page background |
| `--foreground` | `#e0e0e0` | Body text |
| `--primary` | `#ff6b35` | NoDNS accent orange (buttons, links, focus rings) |
| `--card` | `#141414` | Card surfaces |
| `--secondary` | `#222222` | Secondary surfaces |
| `--muted` | `#1a1a1a` | Muted backgrounds |
| `--destructive` | `#e74c3c` | Error states |
| `--radius` | `0.625rem` | Base border radius |

Fonts: Geist Sans (`--font-sans`) and Geist Mono (`--font-geist-mono`), loaded via `next/font/google`.

### Component library

UI primitives are shadcn-style components in `src/components/ui/` built on **`@base-ui/react`** (not Radix). The `Button` component wraps `@base-ui/react/button` with `class-variance-authority` for variants (default, outline, secondary, ghost, destructive, link) and sizes (default, xs, sm, lg, icon variants).

### State management

Two React Context providers wrap the app (in `providers.tsx`):

```
IdentityProvider          → npub, nsec, pk (ephemeral keypair via nostr-tools)
  └─ WalletProvider       → coco-cashu Manager (Cashu wallet, IndexedDB-backed)
```

- **`IdentityContext`**: Generates an ephemeral Nostr keypair on first load (`getOrCreateIdentity()`), stored in `localStorage`. Exposes `npub`, `nsec`, `pk`. Using a personal nsec ties your IP to your npub — ephemeral is the privacy-preserving default.
- **`WalletContext`**: Initializes a `coco-cashu-core` Manager with `IndexedDbRepositories`, connects to the configured Cashu mint, enables operation watcher/processor for background mint/finalize.

### API client (`lib/api.ts`)

All backend calls go through `safeFetch()` — a wrapper around `fetch` that:

- Enforces a 30-second timeout via `AbortController`
- Catches network errors → `"Unable to connect. Please check your network."`
- Catches timeouts → `"Request timed out. Please try again."`
- Sanitizes server error messages (strips anything containing `<` or `stack`, caps at 200 chars, falls back to `HTTP {status}`)
- Sends `X-Nostr-Npub` header for authenticated endpoints (ACME orders)

Base URL comes from `NEXT_PUBLIC_API_BASE` env var (via `lib/constants.ts`). Defaults to `""` (same-origin) for local dev; set to `https://nodns.shop` for GitHub Pages production.

### Key libraries

| Library | Purpose |
|---|---|
| `nostr-tools` | Nostr event creation, signing, relay communication |
| `coco-cashu-core` | Cashu wallet (Manager, mint/melt operations) |
| `coco-cashu-indexeddb` | IndexedDB persistence for Cashu proofs |
| `@base-ui/react` | Headless UI primitives (Button) |
| `class-variance-authority` | Variant styling for UI components |
| `tailwindcss` v4 | Utility-first CSS (via `@import "tailwindcss"`) |

## Observability

The bot uses `tracing` for structured logging. The frontend calls `GET /api/health` (served by `handlers/health.rs`) to check bot liveness. ACME order logs are per-step (`acme_order_logs` table) and surfaced in the frontend's `acme-log-display.tsx` component for debugging certificate issuance.

`deploy/check-expiry.sh` performs RDAP lookups on registered domains to verify `operator_lease_expires` hasn't lapsed. The `expiry-check.yml` GitHub Actions workflow runs this monthly.

## Source layout

```
nodns-bot-rs/src/
  main.rs                    Entry point: loads config, spawns subscriber + axum HTTP server
  types.rs                   Shared types: ParsedEvent, Record, Delegation, etc.
  config.rs                  TOML config loading, multi-zone, per-zone payment, backward compat, validation
  parser.rs                  Kind 11111 tag parsing, record validation, private-IP blocking, TXT length cap
  auth.rs                    Authority checking: npub-derived (free) vs custom (NIP-26 delegation)
  payment.rs                 Cashu token verification via CDK, npub_names_free bypass, dynamic pricing
  event_processor.rs         Orchestrates the full validation pipeline (parse → auth → payment → store → DNS)
  store.rs                   SQLite persistence (rusqlite + Mutex<Connection>), SCHEMA const, AES-256-GCM for ACME keys
  dns.rs                     DDNS UPDATE message construction (RFC 2136 + TSIG signing)
  dns_update_server.rs       RFC 2136 dynamic update listener (accepts external DDNS)
  dnssec_derivation.rs       DNSSEC key/record derivation
  tls_derivation.rs          TLSA (DANE) record derivation
  nip05.rs                   NIP-05 identity verification endpoint
  acme.rs                    ACME client: Let's Encrypt staging/production, ZeroSSL EAB, DNS-01 challenge
  epp.rs                    EPP bridge to ccTLD registry (instant-epp 0.4, simulate mode, domain create/delete)
  classify.rs               Name/mint classification + enforcement matrix (Npub/Testing/Custom × Real/Test)
  subscriber.rs              Nostr relay subscription (nostr-sdk), reconnect backoff
  security_tests.rs          Security regression tests
  handlers/
    mod.rs                   Module exports + route registration
    api.rs                   REST API: /api/check, /api/acme/order, pricing, availability
    acme_dns.rs              ACME DNS-01 challenge TXT handler
    acme_order.rs            ACME certificate ordering endpoint
    client_log.rs            Client-side error log receiver (POST /api/client-log)
    dyndns.rs                Dynamic DNS update endpoints
    health.rs                Health check endpoint (/api/health)
    tls_check.rs             TLS certificate verification endpoint

nodns-cli/src/
  main.rs                    CLI entry point, clap command parsing
  config.rs                  CLI config from env/file
  event.rs                   Event construction helpers
  commands/
    mod.rs                   Subcommand module exports
    add.rs                   Add a DNS record (publish kind 11111)
    delete.rs                Delete a DNS record
    list.rs                  List records for an npub
    resolve.rs               Resolve a name via DNS
    key.rs                   Generate/manage Nostr keys

nodns-frontend/src/
  app/
    layout.tsx               Root layout: Geist fonts, Providers wrapper, skip-to-content link
    page.tsx                 Landing page (hero, features, how-it-works)
    globals.css              Design tokens (dark-only theme), Tailwind v4 + shadcn imports
    dashboard/page.tsx       User dashboard (records, wallet)
    register/page.tsx        Domain registration flow
    domain/page.tsx          Domain detail view
    records/page.tsx         Record browser
    search/page.tsx          Name search
    wallet/page.tsx          Cashu wallet UI
    profile/page.tsx         Nostr profile
    learn/page.tsx           Protocol docs / FAQ
    discoveries/page.tsx     Feature discoveries
  contexts/
    IdentityContext.tsx      Ephemeral keypair provider (npub, nsec, pk)
    WalletContext.tsx        Cashu wallet provider (coco-cashu Manager)
  lib/
    api.ts                   API client: safeFetch (timeout, error sanitization), ACME + check endpoints
    constants.ts             API_BASE, RELAYS, DEFAULT_ZONE, DNS type/status maps
    identity.ts              Key generation/persistence (nostr-tools)
    wallet.ts                coco-cashu Manager factory (IndexedDB, mint watcher)
    nostr.ts                 Nostr event helpers
    dns.ts                   DNS lookup helpers
    pricing.ts               Pricing calculations
    validation.ts            Input validation
    tls-derivation.ts        TLSA record derivation (client-side)
    csr-generator.ts         CSR generation for ACME
    sources.ts               Data source definitions
    types.ts                 Shared TypeScript types
    utils.ts                 cn() class merge utility
  components/
    providers.tsx            IdentityProvider + WalletProvider wrapper
    site-header.tsx          Navigation header
    site-footer.tsx          Footer
    hero.tsx                 Landing hero section
    features.tsx             Feature grid
    how-it-works.tsx         Architecture explainer
    architecture.tsx         ASCII architecture diagram component
    dashboard.tsx            Dashboard view
    record-browser.tsx       DNS record browser
    record-browser-teaser.tsx  Record browser preview
    npub-profile.tsx         Nostr profile card
    npub-gate.tsx            Auth gate (requires identity)
    live-feed.tsx            Live event feed
    publish-demo.tsx         Record publishing demo
    publish-pipeline.tsx     Publishing step visualization
    dual-lookup-demo.tsx     DNS + Nostr dual lookup
    cert-request.tsx         ACME certificate request form
    cert-display.tsx         Certificate display
    acme-log-display.tsx     ACME order step logs
    wallet-debug-widget.tsx  Cashu wallet debug panel
    collapsible-section.tsx  Collapsible UI section
    error-boundary.tsx       React error boundary
    source-indicator.tsx     Data source badge
    protocol-spec.tsx        Protocol specification viewer
    consensus.tsx            Consensus rules display
    infrastructure.tsx       Infrastructure diagram
    discoveries.tsx          Discoveries section
    roadmap.tsx              Roadmap timeline
    faq.tsx                  FAQ accordion
    ui/                      shadcn primitives (button, card, dialog, input, etc. — built on @base-ui/react)

nodns-registrar/
  next.config.ts             Static export (output: 'export'), no basePath
  package.json               Next.js 16, React 19, Tailwind v4, coco-cashu-core, @cashu/cashu-ts, nostr-tools
  deploy.sh                  Build + wrangler pages deploy
  playwright.config.ts       E2E test config (baseURL: nodns-registrar.pages.dev)
  tests/                     Playwright E2E specs (5 spec files: landing, login, dashboard, domain-detail, wallet)
  src/
    app/
      layout.tsx             Root layout: Geist fonts, Providers, SiteHeader, ErrorBoundary
      page.tsx               Landing page: domain search, availability, registration flow
      dashboard/page.tsx     User dashboard: domain list, stats, empty state
      domain/page.tsx        Domain detail: record CRUD form, validation, payment
      wallet/page.tsx        Cashu wallet: top-up, send, receive, NUT-18 payment requests
      globals.css            Design tokens (dark-only), Tailwind v4
      icon.svg               Favicon
    contexts/
      IdentityContext.tsx    NIP-07 + nsec + ephemeral login, saved accounts
      WalletContext.tsx      coco-cashu Manager, IndexedDB, NUT-18 creqA generation
    lib/
      api.ts                 API client: checkAvailability, fetchRecords (response mapping), safeFetch
      nostr.ts               Event signing (extension + local), buildRecordTag, buildCashuTag, subscribeToRecords
      identity.ts            Key management: getAccounts, nsecToSeed, getWalletSeed (64-byte)
      wallet.ts              coco-cashu Manager factory (IndexedDB, ConsoleLogger)
      constants.ts           API_BASE, RELAYS (relay.cashu.email, tollgate), DEFAULT_ZONE, MINT_URL
      validation.ts          A/AAAA/CNAME/TXT/MX validation, private IP blocking
      pricing.ts             Name-length-based pricing
      types.ts               Shared types
      utils.ts               cn() class merge
    components/
      providers.tsx          ErrorBoundary + IdentityProvider + WalletProvider
      site-header.tsx        Beta banner, nav (Dashboard, Wallet, npub, Logout)
      login-modal.tsx        4-method login: ephemeral, extension, nsec paste, generate new
      error-boundary.tsx     React ErrorBoundary + global error handlers + localStorage queue + bot flush
      ui/                    Button, Card, Input, Badge (shadcn-style)

pilot/                       .cv EPP bridge pilot (gitignored)
  epp-integration.md         EPP bridge design
  namespace-policy.md        Enforcement matrix, pricing
  knot-cv.conf               Knot DNS config for .cv preview mirror
  knot-zone-management.md    Per-domain zone creation/deletion workflow
  epp-probe/                 Rust binary for EPP connectivity testing

deploy/
  deploy.sh                  Cross-compile (cargo zigbuild), scp, swap binary, restart systemd
  check-expiry.sh            RDAP domain expiry verification for operator leases
  config-multi-zone.toml     Production config template (zones, policy, payment, registrar keys)
  knot-zones.conf            Knot DNS zone configuration
  DEPLOY.md                  Deployment runbook

docs/
  README.md                  Doc index with status badges (ACTIVE/DRAFT/ARCHIVED)
  11-protocol-experimental-draft.md  Kind 11111 protocol specification

tests/                       Playwright E2E specs (9 spec files)
.githooks/
  pre-commit                 gitleaks + cargo fmt + cargo clippy
  pre-push                   cargo test + next build
```

## Config

The bot reads a TOML config file (default `/opt/nodns-bot/config.toml`, local: `config.toml`). Production template is `deploy/config-multi-zone.toml`.

### Configuration sections

| Section | Key | Default | Purpose |
|---|---|---|---|
| `[server]` | `bind` | `127.0.0.1:9090` | HTTP server bind address |
| `[nostr]` | `relays` | *(required, non-empty)* | Nostr relay URLs to subscribe to |
| `[nostr]` | `zone` | *(required)* | Default zone label (use `"multi"` for multi-zone) |
| `[[dns.zones]]` | `zone` | *(required)* | Zone name (e.g., `nodns.shop`) |
| `[[dns.zones]]` | `knot_address` | *(required)* | Knot DNS address (e.g., `127.0.0.1:5353`) |
| `[[dns.zones]]` | `tsig_key_name` | *(required)* | TSIG key name |
| `[[dns.zones]]` | `tsig_key_secret` | *(required)* | TSIG key secret (base64) |
| `[[dns.zones]]` | `tsig_algorithm` | `hmac-sha256` | TSIG algorithm |
| `[[dns.zones]]` | `default_ttl` | `3600` | Default record TTL (seconds) |
| `[[dns.zones]]` | `negative_ttl` | `60` | NXDOMAIN TTL (seconds) |
| `[dns.zones.payment]` | `enabled` | `false` | Enable Cashu payment for this zone |
| `[dns.zones.payment]` | `create_price` | `2` | Sats required to create a name |
| `[dns.zones.payment]` | `update_price` | `0` | Sats required to update |
| `[dns.zones.payment]` | `delete_price` | `0` | Sats required to delete |
| `[dns.zones.payment]` | `npub_names_free` | `true` | npub-derived names bypass payment |
| `[dns.zones.payment]` | `mint_url` | `https://testnut.cashu.space` | Cashu mint URL |
| `[dns.zones.payment]` | `mint_filter` | `testnut` | Accepted mint hostname filter |
| `[dns.zones.lease]` | `grace_period_days` | `30` | Grace period after lease expiry |
| `[dns.zones.lease]` | `max_lease_days` | `365` | Maximum lease duration |
| `[dns.zones.lease]` | `operator_lease_expires` | *(none)* | Domain expiry date (RDAP-verified) |
| `[policy]` | `max_records` | `20` | Max records per subdomain |
| `[policy]` | `rate_limit` | `5` | Events per npub per window |
| `[policy]` | `allowed_types` | `["A","AAAA","CNAME","TXT","MX"]` | Whitelisted DNS record types |
| `[policy]` | `block_private_ip` | `false` | Reject private/loopback IPs in A/AAAA |
| `[policy]` | `max_txt_length` | `512` | Max TXT record length (chars) |
| `[store]` | `path` | `records.db` | SQLite database path |
| `[payment]` | `enabled` | `false` | Global payment (legacy, propagates to zones without zone-level config) |
| `[payment]` | `required_sats` | `250` | Global create price (legacy) |
| `[payment]` | `update_free` | `true` | Updates free globally (legacy) |
| `[payment]` | `cashu_mint_url` | *(empty)* | Global mint URL (legacy) |
| `[registrar_keys]` | `<zone>` | *(none)* | Bootstrap registrar pubkey hex per zone |
| `[registrar]` | `nsec_hex` | *(empty)* | Registrar nsec for signing |
| `[acme]` | `enabled` | `false` | Enable ACME certificate issuance |
| `[acme]` | `environment` | `staging` | `staging` or `production` (resolves directory URL) |
| `[acme]` | `directory_url` | *(from environment)* | Explicit ACME directory URL (overrides environment) |
| `[acme]` | `contact_email` | *(empty → `cert@nodns.shop`)* | ACME account email |
| `[acme]` | `challenge_ttl` | `300` | DNS-01 challenge TXT TTL |
| `[acme]` | `ca` | `letsencrypt-staging` | `letsencrypt-staging`, `letsencrypt-production`, or `zerossl` |
| `[acme]` | `zerossl_eab_kid` | *(empty)* | ZeroSSL EAB Key ID (required for ZeroSSL) |
| `[acme]` | `zerossl_eab_hmac_key` | *(empty)* | ZeroSSL EAB HMAC key (base64, required for ZeroSSL) |
| `[acme]` | `encryption_key` | *(none → random)* | Hex 32-byte AES-256-GCM key for private keys at rest |
| `[dnssec_derivation]` | `enabled` | `false` | Enable DNSSEC record derivation |
| `[dns_update]` | `enabled` | `false` | Enable RFC 2136 listener |
| `[dns_update]` | `listen` | *(empty)* | DDNS listener bind address |
| `[dns_update]` | `tsig_key_name` | *(empty)* | DDNS listener TSIG key |
| `[dns_update]` | `tsig_key_secret` | *(empty)* | DDNS listener TSIG secret |

### Frontend environment variables

| Variable | Production value | Purpose |
|---|---|---|
| `NEXT_PUBLIC_API_BASE` | `https://nodns.shop` | Bot REST API base URL |
| `GITHUB_PAGES` | `1` | Enables static export optimizations for GitHub Pages |

### Protocol: kind 11111 event tags

| Tag | Purpose | Example |
|---|---|---|
| `["record", TYPE, NAME, RDATA, ...]` | DNS record (create/update/delete) | `["record", "A", "", "1.2.3.4", ...]` |
| `["delegation", DOMAIN, NPUB, FROM, UNTIL, RENEW]` | Custom name delegation | `["delegation", "alice.nodns.shop", "npub1...", ...]` |
| `["registrar", ZONE, PUBKEY]` | Registrar key publication | `["registrar", "nodns.shop", "abc..."]` |
| `["cashu", TOKEN, MINT, AMOUNT]` | Cashu payment proof | `["cashu", "cashuA...", "https://testnut.cashu.space", "2"]` |

Full spec: `docs/11-protocol-experimental-draft.md`.

## Testing

### Bot unit tests

```bash
cd nodns-bot-rs
cargo test
```

Unit tests are inline (`#[cfg(test)] mod tests`). `config.rs` has extensive coverage for multi-zone parsing, backward compat, per-zone payment propagation, ACME defaults, and lease config. `security_tests.rs` contains security regression tests for input validation (private IPs, TXT length, record type whitelist).

### Frontend build verification

```bash
cd nodns-frontend
npm install
npm run build     # next build — fails on TypeScript errors (strict mode)
```

### E2E tests (Playwright)

```bash
npx playwright test
```

9 spec files in `tests/`. Base URL: `https://amperstrand.github.io/nodns-poc/` (the GitHub Pages deployment). Tests cover landing page, record publishing, wallet, dashboard, search, and certificate flows. Config in `playwright.config.ts`.

### Git hooks

```bash
# pre-commit: gitleaks + cargo fmt --check + cargo clippy
# pre-push:   cargo test + next build
./.githooks/pre-commit    # install: git config core.hooksPath .githooks
```

Hooks are opt-in via `git config core.hooksPath .githooks`. The pre-commit hook blocks any commit leaking secrets; pre-push blocks pushes that break tests or the frontend build.

### CI workflows (`.github/workflows/`)

| Workflow | Trigger | Purpose |
|---|---|---|
| `build-bot.yml` | push/PR to main | `cargo build --release` + `cargo test` on the bot |
| `deploy-pages.yml` | push to main | Build frontend static export → publish to GitHub Pages |
| `expiry-check.yml` | monthly schedule | RDAP domain expiry verification (`check-expiry.sh`) |
| `ci-register.yml` | push/PR | Registration flow CI checks |

## Key decisions

- **Rust over Go for the bot rewrite**: The original was Go (`nodns-bot/internal/config/config.go`). Ported to Rust for `nostr-sdk` maturity, strong typing, and zero-cost abstractions. `config.rs` is a 1:1 port of the Go config loader (noted in module docs). `hickory-dns` (Rust) provides RFC 2136 DDNS and TSIG without CGO.

- **cargo-zigbuild for cross-compilation**: Cross-compile from macOS to `x86_64-unknown-linux-gnu` without Docker or a Linux VM. `zig` as the linker handles glibc version targeting. Simpler than `cross` (which needs Docker) and faster than spinning up a VM.

- **Knot DNS over BIND9**: Knot DNS 3.3.4 is purpose-built for dynamic DNS updates with DNSSEC online signing. BIND9 requires more manual key management. Knot's `kzoneedit` + online signing means records are signed immediately on DDNS update — no offline signing step.

- **SQLite over PostgreSQL**: Single-writer `Mutex<Connection>` is sufficient at this scale. The dataset is small (processed events, delegations, ACME orders) and fully rebuildable from Nostr events. Zero ops overhead — no database server to run, back up, or replicate.

- **AES-256-GCM for ACME keys at rest**: Private keys for issued certificates are valuable (they can be used to impersonate domains). Encrypted with a config-specified key (`acme.encryption_key`). If unset, a random key is generated at startup — this means keys are functional while the bot runs but unreadable after restart, a deliberate trade-off favoring "never persist unencrypted keys" over "keys survive restarts without configuration".

- **Nostr signatures as the only auth**: No API keys, no passwords, no sessions. Your Nostr key is your identity and authorization. This eliminates credential management entirely — the same key that signs events is used to prove authority. Trade-off: key loss = identity loss (no recovery), but this matches the crypto-native ethos of the project.

- **npub-derived names free, custom names paid**: npub-derived names (`npub1abc....nodns.shop`) are self-evidently owned by the key signer — no coordination needed, so they're free. Custom names (`alice.nodns.shop`) require a delegation proof, which is a coordination/scarce-resource problem, so payment (Cashu) is required as anti-spam. `npub_names_free = true` is the default.

- **Cashu (ecash) over Lightning for anti-spam**: Cashu tokens are included directly in the Nostr event (stateless payment proof). Lightning would require a separate payment step + callback, breaking the "publish one event, done" UX. Cashu's `checkstate` verifies the token hasn't been double-spent. `testnut.cashu.space` provides free test sats.

- **Static export to GitHub Pages**: The frontend has no server runtime — it's a pure static site. This means zero hosting cost, zero server maintenance, and the frontend can never be a bottleneck or single point of failure. The bot API is called cross-origin from the static site. Trade-off: no SSR, no server-side secrets, all key management happens client-side.

- **Ephemeral keypairs by default**: The frontend generates a new Nostr keypair on first visit, stored in `localStorage`. This ties the npub to the browser, not to a persistent identity. Using a personal nsec would link your IP to your npub — ephemeral is the privacy-preserving default. Users who want persistence can import their own key.

- **`@base-ui/react` over Radix for UI primitives**: The shadcn-style `ui/` components wrap `@base-ui/react` instead of Radix UI. Base UI is the headless component library (same author as MUI, unstyled). Provides the same headless primitives with a different API surface.

- **Dark-only theme**: No light mode. The brand is `#0a0a0a` background with `#ff6b35` orange accent — a deliberate aesthetic choice matching the crypto-native, terminal-inspired identity. Simplifies the design system (no dual token sets, no `dark:` variant maintenance).

- **No code comments convention**: Source files contain no inline comments (except protocol-level documentation in module-level `//!` docs). Code is self-documenting via types and naming. The trade-off: faster scanning for experienced Rust devs, higher barrier for newcomers — acceptable in a small-team project.

- **npm only (pnpm-lock gitignored)**: The project uses npm, not pnpm. `pnpm-lock.yaml` is in `.gitignore`. Decision: npm is the lowest-common-denominator package manager — no contributor needs to install pnpm. Trade-off: slower installs than pnpm, but zero onboarding friction.

- **TSIG localhost-only**: The bot connects to Knot DNS on `127.0.0.1:5353` — never exposed externally. TSIG still signs updates (defense in depth), but the localhost binding means only the bot (and other local processes) can send updates. Caddy exposes only the HTTP API to the outside world.

- **operator_lease_expires as config, not runtime**: Domain expiry is a config value (`2027-06-04` for `nodns.shop`), RDAP-verified and CI-checked monthly. The bot doesn't query RDAP at runtime (would add latency and a network dependency). If the lease lapses, the CI check fails loudly rather than the bot silently serving a domain it no longer owns.

- **nodns-nameserver is a separate repo**: Arjen's `$npub.nostr` reference implementation (Go, Khatru relay, 17 DNS record types) is a sibling project, not part of this build. It's documented in the README as related work. Different language (Go), different relay (Khatru vs nostr-sdk), different scope (reference impl vs production bot).

## Known limitations

- **Payment verification is not yet enforced in production**: `deploy/config-multi-zone.toml` has `[payment] enabled = false`. Cashu verification infrastructure (`payment.rs`, CDK integration) is built but disabled pending mint selection and pricing finalization. Anti-spam currently relies on rate limiting and record count caps only.

- **No database migrations**: Schema is a single `CREATE TABLE IF NOT EXISTS` block applied on startup. Schema evolution requires code changes + restart. Acceptable now (small dataset, rebuildable from events), but will need a migration framework if the schema stabilizes and in-place upgrades become necessary.

- **No rate limiting on the HTTP API**: The REST API (`/api/check`, `/api/acme/order`) has no rate limiting. The bot binds localhost only (behind Caddy), so this is Caddy's responsibility. No WAF or edge rate limiting is configured. A targeted abuse of `/api/acme/order` could trigger excessive ACME orders (Let's Encrypt has its own rate limits, which would kick in).

- **ACME certificate scope is limited**: The ACME flow publishes DNS-01 challenges via DDNS to Knot DNS. This works for domains within the bot's zones but the implementation is still maturing. ZeroSSL EAB support is configured but not heavily tested.

- **Single-writer SQLite**: `Mutex<Connection>` serializes all database access. At current scale (personal/experimental), this is fine. Under high concurrency (many simultaneous event processing + API requests), this becomes a bottleneck. Mitigation would be connection pooling or moving to PostgreSQL.

- **Frontend is a proof-of-concept**: The GitHub Pages site (`nodns-poc`) is explicitly labeled as a thought experiment. Several pages (discoveries, roadmap) are informational, not functional. The wallet and certificate flows work but are not hardened for production use.

- **No automated backups**: The SQLite database is not backed up automatically. Since the dataset is rebuildable from Nostr events (replay the relay subscription), data loss is recoverable but requires reprocessing all historical events.
