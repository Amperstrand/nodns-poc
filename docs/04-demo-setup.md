# 04 — Demo Setup Guide

> **Status**: ACTIVE. VPS setup guide for a NoDNS zone using the Rust bot (`nodns-bot-rs/`).

This guide covers setting up a NoDNS zone on a VPS. After setup, any Nostr user will be able to publish a kind 11111 event and have their domain resolve globally.

## Prerequisites

| Requirement | Spec |
|---|---|
| VPS | 1-2 vCPU, 1-2 GB RAM, 10-20 GB disk |
| OS | Ubuntu 22.04 or Debian 12 |
| Network | Public IPv4, ports 53/UDP+TCP and 443/TCP open |
| Domain | A domain for your NS names (e.g., `your-server.com`) |
| Software | Rust 1.75+, Knot DNS 3.x |

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
    - domain: your-zone.example
      file: "/etc/knot/zones/your-zone.example.zone"
      update: bot-update
      dnssec-signing: on
      dnssec-policy: default
      # Uncomment when secondaries are available:
      # notify: [ SECONDARY_IP_1, SECONDARY_IP_2 ]
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

Create `/etc/knot/zones/your-zone.example.zone`:

```zone
; your-zone.example — NoDNS zone
; Bot adds records via DDNS. This file only needs SOA + NS.
$TTL 3600
@       IN  SOA  ns1.your-server.com. admin.your-zone.example. (
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
@       IN  TXT  "NoDNS protocol demo"
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
sudo knotc zone-status your-zone.example
# Expected: "your-zone.example: loaded"

# Test DNS locally
dig @127.0.0.1 your-zone.example SOA
dig @127.0.0.1 your-zone.example NS
```

## Step 6: Build and Configure nodns-bot

```bash
# Clone repository
cd /opt
git clone https://github.com/Amperstrand/nodns-poc.git

# Build the Rust bot
cd nodns-poc/nodns-bot-rs
cargo build --release

# Install binary
sudo cp target/release/nodns-bot /opt/nodns-bot/
sudo chmod +x /opt/nodns-bot/nodns-bot

# Create config directory
sudo mkdir -p /opt/nodns-bot
```

Create `/opt/nodns-bot/config.toml`:

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
zone = "your-zone.example"

[[dns.zones]]
knot_address = "127.0.0.1:53"
zone = "your-zone.example"
tsig_key_name = "nodns-bot"
tsig_key_secret = "BASE64_SECRET"
tsig_algorithm = "hmac-sha256"
default_ttl = 3600
negative_ttl = 60

[dns]
# Optional: Use this for backward compatibility with single-zone configs.
# If [[dns.zones]] is empty, these fields auto-populate a single zone entry.
# knot_address = "127.0.0.1:53"
# zone = "your-zone.example"
# tsig_key_name = "nodns-bot"
# tsig_key_secret = "BASE64_SECRET"
# tsig_algorithm = "hmac-sha256"
# default_ttl = 3600
# negative_ttl = 60

[policy]
max_records = 20
rate_limit = 5
allowed_types = ["A", "AAAA", "CNAME", "TXT", "MX"]
block_private_ip = true
max_txt_length = 512

[store]
path = "/opt/nodns-bot/records.db"

# Payment configuration (optional)
[payment]
enabled = false
required_sats = 250
update_free = true
cashu_mint_url = "https://testnut.cashu.space"

# Optional: Registrar identity for DNSSEC derivation
[registrar]
nsec_hex = ""

[dnssec_derivation]
enabled = false

# Optional: ACME certificate provisioning
[acme]
enabled = false
environment = "staging"
directory_url = ""
contact_email = ""
challenge_ttl = 300
ca = "letsencrypt-staging"
zerossl_eab_kid = ""
zerossl_eab_hmac_key = ""
encryption_key = ""
```

**Multi-zone support**: The bot supports multiple zones via the `[[dns.zones]]` array. Add additional `[[dns.zones]]` blocks for each zone you want to serve. Each zone can have its own payment and lease configuration.

**Record tag format**: The bot accepts two formats for record tags:

- 11-element format: `["record", TYPE, NAME, RDATA, "", "", "", "", "", "", "3600"]`
- 5-element format: `["record", TYPE, NAME, "3600", RDATA]`

For example:
- A record: `["record", "A", "", "203.0.113.42", "", "", "", "", "", "", "3600"]`
- TXT record: `["record", "TXT", "", "hello world", "", "", "", "", "", "", "3600"]`

## Step 7: Create Systemd Service

Create `/etc/systemd/system/nodns-bot.service`:

```ini
[Unit]
Description=NoDNS Bot
After=network.target knot.service
Requires=knot.service

