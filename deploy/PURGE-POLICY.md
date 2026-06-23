# VPS Purge Policy

## Whitelist (NEVER delete)

| Pattern | Reason |
|---|---|
| `*.utxo`, `utxo*` | Bitcoin UTXO sets — expensive to rebuild |
| `*chainstate*` | Bitcoin chain state |
| `*.key`, `*.pem` | TLS private keys |
| `config.toml` | Bot production config (contains secrets) |
| `records.db` | Bot SQLite database |
| `knot.conf`, `*.zone` | DNS zone files |
| `*.db-wal`, `*.db-shm` | SQLite WAL files |
| Releases with >100 downloads | Actively used, potential monetization |

## Expiry Rules

### System Files

| Category | Location | Max Age | Action |
|---|---|---|---|
| Temp files | `/tmp/*` | 7 days | Delete |
| System logs | `/var/log/*.log` | 30 days | Delete |
| Journal | systemd journal | 7 days | Vacuum |
| Bot binary backups | `/opt/nodns-bot/*.bak*` | 7 days | Delete |
| Old web backups | `/var/www/*-backup-*` | 7 days | Delete |
| Cargo cache | `~/.cargo/registry/cache` | 30 days | Prune stale |
| npm cache | `~/.npm/_cacache` | 30 days | Prune stale |

### Test Artifacts (immediate)

| Category | Pattern | Max Age | Action |
|---|---|---|---|
| fio benchmarks | `seq-*`, `rand-*`, `fio*` | 1 day | Delete |
| Build dirs | `/tmp/*-build` | 1 day | Delete |
| Temp binaries | `/tmp/bark-*`, `/tmp/tollgate-*` | 1 day | Delete |
| FIPS test logs | `fips.log`, `fips/` | 1 day | Delete |

### Rust Toolchains

Keep only `stable` and explicitly pinned versions. Remove all others.
```bash
rustup toolchain list | grep -v stable | grep -v nightly | while read tc; do
  rustup toolchain remove "$tc"
done
```

## Tollgate Release Expiry (releases.tollgate.me)

Apply on the server hosting release files. Not this VPS.

| Release Type | Pattern | Max Age | Rationale |
|---|---|---|---|
| Stable point releases | `tollgate-*-v[0-9]*.[0-9]*.[0-9]*.ipk` | Never | Production releases, users depend on them |
| Release notes | `*.md`, `CHANGELOG*` | Never | Documentation |
| Alpha/beta releases | `tollgate-*-alpha-*`, `tollgate-*-beta-*` | 90 days | Pre-release, superseded by stable |
| Branch IPK (never downloaded) | `tollgate-*-branch-*.ipk` with 0 downloads | 7 days | CI artifact for testing, not production |
| Branch IPK (downloaded) | `tollgate-*-branch-*.ipk` with >0 downloads | 30 days | Someone is using it |
| Old CI artifacts | `tollgate-*-ci-*-*.ipk` | 3 days | Ephemeral build artifacts |
| Source tarballs | `*.tar.gz`, `*.zip` | 365 days | Archive copies |

### Download-count aware purge

For branch/CI IPKs, check download count before purging:
```bash
# Pseudo-code for release file purge
for file in releases/tollgate-*-branch-*.ipk; do
  downloads=$(get_download_count "$file")
  age_days=$(( (now - mtime) / 86400 ))
  if [ "$downloads" -eq 0 ] && [ "$age_days" -gt 7 ]; then
    delete "$file"
  elif [ "$downloads" -gt 0 ] && [ "$age_days" -gt 30 ]; then
    delete "$file"
  fi
done
```

## Cron Schedule

```
0 4 * * * /opt/nodns-bot/purge-rules.sh
```

Daily at 4am UTC. Logs to `/var/log/nodns-purge.log`.

## Manual Run

```bash
# Dry run (show what would be deleted)
/opt/nodns-bot/purge-rules.sh --dry

# Execute purge
/opt/nodns-bot/purge-rules.sh
```

## Pubkey Labels

Known pubkeys are labeled in bot config under `[pubkey_labels]`:

| Pubkey (hex prefix) | Label | Source |
|---|---|---|
| `79be667e...` | nak-test-key | secp256k1 generator — used by nak for testing |
| `bbb5dda0...` | nodns-registrar | Official nodns registrar key |

Add more as Amperstrand project pubkeys are identified:
- Tollgate release pubkey
- nomail service pubkey
- blossomflare service pubkey
- Cashu exchange service pubkey

The bot includes labels in API responses so the admin dashboard can display human-readable names instead of raw npubs.
