# NoDNS .cv Integration — Architecture Proposal

## TL;DR

ARME runs a `nodns-bot` binary on their infrastructure. The bot subscribes to Nostr relays, validates kind 11111 events, and pushes DNS records to their authoritative nameservers via standard DDNS (RFC 2136). Zero software changes. They keep full control.

## Problem

Users publish DNS records as Nostr events (kind 11111). These records need to appear under `.cv` — e.g., `npub1ykal2...pa3dl.cv` resolves to an IP address. The `.cv` registry needs to serve these records without changing their DNS infrastructure.

## Architecture

```
  Nostr Relays                    nodns-bot (runs on ARME infra)       .cv Nameservers
  ┌──────────────┐               ┌──────────────────────┐           ┌──────────────┐
  │ relay.damus  │──WebSocket───▶│                      │──DDNS────▶│              │
  │ nos.lol      │──────────────▶│  validate events     │  UPDATE   │  ns.dns.cv   │─── Internet
  │ relay.nostr  │──────────────▶│  parse records       │  (TSIG)   │  cv01.dns.pt │
  │ .band        │──────────────▶│  push to nameservers │           │  c.dns.pt    │
  └──────────────┘               │  persist state        │           │  dnsnode.net │
                                 └──────────────────────┘           └──────────────┘
```

## What ARME Does

1. **Generate a TSIG key** on their nameserver:
   ```
   tsig-keygen -a hmac-sha256 nodns-bot
   ```

2. **Configure an ACL** allowing the bot to update only `npub1*.cv` records:
   ```
   // Knot DNS example:
   acl "nodns-bot" {
     key "nodns-bot";
     action update;
     // Optional: restrict to npub1* labels only via update-policy
   };
   ```

3. **Deploy the bot binary** on a VM/bare metal inside their network:
   ```
   # Static binary, no dependencies
   curl -L https://releases.nodns.shop/nodns-bot-linux-amd64 -o /usr/local/bin/nodns-bot
   chmod +x /usr/local/bin/nodns-bot
   ```

4. **Write a config file** (they control everything):
   ```toml
   [server]
   bind = "127.0.0.1:9090"

   [nostr]
   relays = [
     "wss://relay.damus.io",
     "wss://nos.lol",
     "wss://relay.nostr.band",
   ]
   zone = "cv"

   [dns]
   # Points to THEIR nameserver, on THEIR network
   knot_address = "127.0.0.1:53"
   tsig_key_name = "nodns-bot."
   tsig_key_secret = "<key-they-generated>"
   tsig_algorithm = "hmac-sha256"

   [policy]
   # ARME controls policy limits
   max_records = 20
   rate_limit = 5
   allowed_types = ["A", "AAAA", "CNAME", "TXT", "MX"]
   block_private_ip = true

   [store]
   path = "/var/lib/nodns-bot/records.db"
   ```

5. **Start the service**:
   ```
   systemctl enable nodns-bot
   systemctl start nodns-bot
   ```

## What ARME Controls

| Aspect | Who Controls |
|--------|-------------|
| TSIG key generation | ARME |
| ACL / update policy | ARME |
| Which relays to subscribe to | ARME (in config) |
| Rate limits, max records | ARME (in config) |
| Allowed record types | ARME (in config) |
| Private IP blocking | ARME (in config) |
| Network access / firewall | ARME |
| Bot binary updates | ARME (or automated via systemd) |
| DNSSEC signing | Automatic (nameserver handles it) |

## Security Guarantees

1. **Cryptographic proof**: Every record is signed by the publisher's Nostr keypair. The bot verifies signatures before pushing.
2. **No trusted user input**: All data validated — record types whitelisted, IPs checked against private ranges, TXT length capped, rate-limited per npub.
3. **TSIG authentication**: Bot authenticates to nameserver with shared secret. No anonymous updates.
4. **Network isolation**: Bot runs inside ARME's network. No inbound ports required (outbound WebSocket to relays + localhost DNS).
5. **Audit trail**: SQLite database logs every processed event with event ID, npub, records, timestamp.
6. **Policy enforcement**: Rate limits, record counts, type restrictions — all configurable by ARME.
7. **Only npub records**: The bot only processes kind 11111 events. No other event types affect DNS.

## What Changes in DNS Resolution

```
Before:  dig npub1ykal2...pa3dl.cv A  →  NXDOMAIN (not registered)
After:   dig npub1ykal2...pa3dl.cv A  →  193.99.144.80 (from Nostr event)
```

Regular `.cv` domains are unaffected. The bot only creates records under `npub1*.cv` subdomain labels.

## Live Demo

We have this running right now on `nodns.shop`:

```bash
# 10 records live, resolving globally
dig npub190queyng2pmx0jfw5rkx4fjjl3u0zxz6nlyaja53p2n0ydupr6jsdnqt8q.nodns.shop A
# → 185.18.221.10

# Web dashboard: https://nodns.shop
# Generate keys, publish records, see them resolve in seconds
```

## FQDN Construction

For `.cv`, records resolve as:
```
npub1{public-key}.cv              → A, AAAA, TXT, MX records
www.npub1{public-key}.cv          → CNAME or other subdomain records
blog.npub1{public-key}.cv         → any subdomain the user specifies
```

The npub (bech32-encoded public key, 59 chars) becomes the subdomain label.

## Why This Matters for Cape Verde

- **Free domains for CV citizens**: Anyone with a Nostr keypair gets a `.cv` domain instantly — no registration, no fee
- **No infrastructure cost**: Bot uses ~10MB RAM, negligible CPU. Runs alongside existing DNS.
- **No support burden**: Self-service via Nostr events. Users manage their own records.
- **Cryptographic ownership**: No account hijacking — you need the private key to publish records
- **Anti-censorship**: Records can't be seized without the private key
- **Standards-based**: DDNS (RFC 2136), TSIG, DNSSEC — all standard DNS protocols
