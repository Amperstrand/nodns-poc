# NoDNS — DNS Records from Nostr Events

> **A thought experiment in decentralized naming.** Nothing here is production. The protocol is an experimental draft.

**Live demo**: [nodns.shop](https://nodns.shop) · **GitHub Pages**: [nodns-poc pages](https://amperstrand.github.io/nodns-poc/)

The [GitHub Pages site](https://amperstrand.github.io/nodns-poc/) explains the thought experiment, consensus rules, and design philosophy. What follows here is purely technical — how to build, configure, and run the software.

---

## Architecture

| Component | Tech | Purpose |
|---|---|---|
| **nodns-bot** | Rust (nostr-sdk, hickory, axum) | Subscribes to Nostr relays, validates events, pushes DDNS updates |
| **nodns-cli** | TypeScript (commander, nostr-tools) | Local record management: add, delete, list, resolve, conformance, zone ops |
| **nodns-frontend** | Next.js (static export) | Key generation, record publishing, live feed — deployed to GitHub Pages |
| **nodns-registrar** | Vite + Preact (static) | Domain registration UI with Cashu wallet — deployed to Cloudflare Pages |
| **nodns-explorer** | Vite + Preact (static) | DNS event explorer: live feed, zone monitoring, DNS lookups |
| **Knot DNS** | 3.3.4 | Authoritative nameserver with DNSSEC signing (ECDSAP256SHA256) |
| **Caddy** | Reverse proxy | Proxies `/api/*` to bot, redirects `*.nodns.shop` to GitHub Pages |

```
User publishes kind 11111 event to Nostr relay
        │
        ▼
  nodns-bot (Rust) subscribes to relays
        │
        ├─ Validates event signature
        ├─ Checks authority (npub owns the subdomain)
        ├─ Verifies Cashu payment (anti-spam, dynamic pricing by name length)
        │
        ▼
  DDNS UPDATE to Knot DNS (TSIG-signed)
        │
        ▼
  Record live in DNS, automatically DNSSEC-signed
        │
        ▼
  Resolvable globally in ~3 seconds
```

## Quick Start

### Create a DNS record

Publish a kind 11111 Nostr event:

```json
{
  "kind": 11111,
  "content": "",
  "tags": [
    ["record", "TXT", "", "hello from nostr!", "", "", "", "", "", "", "3600"]
  ]
}
```

Your record appears at `<npub>.nodns.shop` within seconds.

### Or use the web UI

Visit [nodns.shop](https://nodns.shop) — the frontend generates an ephemeral keypair and publishes records directly from the browser.

### Verify it worked

```bash
dig @8.8.8.8 <npub>.nodns.shop TXT +short
# "hello from nostr!"
```

## Protocol

All operations use **kind 11111** events. The event type is determined by tags:

| Tag | Purpose | Example |
|---|---|---|
| `["record", TYPE, NAME, RDATA, ...]` | DNS record update | `["record", "A", "", "1.2.3.4", ...]` |
| `["delegation", DOMAIN, NPUB, FROM, UNTIL, RENEW]` | Custom name delegation | `["delegation", "alice.nodns.shop", "npub1...", ...]` |
| `["registrar", ZONE, PUBKEY]` | Registrar key publication | `["registrar", "nodns.shop", "abc..."]` |
| `["cashu", TOKEN, MINT, AMOUNT]` | Cashu payment proof | `["cashu", "cashuA...", "https://testnut.cashu.space", "2"]` |

Full spec: [docs/11-protocol-experimental-draft.md](docs/11-protocol-experimental-draft.md)

## Project Structure

```
nodns-poc/
├── content/                # Compiled JSON for websites (derived from docs/)
│   └── consensus.json      # Curated public summary of consensus rules
├── deploy/                 # Bot deployment scripts
│   ├── deploy.sh           # Cross-compile (cargo zigbuild), scp, swap, restart
│   └── check-expiry.sh     # RDAP domain expiry verification
├── docs/                   # Single source of truth — see docs/README.md
│   ├── README.md           # Doc index with status badges (ACTIVE/DRAFT/ARCHIVED)
│   ├── 11-protocol-experimental-draft.md  # Protocol specification
│   ├── 20-26               # Design philosophy series (consensus, naming, payments...)
│   ├── 27-43               # Implementation plans, research, architecture
│   ├── competitive/        # Competitive analysis (ENS, Handshake, NIP-97)
│   └── examples/           # Integration scripts (certbot, nsupdate, ddclient)
├── nodns-bot-rs/           # Rust bot
│   └── src/
│       ├── main.rs         # Entry point, event loop
│       ├── auth.rs         # Authority/delegation checking
│       ├── acme.rs         # ACME (Let's Encrypt) certificate management
│       ├── classify.rs     # Name/mint classification + enforcement matrix
│       ├── cloudflare_backend.rs  # Cloudflare API DNS backend (DnsConnector impl)
│       ├── config.rs       # TOML config with multi-zone, per-zone payment
│       ├── dns.rs          # DDNS updater (RFC 2136 + TSIG)
│       ├── dns_cache.rs    # Experimental Nostr-over-DNS event caching
│       ├── dns_update_server.rs  # RFC 2136 dynamic update listener
│       ├── dnssec_derivation.rs  # DNSSEC key derivation
│       ├── epp.rs          # EPP bridge to ccTLD registry (.cv pilot)
│       ├── event_processor.rs    # Event validation pipeline
│       ├── handlers/       # HTTP handlers
│       │   ├── mod.rs      # Module exports + route registration
│       │   ├── api.rs      # REST API: records, pricing, availability, zone export
│       │   ├── acme_dns.rs # ACME DNS-01 challenge handler
│       │   ├── acme_order.rs    # ACME certificate ordering
│       │   ├── client_log.rs    # Client-side error log receiver
│       │   ├── dyndns.rs   # DynDNS v2 update endpoint
│       │   ├── health.rs   # Health check endpoint
│       │   └── tls_check.rs# TLS certificate verification
│       ├── nip05.rs        # NIP-05 verification
│       ├── parser.rs       # Nostr event parsing & validation
│       ├── payment.rs      # Cashu token verification (CDK), npub_names_free
│       ├── security_tests.rs    # Security regression tests
│       ├── store.rs        # SQLite persistence, AES-256-GCM for ACME keys
│       ├── subscriber.rs   # Nostr relay subscription
│       ├── tls_derivation.rs    # TLS key derivation
│       └── types.rs        # Shared types
├── nodns-cli/              # TypeScript CLI tool
│   └── src/
│       ├── index.ts        # Entry point, commander command parsing
│       ├── commands/       # key, add, delete, list, resolve, refund,
│       │                   # conformance, zone-check, zone-export
│       └── lib/            # nostr, p2pk, cashu, dns, zones, validation
├── nodns-frontend/         # Next.js frontend (static export to GitHub Pages)
├── nodns-registrar/        # Vite + Preact registrar UI (Cloudflare Pages)
├── nodns-explorer/         # Vite + Preact DNS event explorer
├── nodns-nameserver/       # Go reference impl ($npub.nostr via Khatru, Arjen)
├── pilot/                  # .cv EPP bridge pilot (gitignored)
├── tests/                  # Playwright E2E tests (10 specs)
└── .githooks/              # Pre-commit hooks (gitleaks, fmt, clippy)
```

## Development

### Prerequisites

- Rust 1.95+ (for nodns-bot-rs)
- Node.js 18+ (for frontend, CLI, registrar, explorer, and Playwright tests)

### Build the bot

```bash
cd nodns-bot-rs
cargo build --release
```

### Build the frontend

```bash
cd nodns-frontend
npm install
npm run build
# Static output in out/
```

### Build the CLI

```bash
cd nodns-cli
npm install
npm run build
# Output: dist/index.js
npm link  # optional — makes `nodns` available globally
```

### Run tests

```bash
# Rust unit tests
cd nodns-bot-rs && cargo test

# E2E tests (requires nodns.shop running)
npx playwright test
```

### Deploy

**Frontend** — automatically deployed to GitHub Pages via CI on push to `main`. No manual steps needed.

**Bot** — cross-compile and deploy to VPS:

```bash
deploy/deploy.sh --push
```

This runs `cargo zigbuild --release --target x86_64-unknown-linux-gnu`, scp's the binary to the VPS, and restarts the bot service.

See [docs/29-beta-deployment.md](docs/29-beta-deployment.md) for full deployment details.

### Configuration

The bot reads a TOML config file. Example:

```toml
[server]
bind = "127.0.0.1:9090"

[nostr]
relays = ["wss://relay.damus.io", "wss://nos.lol"]
zone = "nodns.shop"

[dns]
zone = "nodns.shop"
knot_address = "127.0.0.1:53"
tsig_key_name = "nodns-bot"
tsig_key_secret = "base64-encoded-secret"
tsig_algorithm = "hmac-sha256"
default_ttl = 3600

[policy]
max_records = 20
rate_limit = 5
allowed_types = ["A", "AAAA", "CNAME", "TXT", "MX"]
block_private_ip = true
max_txt_length = 512

[store]
path = "records.db"

# [payment]  # Per-zone payment via [[zones.payment]]
# enabled = true
# create_price = 2
# npub_names_free = true
# mint_url = "https://testnut.cashu.space"
#
# [[zones]]
# name = "nodns.shop"
# npub_names_free = true
# [zones.payment]
# enabled = true
# create_price = 2
# mint_url = "https://testnut.cashu.space"
```

## Security

- **DNSSEC**: Zone signed with ECDSAP256SHA256, NSEC3 with 0 iterations (RFC 9276). DS record at registrar.
- **TSIG**: DDNS updates authenticated via HMAC-SHA256. Bot connects from localhost only.
- **Nostr signatures**: Every event cryptographically verified before processing.
- **Input validation**: Private IPs blocked, TXT length capped, record types whitelisted.
- **Rate limiting**: Per-npub rate limits and record count caps.
- **No secrets in git**: Config files with TSIG keys are gitignored. Pre-commit hook runs gitleaks.
- **Privacy**: Frontend generates ephemeral keypairs by default. Using a personal nsec ties your IP to your npub.

## Related

- **Arjen's nodns-nameserver** — The `$npub.nostr` reference implementation ([gitworkshop.dev](https://gitworkshop.dev/npub1hw6amg8p24ne08c9gdq8hhpqx0t0pwanpae9z25crn7m9uy7yarse465gr/nos.lol/nodns-nameserver))

## License

Unlicense. This is a thought experiment and an idea. Do whatever you want with it.
