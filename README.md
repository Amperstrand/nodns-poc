# NoDNS — DNS Records from Nostr Events

Nostr-native DNS management. Publish a Nostr event, get a DNS record. No control panel, no API keys, no human intervention.

Live at [nodns.shop](https://nodns.shop).

## How It Works

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
  "content": "DNS record update",
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

## Architecture

| Component | Tech | Purpose |
|---|---|---|
| **nodns-bot** | Rust (nostr-sdk, hickory, axum) | Subscribes to Nostr relays, validates events, pushes DDNS updates |
| **Knot DNS** | 3.3.4 | Authoritative nameserver with DNSSEC signing (ECDSAP256SHA256) |
| **Frontend** | HTML/JS (being rebuilt in Next.js) | Key generation, record publishing, live feed |
| **Caddy** | Reverse proxy | Serves frontend at nodns.shop, proxies `/api/*` to bot |

### DNSSEC

The zone is fully signed with DNSSEC. The `ad` (Authenticated Data) flag is confirmed across all major resolvers (Google, Cloudflare, Quad9). Full chain of trust: Root → .shop → nodns.shop.

```bash
dig +dnssec @8.8.8.8 nodns.shop SOA
# flags: qr rd ra ad  ← authenticated
```

### Record Types

A, AAAA, CNAME, TXT, MX — with private IP blocking and input validation.

## Protocol

All operations use **kind 11111** events. The event type is determined by tags:

| Tag | Purpose | Example |
|---|---|---|
| `["record", TYPE, NAME, RDATA, ...]` | DNS record update | `["record", "A", "", "1.2.3.4", ...]` |
| `["delegation", DOMAIN, NPUB, FROM, UNTIL, RENEW]` | Custom name delegation | `["delegation", "alice.nodns.shop", "npub1...", ...]` |
| `["registrar", ZONE, PUBKEY]` | Registrar key publication | `["registrar", "nodns.shop", "abc..."]` |
| `["cashu", TOKEN, MINT, AMOUNT]` | Cashu payment proof | `["cashu", "cashuA...", "https://mint.example.com", "250"]` |

Full spec: [docs/11-protocol-spec-v0.1.md](docs/11-protocol-spec-v0.1.md)

## Project Structure

```
nodns-poc/
├── nodns-bot-rs/          # Rust bot (production)
│   └── src/
│       ├── main.rs        # Entry point, event loop
│       ├── auth.rs        # Authority/delegation checking
│       ├── config.rs      # TOML config with multi-zone support
│       ├── dns.rs         # DDNS updater (RFC 2136 + TSIG)
│       ├── parser.rs      # Nostr event parsing & validation
│       ├── payment.rs     # Cashu token verification (CDK)
│       ├── store.rs       # SQLite persistence
│       ├── subscriber.rs  # Nostr relay subscription
│       └── types.rs       # Shared types
├── nodns-bot-archive/     # Go bot (archived, superseded by Rust)
├── docs/                  # Documentation
│   ├── 01-overview.md     # Project overview
│   ├── 02-architecture.md # System design
│   ├── 11-protocol-spec-v0.1.md  # Protocol specification
│   ├── 12-dnssec-setup.md # DNSSEC deployment reference
│   ├── 13-nostr-dnssec-derivation.md  # SLIP-10 research
│   ├── 14-demo-recipes.md # Demo scripts with exact commands
│   └── 15-nsec-to-dnssec-analysis.md  # nsec→DNSSEC tradeoff analysis
├── tests/                 # Playwright E2E tests
└── .githooks/             # Pre-commit hooks (gitleaks)
```

## Development

### Prerequisites

- Rust 1.75+ (for nodns-bot-rs)
- Node.js 18+ (for Playwright tests)

### Build the bot

```bash
cd nodns-bot-rs
cargo build --release
```

### Run tests

```bash
# Rust unit tests
cd nodns-bot-rs && cargo test

# E2E tests (requires nodns.shop running)
npx playwright test
```

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

## License

Private repository. All rights reserved.