[Service]
Type=simple
User=nodns
Group=nodns
ExecStart=/opt/nodns-bot/nodns-bot --config /opt/nodns-bot/config.toml
Restart=on-failure
RestartSec=5

# Security hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/opt/nodns-bot
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

```bash
# Create user
sudo useradd -r -s /bin/false nodns

# Set permissions
sudo chown -R nodns:nodns /opt/nodns-bot

# Enable and start
sudo systemctl daemon-reload
sudo systemctl enable nodns-bot
sudo systemctl start nodns-bot

# Check logs
sudo journalctl -u nodns-bot -f
```

## Step 8: Test Locally

Before setting up delegation, verify the full pipeline works:

### 8a. Publish a test event

Using any Nostr client that supports kind 11111:

```json
{
  "kind": 11111,
  "tags": [
    ["record", "A", "", "203.0.113.42", "", "", "", "", "", "", "3600"]
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
dig @127.0.0.1 npub1YOURNPUB.your-zone.example A
# Expected: 203.0.113.42

# With DNSSEC
dig @127.0.0.1 npub1YOURNPUB.your-zone.example A +dnssec
# Expected: A record + RRSIG
```

### 8d. Verify DNSSEC

```bash
# Check DNSKEY
dig @127.0.0.1 your-zone.example DNSKEY

# Check NSEC3
dig @127.0.0.1 your-zone.example NSEC3PARAM

# Validate chain (requires delv tool)
delv @127.0.0.1 npub1YOURNPUB.your-zone.example A
# Expected: "; fully validated"
```

## Step 9: Set Up Delegation

Configure your domain registrar to delegate your zone to your nameserver:

1. Create glue records at your registrar: `ns1.your-server.com → YOUR_VPS_IP`
2. Set nameservers for `your-zone.example` to `ns1.your-server.com` and `ns2.your-server.com`
3. Wait for delegation to propagate (usually minutes to hours)

See [12-dnssec-setup.md](12-dnssec-setup.md) for DNSSEC configuration and DS record submission.

## Step 10: Verify Global Resolution

After delegation is in place:

```bash
# Check delegation from parent zone
dig your-zone.example NS
# Should show ns1.your-server.com

# Check from public resolvers
dig @8.8.8.8 npub1YOURNPUB.your-zone.example A
dig @1.1.1.1 npub1YOURNPUB.your-zone.example A

# Check DNSSEC validation (will only work once DS is in parent zone)
dig @8.8.8.8 npub1YOURNPUB.your-zone.example A +dnssec
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
- The bot subscribes to ALL kind 11111 events and matches zones in the event processor. No tag filter needed.

### DDNS updates failing
- Check TSIG key matches between knot.conf and config.toml
- Check bot is connecting to 127.0.0.1 (not public IP)
- Check Knot logs: `sudo journalctl -u knot`

### Records not resolving externally
- Check firewall allows port 53
- Check delegation is in place: `dig your-zone.example NS` from external resolver
- Check Knot is listening on public IP: `sudo netstat -ulnp | grep :53`

### DNSSEC validation failures
- Expected: Zone DNSSEC won't validate globally until DS record is in parent zone
- Local validation works: Knot signs the zone, just no chain of trust to root yet
- This is not a problem for testing — records still resolve, just without cryptographic chain to root

## Additional Resources

- [08-deployment-status.md](08-deployment-status.md): Production deployment details for nodns.shop
- [12-dnssec-setup.md](12-dnssec-setup.md): Production DNSSEC deployment reference
- [11-protocol-experimental-draft.md](11-protocol-experimental-draft.md): Protocol specification with record/delegation/payment tag formats