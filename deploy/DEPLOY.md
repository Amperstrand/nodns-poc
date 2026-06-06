# VPS Deployment Instructions — NoDNS Multi-Zone Setup

## Prerequisites

- VPS: `46.224.104.12` (Ubuntu 24.04)
- Knot DNS 3.3.4 running on port 5353
- Caddy 2.11.4 serving nodns.shop
- Existing nodns-bot service running

## Step 1: Add zone files to Knot

```bash
# Copy zone files
sudo cp deploy/zones/nostr.shop.zone /etc/knot/zones/
sudo cp deploy/zones/cv.zone /etc/knot/zones/

# Fix ownership
sudo chown knot:knot /etc/knot/zones/nostr.shop.zone
sudo chown knot:knot /etc/knot/zones/cv.zone

# Verify existing zone is still owned correctly
ls -la /etc/knot/zones/
```

## Step 2: Update Knot configuration

Edit `/etc/knot/knot.conf` and add the new zones:

```bash
sudo nano /etc/knot/knot.conf
```

Add the nostr.shop and cv zone blocks (see `deploy/knot-zones.conf` for the snippet).

Then reload:

```bash
sudo knotc reload
sudo knotc zone-check nostr.shop
sudo knotc zone-check cv
```

Verify the zones are loaded:

```bash
sudo knotc zone-status
```

## Step 3: Update bot configuration

```bash
# Copy multi-zone config
sudo cp deploy/config-multi-zone.toml /opt/nodns-bot/config.toml

# EDIT: Replace placeholder values with real secrets
sudo nano /opt/nodns-bot/config.toml
# Replace:
#   - REPLACE_WITH_REAL_SECRET (TSIG key)
#   - REPLACE_WITH_REAL_REGISTRAR_PUBKEY_HEX (registrar keys)
```

## Step 4: Build and deploy new bot binary

```bash
# On local machine:
cd /Users/macbook/src/nodns/nodns-bot
GOOS=linux GOARCH=amd64 go build -o nodns-bot-linux-amd64 .

# Copy to VPS:
scp nodns-bot-linux-amd64 root@46.224.104.12:/opt/nodns-bot/nodns-bot

# On VPS:
sudo systemctl restart nodns-bot
sudo systemctl status nodns-bot
```

## Step 5: Test resolution

```bash
# Test nostr.shop zone (after DNS delegation is set up)
dig @46.224.104.12 nostr.shop SOA

# Test cv demo zone
dig @46.224.104.12 cv SOA

# Test npub resolution across zones
dig @46.224.104.12 npub1ykal28...pa3dl.nodns.shop A
dig @46.224.104.12 npub1ykal28...pa3dl.cv A
```

## Step 6: Update Caddy (optional — for nostr.shop web)

Add nostr.shop to Caddyfile:

```
nostr.shop {
    reverse_proxy /api/* 127.0.0.1:9090
    root * /var/www/nostr.shop
    file_server
}
```

## DNS Delegation for nostr.shop

To make nostr.shop resolve globally, you need to delegate NS records at the .shop registry:

```
nostr.shop NS ns1.nodns.shop
nostr.shop NS puck.nether.net
```

This is done at your domain registrar's control panel (wherever you registered nostr.shop).

## DNS Delegation for .cv (demo only)

For the demo, `dig @46.224.104.12` (direct query) works without delegation.
Real .cv integration requires ARME to run the bot on their infrastructure.

## Verify Everything

```bash
# Bot health
curl http://127.0.0.1:9090/health

# Bot records API
curl http://127.0.0.1:9090/api/records

# Knot zone status
sudo knotc zone-status

# Test DDNS update to new zone
echo "Test update to nostr.shop zone" && \
  dig @46.224.104.12 nostr.shop SOA +short
```

## Troubleshooting

- **Zone not loading**: Check `sudo journalctl -u knot -n 50`
- **DDNS refused**: Check zone file ownership (`knot:knot`), TSIG key, ACL
- **Bot crash**: Check `sudo journalctl -u nodns-bot -n 50`
- **SERVFAIL**: Flush journal with `sudo knotc zone-flush nodns.shop`, check zone file ownership
