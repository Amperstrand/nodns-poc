# NoDNS — DNS Records from Nostr Events

> **A thought experiment in decentralized naming.** Nothing here is production. The protocol is an experimental draft.

**Live demo**: [nodns.shop](https://nodns.shop) · **Beta**: [beta.nodns.shop](https://beta.nodns.shop) · **GitHub Pages**: [nodns-poc pages](https://amperstrand.github.io/nodns-poc/)

The [GitHub Pages site](https://amperstrand.github.io/nodns-poc/) explains the thought experiment, consensus rules, and design philosophy. What follows here is purely technical — how to build, configure, and run the software.

---

## Architecture

| Component | Tech | Purpose |
|---|---|---|
| **nodns-bot** | Rust (nostr-sdk, hickory, axum) | Subscribes to Nostr relays, validates events, pushes DDNS updates |
| **Knot DNS** | 3.3.4 | Authoritative nameserver with DNSSEC signing (ECDSAP256SHA256) |
| **Frontend** | Next.js (static export) | Key generation, record publishing, live feed |
| **Caddy** | Reverse proxy | Serves frontend at nodns.shop, proxies `/api/*` to bot |

```
User publishes kind 11111 event to Nostr relay
        │
        ▼
  nodns-bot (Rust) subscribes to relays
        │
        ├─ Validates event signature
        ├─ Checks authority (npub owns the subdomain)
        ├─ Verifies Cashu payment (anti-spam, 250 sats/record)
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
| `["cashu", TOKEN, MINT, AMOUNT]` | Cashu payment proof | `["cashu", "cashuA...", "https://mint.example.com", "250"]` |

Full spec: [docs/11-protocol-experimental-draft.md](docs/11-protocol-experimental-draft.md)

## Project Structure

```
nodns-poc/
├── content/                # Compiled JSON for websites (derived from docs/)
│   └── consensus.json      # Curated public summary of consensus rules
├── docs/                   # Single source of truth — see docs/README.md
│   ├── README.md           # Doc index with status badges (ACTIVE/DRAFT/ARCHIVED)
│   ├── 11-protocol-experimental-draft.md  # Protocol specification
│   ├── 20-26               # Design philosophy series (consensus, naming, payments...)
│   ├── 27-30               # Implementation plans
│   └── competitive/        # Competitive analysis (ENS, Handshake, etc.)
├── gh-pages/               # GitHub Pages static site
│   ├── index.html          # Vanilla HTML/CSS/JS rendering consensus.json
│   └── consensus.json      # Symlink → ../content/consensus.json
├── nodns-bot-rs/           # Rust bot
│   └── src/
│       ├── main.rs         # Entry point, event loop
│       ├── auth.rs         # Authority/delegation checking
│       ├── config.rs       # TOML config with multi-zone support
│       ├── dns.rs          # DDNS updater (RFC 2136 + TSIG)
│       ├── parser.rs       # Nostr event parsing & validation
│       ├── payment.rs      # Cashu token verification (CDK)
│       ├── store.rs        # SQLite persistence
│       ├── subscriber.rs   # Nostr relay subscription
│       └── types.rs        # Shared types
├── nodns-frontend/         # Next.js frontend
├── tests/                  # Playwright E2E tests
└── .githooks/              # Pre-commit hooks (gitleaks)
```

## Development

### Prerequisites

- Rust 1.75+ (for nodns-bot-rs)
- Node.js 18+ (for frontend and Playwright tests)

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

### Run tests

```bash
# Rust unit tests
cd nodns-bot-rs && cargo test

# E2E tests (requires nodns.shop running)
npx playwright test
```

### Deploy to VPS

```bash
cd nodns-frontend
npm run build
scp -r out/* root@46.22.104.104:/var/www/nodns-beta/
```

See [docs/29-beta-deployment.md](docs/29-beta-deployment.md) for full deployment instructions.

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

# [payment]  # Payment is disabled by default
# enabled = true
# required_sats = 250
# update_free = true
# cashu_mint_url = "https://testnut.cashu.space"
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
