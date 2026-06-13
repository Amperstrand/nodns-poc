# VPS Deployment Instructions — NoDNS (nodns.shop)

## Prerequisites

- VPS: `46.224.104.12` (Ubuntu 24.04)
- Knot DNS 3.3.4 running on port 5353
- Caddy 2.11.4 serving nodns.shop
- Existing nodns-bot service running

## Step 1: Build the binary (local machine)

```bash
cd /Users/macbook/src/nodns/nodns-bot-rs
cargo zigbuild --release --target x86_64-unknown-linux-gnu
```

## Step 2: Deploy binary + config to VPS

```bash
# Copy binary
scp target/x86_64-unknown-linux-gnu/release/nodns-bot root@46.224.104.12:/opt/nodns-bot/nodns-bot

# Copy config (EDIT secrets first!)
scp deploy/config-multi-zone.toml root@46.224.104.12:/opt/nodns-bot/config.toml
```

Before deploying config, edit `config-multi-zone.toml`:
- Replace `REPLACE_WITH_REAL_SECRET` with the actual TSIG key
- Replace `REPLACE_WITH_REAL_REGISTRAR_PUBKEY_HEX` with the registrar's pubkey hex

The `operator_lease_expires = "2027-06-04"` is already set (from RDAP).

## Step 3: Restart the bot

```bash
ssh root@46.224.104.12
sudo systemctl restart nodns-bot
sudo systemctl status nodns-bot
```

## Step 4: Verify

```bash
# Bot health
curl http://127.0.0.1:9090/health

# Bot records API
curl http://127.0.0.1:9090/api/records

# DNS resolution
dig @46.224.104.12 nodns.shop SOA
dig @46.224.104.12 npub1ykal28...pa3dl.nodns.shop A
```

## Troubleshooting

- **Zone not loading**: Check `sudo journalctl -u knot -n 50`
- **DDNS refused**: Check zone file ownership (`knot:knot`), TSIG key, ACL
- **Bot crash**: Check `sudo journalctl -u nodns-bot -n 50`
- **SERVFAIL**: Flush journal with `sudo knotc zone-flush nodns.shop`, check zone file ownership
