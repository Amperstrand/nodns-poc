# Demo Setup Guide — nostr.cv

This guide covers setting up the `nostr.cv` NoDNS demo on a VPS. After setup, any Nostr user will be able to publish a kind 11111 event and have their domain resolve globally.

## Prerequisites

| Requirement | Spec |
|---|---|
| VPS | 1-2 vCPU, 1-2 GB RAM, 10-20 GB disk |
| OS | Ubuntu 22.04 or Debian 12 |
| Network | Public IPv4, ports 53/UDP+TCP and 443/TCP open |
| Domain | A domain for your NS names (e.g., `your-server.com`) |
| Software | Go 1.21+, Knot DNS 3.x |

## Step 1: Install Knot DNS

```bash
# Ubuntu 22.04 / Debian 12
sudo apt update
sudo apt install knot

# Verify
knotd --version
# Expected: Knot DNS 3.x
```

## Step 2: Generate TSIG Key

```bash
# Generate a TSIG key for bot ↔ Knot authentication
keymgr -t nodns-bot hmac-sha256
# Output: nodns-bot:hmac-sha256:BASE64_SECRET
```

Save the `BASE64_SECRET` — you'll need it in both Knot config and bot config.

## Step 3: Configure Knot DNS

Create `/etc/knot/knot.conf`:

```knot
server:
    listen: [ 0.0.0.0@53, ::@53 ]
    udp-workers: 2
    tcp-workers: 2
    background-workers: 2

key:
    - id: nodns-bot
      algorithm: hmac-sha256
      secret: BASE64_SECRET

acl:
    - id: bot-update
      key: nodns-bot
      action: [ update, transfer ]
      remote: 127.0.0.1

zone:
    - domain: nostr.cv
      file: "/etc/knot/zones/nostr.cv.zone"
      update: bot-update
      dnssec-signing: on
      dnssec-policy: default
      # Uncomment when secondaries are available:
      # notify: [ 193.137.12.78, 193.137.12.79 ]
      # acl: [ secondary-transfer ]

log:
    - target: syslog
      any: info
```

Create the zone directory:

```bash
sudo mkdir -p /etc/knot/zones
```

## Step 4: Create Initial Zone File

Create `/etc/knot/zones/nostr.cv.zone`:

```zone
; nostr.cv — NoDNS demo zone
; Bot adds records via DDNS. This file only needs SOA + NS.
$TTL 3600
@       IN  SOA  ns1.your-server.com. admin.nodns.cv. (
            2026060401  ; serial (YYYYMMDDNN)
            3600        ; refresh
            600         ; retry
            2592000     ; expire
            60          ; minimum / negative cache (60s for fast propagation)
            )

@       IN  NS   ns1.your-server.com.
@       IN  NS   ns2.your-server.com.

; Zone apex — landing page
@       IN  A    YOUR_VPS_IP
@       IN  TXT  "NoDNS protocol demo — .cv ccTLD Nostr integration"
```

**Note on negative cache TTL**: Set to 60 seconds (the `minimum` field). This means if a domain doesn't exist yet, resolvers will only cache the NXDOMAIN for 60 seconds. When a user publishes a Nostr event and the bot creates the record, it becomes resolvable within 60 seconds max. Without this, the default would be 3600 seconds (1 hour) of NXDOMAIN caching.

## Step 5: Start Knot

```bash
# Check config
sudo knotc conf-check

# Start Knot
sudo systemctl enable knot
sudo systemctl start knot

# Verify zone is loaded
sudo knotc zone-status nostr.cv
# Expected: "nostr.cv: loaded"

# Test DNS locally
dig @127.0.0.1 nostr.cv SOA
dig @127.0.0.1 nostr.cv NS
```

## Step 6: Build and Configure nodns-bot

```bash
# Clone and build
cd /opt
git clone https://github.com/YOUR_ORG/nodns-bot.git
cd nodns-bot
go build -o nodns-bot .

# Create data directory
sudo mkdir -p /var/lib/nodns-bot
sudo chown nodns:nodns /var/lib/nodns-bot
```

Create `/etc/nodns-bot/config.toml`:

