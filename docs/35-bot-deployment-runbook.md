# 35 — Bot Deployment Runbook

> **Status**: ACTIVE. Step-by-step guide for deploying the Rust bot to the VPS.

## Prerequisites

- Rust 1.95+ with `cargo-zigbuild` and `zig` installed for cross-compilation
- SSH access to `root@46.224.104.12` (nodns.shop VPS)
- The VPS must already have Knot DNS, Caddy, and the systemd service configured

## Quick Deploy

```bash
./deploy/deploy.sh --push
```

This runs all four steps automatically: expiry check, tests, cross-compile, upload + restart.

## Manual Step-by-Step

### 1. Verify domain expiry config matches RDAP

```bash
./deploy/check-expiry.sh
```

Exits 0 if `operator_lease_expires` in `deploy/config-multi-zone.toml` matches the actual RDAP expiration date for nodns.shop. Run with `--update` to auto-fix mismatches.

### 2. Run tests locally

```bash
cd nodns-bot-rs && cargo test
```

All unit tests must pass before deploying. This takes ~5 minutes on first run.

### 3. Cross-compile for Linux x86_64

```bash
cd nodns-bot-rs
cargo zigbuild --release --target x86_64-unknown-linux-gnu
```

Output binary: `nodns-bot-rs/target/x86_64-unknown-linux-gnu/release/nodns-bot`

First build takes ~10 minutes; incremental builds ~2 minutes.

### 4. Upload binary to VPS

```bash
scp nodns-bot-rs/target/x86_64-unknown-linux-gnu/release/nodns-bot \
    root@46.224.104.12:/opt/nodns-bot/nodns-bot
```

### 5. Restart the service

```bash
ssh root@46.224.104.12 \
    "systemctl restart nodns-bot && sleep 2 && systemctl status nodns-bot --no-pager"
```

### 6. Verify

```bash
ssh root@46.224.104.12 'curl -s http://127.0.0.1:9090/health'
```

Should return JSON with `"status":"ok"`.

## CI Pipeline

CI (`.github/workflows/build-bot.yml`) runs on every push to `main` that touches `nodns-bot-rs/**` or `nodns-cli/**`:

1. `cargo fmt --check` — formatting must be clean
2. `cargo clippy -- -D warnings` — zero warnings allowed
3. `cargo build --release` — must compile
4. `cargo test` — all unit tests must pass
5. Domain expiry verification via `deploy/check-expiry.sh`
6. Binary uploaded as artifact (5-day retention)

CI does NOT auto-deploy. The deploy script must be run manually.

## VPS File Layout

| Path | Purpose |
|---|---|
| `/opt/nodns-bot/nodns-bot` | Binary (uploaded by deploy script) |
| `/opt/nodns-bot/config.toml` | Production config (TSIG keys, relay list, pricing) |
| `/opt/nodns-bot/records.db` | SQLite database (events, delegations, ACME orders) |
| `/etc/systemd/system/nodns-bot.service` | Systemd unit file |

## Rollback

The previous binary is overwritten on upload. To rollback:

1. SSH to the VPS
2. The systemd journal retains old logs: `journalctl -u nodns-bot -n 100`
3. Rebuild from the previous git commit: `git checkout <prev-hash> && ./deploy/deploy.sh --push`

## Frontend Deployment

The frontend deploys automatically via GitHub Actions (`.github/workflows/deploy-pages.yml`) on every push to `main`. No manual steps needed — the static export is published to GitHub Pages at `https://amperstrand.github.io/nodns-poc/`.

The VPS serves a copy at `/var/www/nodns.shop/` via Caddy for `nodns.shop` / `*.nodns.shop` domains. To update the VPS copy:

```bash
cd nodns-frontend && npm run build
scp -r out/* root@46.224.104.12:/var/www/nodns.shop/
```
