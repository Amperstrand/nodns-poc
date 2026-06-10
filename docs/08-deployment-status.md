# 08 — Deployment Status

> **Status**: ACTIVE. Production deployment details for nodns.shop.

## What We Built

A production Knot DNS authoritative nameserver on `46.224.104.12` serving the `nodns.shop` zone with DNSSEC, a Rust bot for Nostr event processing, a Next.js frontend, and ACME HTTPS certificate provisioning.

## Server Configuration

| Component | Details |
|---|---|
| **VPS** | inr2.cashu.exchange (Hetzner) |
| **OS** | Ubuntu 24.04.3 LTS |
| **IP** | 46.224.104.12 |
| **Software** | Knot DNS 3.3.4 |
| **Zone** | nodns.shop |
| **Primary NS** | ns1.nodns.shop → 46.224.104.12 |
| **Secondary NS** | puck.nether.net → 204.42.254.5 (Michigan, USA) |
| **Bot** | nodns-bot (Rust) — systemd service `nodns-bot.service` |
| **Frontend** | Next.js static export served by Caddy |
| **DNSSEC** | ECDSAP256SHA256, NSEC3 `1 0 0 -`, `ad` flag confirmed on Google/Cloudflare/Quad9 |
| **TLS** | Caddy (automatic HTTPS for nodns.shop) |
| **ACME** | Client-side P-256 key derivation, CSR generation, bot provisions certs via Let's Encrypt (staging by default, production per-request) |
| **Registrar** | Namecheap (manual DS only — no RFC 8078 support) |

## What Was Done

1. Disabled `systemd-resolved` stub listener to free port 53
2. Set `/etc/resolv.conf` to use 8.8.8.8 / 1.1.1.1 for system DNS resolution
3. Installed Knot DNS 3.3.4 via apt
4. Created `/etc/knot/knot.conf` — listens on 0.0.0.0:53, serves nodns.shop zone, allows AXFR from puck.nether.net, sends NOTIFY to puck on zone changes
5. Registered `nodns.shop` as secondary at puck.nether.net — AXFR confirmed
6. Registered glue record `ns1.nodns.shop → 46.224.104.12` at Namecheap
7. Switched Namecheap nameservers to custom: `ns1.nodns.shop` + `puck.nether.net`
8. Deployed Rust bot (`nodns-bot-rs`) as systemd service at `127.0.0.1:9090`
9. Archived Go bot in `nodns-bot-archive/`
10. Configured Caddy to serve nodns.shop with `/api/*` and `/.well-known/*` proxy to bot
11. Enabled DNSSEC: KSK tag 12717, ZSK tag 33240, both ECDSAP256SHA256
12. Submitted DS record at Namecheap: `12717 13 2 b5a6a5f1...55758726`
13. **DNSSEC validated**: `ad` flag confirmed across all major resolvers (Google, Cloudflare, Quad9)
14. Deployed Next.js frontend (static export to `/var/www/nodns.shop/`)
15. Added ACME certificate provisioning: client-side TLS key derivation from nsec, P-256 CSR generation, bot handles Let's Encrypt challenge/response
16. Added DNS record delete support: `["delete", TYPE, NAME]` tag in kind 11111 events
17. Added staging/production toggle for ACME certificate requests

## Files on the VPS

| File | Purpose |
|---|---|
| `/etc/knot/knot.conf` | Knot DNS configuration (DNSSEC-enabled) |
| `/etc/knot/knot.conf.pre-dnssec` | Pre-DNSSEC backup |
| `/opt/nodns-bot/config.toml` | Rust bot production config |
| `/opt/nodns-bot/nodns-bot` | Rust bot binary |
| `/opt/nodns-bot/records.db` | SQLite database (events, delegations, acme_orders, acme_order_logs) |
| `/opt/nodns-bot-src/` | Bot source tree (for on-VPS builds) |
| `/var/www/nodns.shop/` | Next.js static export (frontend) |
| `/etc/caddy/Caddyfile` | Caddy reverse proxy config |
| `/etc/systemd/system/nodns-bot.service` | Bot systemd service |

## Bot API Routes

| Route | Method | Purpose |
|---|---|---|
| `/health` | GET | Health check (JSON status) |
| `/api/records` | GET | List all DNS records |
| `/api/acme/order` | POST | Submit ACME certificate request |
| `/api/acme/order/{id}` | GET | Check ACME order status + logs |
| `/.well-known/nostr.json` | GET | NIP-05 Nostr user lookup |

## Protocol Operations

| Operation | Event Tag | Fee |
|---|---|---|
| Create DNS record | `["record", TYPE, NAME, RDATA, ...]` | 250 sats |
| Update DNS record | Same record tag (overwrites) | Free |
| Delete DNS record | `["delete", TYPE, NAME]` | Free |
| Domain delegation | `["delegation", DOMAIN, NPUB, ...]` | Registrar fee |
| ACME certificate | Via HTTP API (not Nostr event) | Free (Let's Encrypt) |
