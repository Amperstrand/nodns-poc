# Deployment Status — nodns.shop

## What We Built

A production Knot DNS authoritative nameserver on `46.224.104.12` serving the `nodns.shop` zone with DNSSEC.

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
| **Bot** | nodns-bot-rs (Rust) — systemd service `nodns-bot.service` |
| **DNSSEC** | ECDSAP256SHA256, NSEC3 `1 0 0 -`, `ad` flag confirmed on Google/Cloudflare/Quad9 |
| **Web** | Caddy serving `/var/www/nodns.shop` with `/api/*` proxied to Rust bot |
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
10. Configured Caddy to serve nodns.shop with `/api/*` proxy to bot
11. Enabled DNSSEC: KSK tag 12717, ZSK tag 33240, both ECDSAP256SHA256
12. Submitted DS record at Namecheap: `12717 13 2 b5a6a5f1...55758726`
13. **DNSSEC validated**: `ad` flag confirmed across all major resolvers (Google, Cloudflare, Quad9)

## Files on the VPS

| File | Purpose |
|---|---|
| `/etc/knot/knot.conf` | Knot DNS configuration (DNSSEC-enabled) |
| `/etc/knot/knot.conf.pre-dnssec` | Pre-DNSSEC backup |
| `/opt/nodns-bot/config-rs.toml` | Rust bot production config |
| `/opt/nodns-bot/nodns-bot-rs` | Rust bot binary |
| `/var/www/nodns.shop/index.html` | Frontend (monolithic HTML) |
| `/etc/caddy/Caddyfile` | Caddy reverse proxy config |
