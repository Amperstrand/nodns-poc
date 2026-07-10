# Known Issues

Issues that are documented but not yet resolved. Each links to the corresponding
GitHub issue for tracking.

## No VPS database backup (cashu-cf#4)

**Risk**: If the VPS disk fails, the SQLite database at
`/opt/nodns-bot/records.db` is lost. The bot must then replay all historical
Nostr events to rebuild state — events that relays may have already evicted.

**Mitigation**: None currently in place.

**Recommended fix**: [Litestream](https://litestream.io/) continuous WAL
replication to Cloudflare R2. See
[Amperstrand/cashu-cf#4](https://github.com/Amperstrand/cashu-cf/issues/4) for
full implementation details.

**Interim measure**: A manual backup can be taken at any time:

```bash
ssh root@46.224.104.12 \
  "sqlite3 /opt/nodns-bot/records.db '.backup /tmp/records-backup.db'" \
  && scp root@46.224.104.12:/tmp/records-backup.db ./records-backup.db
```

> **Do NOT** `cp` the `.db` file directly — SQLite WAL mode requires the
> `.backup()` API for a consistent snapshot.

## DoH resolver is experimental (2026-07-10)

**Status**: Deployed and functional, but uses testnut Cashu (no monetary value).

**Risk**: The Cashu-gated DoH resolver at `dns.nodns.shop` uses testnut tokens
for anti-spam. A determined attacker could automate the testnut faucet to
obtain tokens. Per-subscription rate limits (10,000 queries/day) and Caddy
IP rate limiting mitigate this, but the gate is friction-based, not
economic.

**Limitation**: Browser-native DoH (Firefox, Chrome) cannot send custom
headers. Premium tier users need a local proxy (`tools/doh-proxy.py`) to
inject the `X-Subscription` header. The free tier (`.nostr` resolution)
works without headers and is fully browser-native.

**Future**: Switch `mint_filter` from `testnut` to a real-sats mint when
the experiment graduates to production. See `docs/47-resolver-service.md`.

## BSI open resolver incident — RESOLVED (2026-07-09)

**Status**: Fixed. Documented for historical reference.

A BSI/Hetzner abuse report (CB-Report 2026-07-07) flagged `46.224.104.12`
as an open recursive resolver. Root cause: Knot's `mod-dnsproxy` module was
attached as a `global-module` on the `default` template, forwarding
non-authoritative queries to Cloudflare `1.1.1.1`. Fixed by removing the
`global-module` line. Additional hardening: `mod-rrl` (rate-limit 200, slip 2),
`server.nsid: ""`, `server.version: ""`. See `deploy/DEPLOY.md` → "DNS
hardening".
