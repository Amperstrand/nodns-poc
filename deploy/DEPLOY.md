# VPS Deployment Instructions — NoDNS (nodns.shop)

## Prerequisites

- VPS: `46.224.104.12` (Ubuntu 24.04)
- Knot DNS 3.3.4 running on port 53 (public authoritative listener on `0.0.0.0:53`; bot sends DDNS to `127.0.0.1:53`)
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
- **Open DNS resolver abuse report (BSI/Hetzner)**: See "DNS hardening" below

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

## DNS hardening (open resolver incident — 2026-07-09)

### Background

A BSI abuse notification (forwarded by Hetzner, CB-Report timestamp
`2026-07-07 03:15:18 UTC`) flagged `46.224.104.12` as an open DNS resolver.
Investigation confirmed the server was a **genuine open recursive resolver**:
`dig @46.224.104.12 google.com A` returned live recursive answers with the
`ra` (Recursion Available) flag set, and the NSID leaked an upstream Frankfurt
resolver (`fra19`).

### Root cause

`/etc/knot/knot.conf` loaded Knot's `mod-dnsproxy` module as a `global-module`
on the `default` template. This forwarded every non-authoritative query to
Cloudflare `1.1.1.1`, turning the public authoritative listener into a fully
open recursive resolver for the internet. The module was intended for the `.cv`
pilot (`cv-forward` / `cv-domain`) but was mistakenly attached to `default`
instead of the `cv-domain` template, so it applied to all zones.

### Fix applied

Removed the line `global-module: mod-dnsproxy/cv-forward` from the `default`
template in `/etc/knot/knot.conf` and reloaded Knot (`knotc reload`). The
`mod-dnsproxy` block definition was left in place (harmless when unreferenced)
in case the `.cv` pilot later scopes it to `cv-domain` correctly.

Verification (all from an external host):
- `dig @46.224.104.12 google.com A` → `status: REFUSED`, no `ra` flag, NSID
  `inr.cashu.dev` (the local Knot, no longer the upstream). ✅ closed
- `dig @46.224.104.12 nodns.shop SOA` → `aa` NOERROR. ✅ auth intact
- `dig @46.224.104.12 nodns.shop DNSKEY` → DNSSEC still signing. ✅
- `dig @8.8.8.8 nodns.shop SOA +short` → globally resolvable. ✅

Backup of the pre-fix config: `/etc/knot/knot.conf.bak.20260708T224533Z` on
the VPS.

### Do NOT re-introduce

Never attach `mod-dnsproxy` (or any forwarding module) to the `default`
template on a public-facing authoritative server. If forwarding is required for
a specific pilot, scope it to that pilot's dedicated template only. See the
warning header in `deploy/knot-zones.conf`.

### Additional hardening applied (2026-07-09)

Beyond closing the open resolver, the following defense-in-depth measures were
applied to `/etc/knot/knot.conf` and verified on Knot DNS 3.3.4:

- **Response Rate Limiting (RRL)** via the `mod-rrl` module (NOT a `server.`
  key — `server.response-rate-limiting` is invalid in Knot 3.3.4):
  ```
  mod-rrl:
      - id: rrl
        rate-limit: 200
        slip: 2
  template:
      - id: default
        global-module: mod-rrl/rrl
  ```
  Caps amplification abuse even for authoritative answers (DNSSEC responses
  are large). `slip: 2` leaks every 2nd throttled response as TC=1 so legit
  clients retry over TCP.
- **Version/NSID disclosure suppressed**: `server.nsid: ""` blanks the NSID
  (was advertising `inr.cashu.dev`); `server.version: ""` hides the version
  (`version.bind CH TXT` was returning `Knot DNS 3.3.4`, now empty).

Verification post-hardening (external): `version.bind CH TXT` returns empty,
NSID no longer appears in responses, `nodns.shop` SOA/DNSKEY still resolve
with `aa`, `google.com` still REFUSED with no `ra` flag.
