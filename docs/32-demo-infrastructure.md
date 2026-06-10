# 32 — Demo Infrastructure: dns4sats, DoH, and .nostr

> **Status**: ACTIVE. Production configuration for the dual-resolution demo, DNS-over-HTTPS resolver, and .nostr TLD.

## Overview

The NoDNS demo infrastructure has three components beyond the core bot:

1. **dns4sats.xyz** — Dual-resolution demo domain showing how Cloudflare and NoDNS return different results for the same domain
2. **dns.nodns.shop** — Public DNS-over-HTTPS resolver that routes .nostr and dns4sats.xyz queries to our Knot DNS
3. **.nostr TLD** — Resolves `$npub.nostr` via our DoH endpoint

## dns4sats.xyz Dual-Resolution Demo

### Concept

The Bitcoin "Liar / No you" meme implemented in DNS. One domain, two truths depending on which resolver you ask.

| Query | Standard DNS (8.8.8.8) | NoDNS (dns.nodns.shop) |
|---|---|---|
| `truth.<npub>.dns4sats.xyz` TXT | `"Liar!"` | `"No you!"` |
| `<npub>.dns4sats.xyz` A | `188.114.96.3` (Cloudflare) | `46.224.104.12` (VPS) |
| `<npub>.dns4sats.xyz` web | "Respect my authority" | "Liar" |

### How It Works

**Standard DNS path** (Google/Cloudflare):
```
User → 8.8.8.8 → Cloudflare DNS (CNAME @ → dns4sats.pages.dev)
                                 → Cloudflare Pages ("Respect my authority")
                                 → TXT record "Liar!" (set in Cloudflare dashboard)
```

**NoDNS path** (our VPS):
```
User → dns.nodns.shop → dnsproxy → Knot DNS on VPS
                                        → A record 46.224.104.12 (VPS IP)
                                        → TXT record "No you!" (set by Nostr keyholder)
                                        → Caddy serves "Liar" page at http://dns4sats.xyz
```

### Cloudflare Configuration

**DNS records** (zone: dns4sats.xyz, zone ID: `71009097e6f9ee0e65f4cd254f86e3f2`):

| Type | Name | Value | Proxied |
|---|---|---|---|
| CNAME | `@` | `dns4sats.pages.dev` | Yes |
| CNAME | `<npub>` | `dns4sats.pages.dev` | Yes |
| TXT | `truth.<npub>` | `Liar!` | — |

**Pages project**: `dns4sats` — serves the "Respect my authority" page from `static-dns4sats/index.html`.

### VPS Configuration

**Knot DNS zone** (`/etc/knot/zones/dns4sats.xyz.zone`):
```
@       IN  A    46.224.104.12
<npub>  IN  A    46.224.104.12
truth.<npub> IN TXT "No you!"
```

**Caddy** (`/etc/caddy/Caddyfile`):
```
http://dns4sats.xyz {
    root * /var/www/dns4sats-liar
    file_server
}
```

The "Liar" page is served via HTTP (not HTTPS) because our VPS is not authoritative over dns4sats.xyz in standard DNS — Cloudflare is.

### Demo npub

`npub10mluej6gljwsjx5v4dnr54n9y0yzf8thwr2l60p3e94q72udh8ksz6uw6q`

## DNS-over-HTTPS Resolver (dns.nodns.shop)

### Architecture

```
Client (DoH) → Caddy (HTTPS :443) → dnsproxy (localhost:8053) → Knot DNS (127.0.0.1:53)
                                                                    ↑ .nostr queries
                                                                    ↑ dns4sats.xyz queries
                                                                    ↓ everything else
                                                             Google/Cloudflare DoH upstreams
```

### dnsproxy Configuration

Installed from https://github.com/AdguardTeam/dnsProxy. Runs as systemd service `dnsproxy.service`.

**Key CLI flags** (in systemd unit):
```
--port=0 --https-port=8053
--upstream https://dns.google/dns-query
--upstream https://cloudflare-dns.com/dns-query
--upstream [/nostr/]127.0.0.1:53
--upstream [/dns4sats.xyz/]127.0.0.1:53
```

Domain-specific routing: queries ending in `.nostr` or `dns4sats.xyz` go to local Knot DNS. Everything else goes to Google/Cloudflare DoH.

### Caddy Configuration

```
dns.nodns.shop {
    reverse_proxy localhost:8053
}
```

### Verification

```bash
# DoH query via curl
curl -sH "Accept: application/dns-message" "https://dns.nodns.shop/dns-query?dns=AAABAAABAAAAAAAABW1sYXVlbmNlcgVub2NlcgJubwAAAQAB" | hexdump -C

# DoH query via doggo
doggo TXT mlauenostr.nostr @https://dns.nodns.shop/dns-query

# Standard dig via our DoH (through dnsproxy)
dig @dns.nodns.shop mlauenostr.nostr TXT
```

## .nostr TLD

### How It Works

The `.nostr` TLD is not a real TLD — it's resolved through our DoH endpoint. Any client configuring `dns.nodns.shop` as their DNS resolver will be able to resolve `.nostr` domains.

**Resolution rules**:
- `$npub.nostr` → resolves (cryptographic ownership via Nostr keypair)
- `$string.nostr` → NXDOMAIN (no consensus mechanism for string ownership)
- The bot subscribes to kind 11111 events and creates records for the `.nostr` zone

### Knot DNS Zone (`/etc/knot/zones/nostr.zone`)

The zone file is minimal — the bot adds records via DDNS:

```zone
$TTL 60
@       IN  SOA  ns1.nodns.shop. admin.nodns.shop. (
            2026060401 3600 600 2592000 60
            )
@       IN  NS   ns1.nodns.shop.
```

### Bot Config (multi-zone)

The bot config (`/opt/nodns-bot/config.toml`) has two zones:

```toml
[[dns.zones]]
zone = "nodns.shop"
knot_address = "127.0.0.1:53"
tsig_key_name = "nodns-bot"
tsig_key_secret = "BASE64_SECRET"
tsig_algorithm = "hmac-sha256"
default_ttl = 3600

[[dns.zones]]
zone = "nostr"
knot_address = "127.0.0.1:53"
tsig_key_name = "nodns-bot"
tsig_key_secret = "BASE64_SECRET"
tsig_algorithm = "hmac-sha256"
default_ttl = 60
```

### dnsproxy Routing

The `[/nostr/]127.0.0.1:53` upstream in dnsproxy config ensures all `.nostr` queries hit our Knot DNS instead of going to public upstreams.

## Related Docs

- [04-demo-setup.md](04-demo-setup.md) — Setting up a new NoDNS zone from scratch
- [08-deployment-status.md](08-deployment-status.md) — Full production deployment details
- [11-protocol-experimental-draft.md](11-protocol-experimental-draft.md) — Protocol wire format
- [14-demo-recipes.md](14-demo-recipes.md) — Demo scripts with exact commands
