# Deployment Status — nodns.shop

## What We Built

A production Knot DNS authoritative nameserver on `inr2.cashu.exchange` (46.224.104.12) serving the `nodns.shop` zone.

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

## What Was Done

1. Disabled `systemd-resolved` stub listener to free port 53
2. Set `/etc/resolv.conf` to use 8.8.8.8 / 1.1.1.1 for system DNS resolution
3. Installed Knot DNS 3.3.4 via apt
4. Created `/etc/knot/knot.conf` — listens on 0.0.0.0:53, serves nodns.shop zone, allows AXFR from puck.nether.net, sends NOTIFY to puck on zone changes
5. Created `/etc/knot/zones/nodns.shop.zone` — SOA, NS (ns1 + puck), glue A record, apex A record, TXT record
6. Registered `nodns.shop` as secondary at puck.nether.net — AXFR confirmed successful (253 bytes transferred at 2026-06-04T20:16:24Z)
7. Registered glue record `ns1.nodns.shop → 46.224.104.12` at Namecheap
8. Switched Namecheap nameservers to custom: `ns1.nodns.shop` + `puck.nether.net`

## Current Zone File

```zone
; nodns.shop - NoDNS zone
$TTL 3600
@       IN  SOA  ns1.nodns.shop. admin.nodns.shop. (
            2026060402  ; serial
            3600        ; refresh
            600         ; retry
            2592000     ; expire
            60          ; minimum / negative cache
            )

@       IN  NS   ns1.nodns.shop.
@       IN  NS   puck.nether.net.

ns1     IN  A    46.224.104.12

@       IN  A    46.224.104.12
@       IN  TXT  "NoDNS - DNS records from Nostr events"
```

## Verification Steps

### Step 1: Check Namecheap delegation propagated

```bash
# Check .shop registry directly — should show our NS records
dig @a.gmoregistry.net nodns.shop NS

# Expected:
# nodns.shop. IN NS ns1.nodns.shop.
# nodns.shop. IN NS puck.nether.net.

# Also check glue record at registry
dig @a.gmoregistry.net ns1.nodns.shop A
# Expected: ns1.nodns.shop. IN A 46.224.104.12
```

Can take up to 24 hours, but usually propagates within minutes to a few hours.

### Step 2: Check public resolvers

```bash
# Google DNS
dig @8.8.8.8 nodns.shop NS +short
# Expected: ns1.nodns.shop. / puck.nether.net.

dig @8.8.8.8 nodns.shop A +short
# Expected: 46.224.104.12

# Cloudflare DNS
dig @1.1.1.1 nodns.shop A +short
# Expected: 46.224.104.12
```

### Step 3: Check secondary (puck.nether.net)

```bash
dig @204.42.254.5 nodns.shop SOA +short
# Expected: ns1.nodns.shop. admin.nodns.shop. 2026060402 3600 600 2592000 60

dig @204.42.254.5 nodns.shop A +short
# Expected: 46.224.104.12
```

### Step 4: Full end-to-end test

```bash
# From any machine on the internet
dig nodns.shop A +short
# Expected: 46.224.104.12

dig nodns.shop TXT +short
# Expected: "NoDNS - DNS records from Nostr events"

dig ns1.nodns.shop A +short
# Expected: 46.224.104.12
```

### Step 5: Test zone update propagation

After the bot writes records via DDNS, verify they propagate:

```bash
# On the VPS — check Knot serves the new record
dig @127.0.0.1 npub1xxx.nodns.shop A

# Check puck picked it up (may take a few seconds after NOTIFY)
dig @204.42.254.5 npub1xxx.nodns.shop A

# Check via public resolvers
dig @8.8.8.8 npub1xxx.nodns.shop A
```

## Files on the VPS

| File | Purpose |
|---|---|
| `/etc/knot/knot.conf` | Knot DNS configuration |
| `/etc/knot/zones/nodns.shop.zone` | Zone file (bot will write here via DDNS, or we switch to DDNS-only) |
| `/etc/systemd/resolved.conf.d/no-stub.conf` | Disables systemd-resolved stub listener on port 53 |

## Next: Install nodns-bot

Once delegation is verified, the next step is to build and deploy the `nodns-bot` Go daemon that subscribes to Nostr relays and pushes DNS records to Knot via DDNS. See [08-implementation-plan.md](08-implementation-plan.md).
