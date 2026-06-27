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

## KSK Rollover (DNSSEC Key Rotation)

When the zone's DNSSEC KSK (Key Signing Key) is rotated, the `dnskey_hash` in the
NIP-89 attestation event becomes stale. The bot checks for this at startup and logs
a warning if the live DNSKEY doesn't match the attested key.

### After KSK rollover:

1. **Verify the new KSK is live**:
   ```bash
   dig @46.224.104.12 nodns.shop DNSKEY +short
   ```

2. **Restart the bot** — it will detect the new key at startup:
   ```bash
   sudo systemctl restart nodns-bot
   sudo journalctl -u nodns-bot -n 20 | grep -i "dnskey\|attestation\|rollover"
   ```

3. **Confirm the bot re-attested** the new key. The startup log should show:
   - `DNSSEC KSK mismatch: live DNSKEY differs from derived key — attesting live key`
   - Or `attestation source: live (rollover detected)`

4. **Update the DS record at the registrar** (Namecheap) if the KSK algorithm or
   key changed. Use:
   ```bash
   sudo knotc keymgr nodns.shop ds
   ```

5. **Verify DNSSEC validation** on public resolvers:
   ```bash
   dig @8.8.8.8 nodns.shop DNSKEY +dnssec +multi | grep "ad"
   ```

### What the bot does automatically

- At startup, fetches the live DNSKEY from Knot DNS
- Compares against the SLIP-10 derived key
- If they differ (rollover happened), attests the live key and logs a warning
- If no KSK is found, falls back to the derived key with a warning
