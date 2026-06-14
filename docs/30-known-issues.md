# Known Issues

Issues that are documented but not yet resolved. Each links to the corresponding
GitHub issue for tracking.

## No VPS database backup (cashu-cf#4)

**Risk**: If the VPS disk fails, the SQLite database at
`/var/lib/nodns-bot/records.db` is lost. The bot must then replay all historical
Nostr events to rebuild state — events that relays may have already evicted.

**Mitigation**: None currently in place.

**Recommended fix**: [Litestream](https://litestream.io/) continuous WAL
replication to Cloudflare R2. See
[Amperstrand/cashu-cf#4](https://github.com/Amperstrand/cashu-cf/issues/4) for
full implementation details.

**Interim measure**: A manual backup can be taken at any time:

```bash
ssh root@46.224.104.12 \
  "sqlite3 /var/lib/nodns-bot/records.db '.backup /tmp/records-backup.db'" \
  && scp root@46.224.104.12:/tmp/records-backup.db ./records-backup.db
```

> **Do NOT** `cp` the `.db` file directly — SQLite WAL mode requires the
> `.backup()` API for a consistent snapshot.