```toml
[server]
bind = "127.0.0.1:9090"

[nostr]
relays = [
    "wss://relay.damus.io",
    "wss://nos.lol",
    "wss://relay.nostr.band",
    "wss://nostr.wine",
]
zone = "nostr.cv"
reconnect_min = "1s"
reconnect_max = "60s"

[dns]
knot_address = "127.0.0.1:53"
zone = "nostr.cv"
tsig_key_name = "nodns-bot"
tsig_key_secret = "BASE64_SECRET"    # Same as knot.conf
tsig_algorithm = "hmac-sha256"
default_ttl = 3600
negative_ttl = 60

[policy]
max_records = 20
rate_limit = 5
allowed_types = ["A", "AAAA", "CNAME", "TXT", "MX"]
block_private_ips = true
max_txt_length = 512

[store]
path = "/var/lib/nodns-bot/records.db"
```

## Step 7: Create Systemd Service

Create `/etc/systemd/system/nodns-bot.service`:

```ini
[Unit]
Description=NoDNS Bot for nostr.cv
After=network.target knot.service
Requires=knot.service

[Service]
Type=simple
User=nodns
Group=nodns
ExecStart=/opt/nodns-bot/nodns-bot -config /etc/nodns-bot/config.toml
Restart=on-failure
RestartSec=5

# Security hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/nodns-bot
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable nodns-bot
sudo systemctl start nodns-bot

# Check logs
sudo journalctl -u nodns-bot -f
```

## Step 8: Test Locally

Before asking ARME to delegate, verify the full pipeline works:

### 8a. Publish a test event

Using any Nostr client that supports kind 11111:

```json
{
  "kind": 11111,
  "tags": [
    ["record", "", "A", "IN", "3600", "203.0.113.42", "", "", "", "", "nostr.cv"]
  ],
  "content": ""
}
```

### 8b. Verify bot received it

```bash
curl http://127.0.0.1:9090/health
# Check events_processed > 0
```

### 8c. Verify DNS resolution

```bash
dig @127.0.0.1 npub1YOURNPUB.nostr.cv A
# Expected: 203.0.113.42

# With DNSSEC
dig @127.0.0.1 npub1YOURNPUB.nostr.cv A +dnssec
# Expected: A record + RRSIG
```

### 8d. Verify DNSSEC

```bash
# Check DNSKEY
dig @127.0.0.1 nostr.cv DNSKEY

# Check NSEC3
dig @127.0.0.1 nostr.cv NSEC3PARAM

# Validate chain (requires delv tool)
delv @127.0.0.1 npub1YOURNPUB.nostr.cv A
# Expected: "; fully validated"
```

## Step 9: Ask ARME to Delegate

Once everything works locally, send ARME the delegation instructions from [05-arme-delegation.md](05-arme-delegation.md).

## Step 10: Verify Global Resolution

After ARME adds the NS records:

```bash
# Check delegation from .cv
dig cv NS
# Should show ns1.your-server.com for nostr.cv

# Check from public resolvers
dig @8.8.8.8 npub1YOURNPUB.nostr.cv A
dig @1.1.1.1 npub1YOURNPUB.nostr.cv A

# Check DNSSEC validation (will only work once .cv has DS in root zone)
dig @8.8.8.8 npub1YOURNPUB.nostr.cv A +dnssec
```

## Firewall Rules

```bash
# Allow DNS
sudo ufw allow 53/udp
sudo ufw allow 53/tcp

# Allow HTTPS (for landing page / health check if exposed)
sudo ufw allow 443/tcp

# Block everything else
sudo ufw default deny incoming
sudo ufw default allow outgoing
sudo ufw enable
```

## Troubleshooting

### Bot not receiving events
- Check relay connectivity: `curl -i -N -H "Connection: Upgrade" -H "Upgrade: websocket" -H "Sec-WebSocket-Version: 13" -H "Sec-WebSocket-Key: test" wss://relay.damus.io`
- Check subscription filter: bot should send `["REQ", "sub-id", {"kinds": [11111], "#x": ["nostr.cv"]}]`

### DDNS updates failing
- Check TSIG key matches between knot.conf and config.toml
- Check bot is connecting to 127.0.0.1 (not public IP)
- Check Knot logs: `sudo journalctl -u knot`

### Records not resolving externally
- Check firewall allows port 53
- Check delegation is in place: `dig cv NS` or `dig nostr.cv NS` from external resolver
- Check Knot is listening on public IP: `sudo netstat -ulnp | grep :53`

### DNSSEC validation failures
- Expected: `nostr.cv` DNSSEC won't validate globally until `.cv` DS is in root zone
- Local validation works: Knot signs the zone, just no chain of trust to root yet
- This is not a problem for the demo — records still resolve, just without cryptographic chain to root
