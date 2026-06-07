# nodns-bot Specification

> **Note**: This document describes the original Go bot. The Go bot is archived in `nodns-bot-archive/`. The current production bot is written in Rust — see `nodns-bot-rs/src/` for the authoritative implementation. Key differences:
> - Language: Rust (not Go)
> - Dependencies: `nostr-sdk` 0.39, `hickory-client` 0.25, `cdk` 0.17 (Cashu), `rusqlite` 0.34, `axum` 0.8
> - Module layout: `main.rs`, `auth.rs`, `config.rs`, `dns.rs`, `parser.rs`, `payment.rs`, `store.rs`, `subscriber.rs`, `types.rs`
> - Cashu payment verification: Fully implemented using CDK crate (gated off by config)
> - Multi-zone support: Via `[[dns.zones]]` TOML arrays with backward compatibility

## Overview

`nodns-bot` is a daemon that bridges Nostr relays and Knot DNS. It subscribes to kind 11111 events, validates them against policy rules, and pushes DNS records to Knot via DDNS updates.

## Module Layout

```
nodns-bot/
├── main.go                  # Entry point, config loading, orchestration
├── internal/
│   ├── config/
│   │   └── config.go        # TOML config parser
│   ├── nostr/
│   │   ├── subscriber.go    # Relay pool, subscription management
│   │   ├── filter.go        # Kind 11111 filter builder
│   │   ├── validator.go     # Event signature + tag validation
│   │   └── parser.go        # Extract DNS records from events
│   ├── dns/
│   │   ├── updater.go       # DDNS client (miekg/dns)
│   │   ├── zonefile.go      # Initial zone bootstrapping
│   │   └── types.go         # Record type constants
│   ├── policy/
│   │   ├── policy.go        # Policy engine interface
│   │   ├── ratelimit.go     # Per-npub rate limiting
│   │   ├── validation.go    # IP/hostname validation
│   │   └── abuse.go         # Abuse detection (known-bad ranges)
│   ├── store/
│   │   ├── store.go         # State persistence interface
│   │   ├── sqlite.go        # SQLite implementation
│   │   └── models.go        # RecordEvent model
│   └── cashu/
│       └── paid.go          # Cashu proof verification (Phase 2)
├── config.toml              # Production config
├── config.demo.toml         # Demo config
├── go.mod
└── go.sum
```

## Configuration (`config.toml`)

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
tsig_key_secret = "base64-encoded-secret"
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

## Dependencies

| Package | Purpose |
|---|---|
| `github.com/nbd-wtf/go-nostr` | Nostr client (relay pool, event parsing, signature verification) |
| `github.com/miekg/dns` | DNS library (DDNS UPDATE messages, TSIG signing) |
| `github.com/BurntSushi/toml` | Config parsing |
| `modernc.org/sqlite` | SQLite driver (no CGo needed) |

## Core Data Flow

### 1. Boot Sequence

```
Load config.toml
  → Open SQLite DB (create if not exists)
  → Connect to Knot DNS (TSIG auth test)
  → Bootstrap zone if empty (write SOA + NS via DDNS)
  → Start HTTP health server (:9090/health)
  → Subscribe to Nostr relays
```

### 2. Subscription Setup

The bot establishes persistent WebSocket connections to all configured relays and sends a subscription filter:

```json
{
  "kinds": [11111],
  "#x": ["nostr.cv"],
  "since": 1749000000
}
```

The `since` timestamp is read from SQLite (last processed event). On first run, the bot processes all historical events.

### 3. Event Processing

```
Event arrives from relay
  │
  ├─ Validate signature (go-nostr)
  │   └─ REJECT if invalid
  │
  ├─ Check kind == 11111
  │   └─ SKIP if not
  │
  ├─ Parse "record" tag (11-element fixed format)
  │   └─ REJECT if malformed
  │
  ├─ Extract npub from event.PubKey
  │
  ├─ Compute FQDN: {name}.npub{short}.{zone}
  │   Example: www.npub1b3e4f7a1.nostr.cv.
  │
  ├─ POLICY CHECK:
  │   ├─ Rate limit: max {rate_limit} events/minute for this npub?
  │   ├─ Record count: < {max_records} for this npub?
  │   ├─ Type: in {allowed_types}?
  │   ├─ IP: not private/reserved (if block_private_ips)?
  │   └─ TXT: length < {max_txt_length}?
  │   └─ REJECT if any check fails
  │
  ├─ DDNS UPDATE:
  │   ├─ Build UPDATE message:
  │   │   DELETE {fqdn} {type}    (remove old records for this name+type)
  │   │   ADD {fqdn} {ttl} {type} {rdata}
  │   ├─ Sign with TSIG ({tsig_key_name})
  │   ├─ Send to {knot_address}
  │   └─ Verify NOERROR response
  │   └─ LOG ERROR if SERVFAIL/REFUSED
  │
  └─ PERSIST:
      ├─ Upsert SQLite: (event_id, npub, name, type, rdata, ttl, created_at)
      └─ Update last_seen timestamp
```

### 4. Deletion Handling (kind 5)

Nostr kind 5 events are deletion requests. The bot handles them:

```
Kind 5 event arrives
  ├─ Extract "e" tag (referenced event ID)
  ├─ Look up referenced event in SQLite
  │   └─ SKIP if not found (event not in our zone)
  ├─ Verify deletion event pubkey matches original event pubkey
  │   └─ REJECT if mismatch (can't delete someone else's records)
  ├─ Send DDNS DELETE for all records from the original event
  └─ Remove from SQLite
```

### 5. Reconciliation (every 5 minutes)

```
AXFR query to Knot → get full zone dump
  ├─ Parse all records
  ├─ Compare with SQLite state
  ├─ Detect drift:
  │   ├─ Records in Knot but not SQLite → add to SQLite (Knot is truth)
  │   └─ Records in SQLite but not Knot → remove from SQLite (Knot is truth)
  └─ Log any discrepancies
```

This catches edge cases: bot crashed mid-update, DDNS failed silently, manual zone edits by admin.

## Nostr Event to DNS Record Mapping

The NoDNS protocol defines kind 11111 events with a fixed 11-element `record` tag:

```
Tag element:  [0]      [1]     [2]    [3]   [4]    [5]        [6-9]  [10]
              record   name    type   class ttl    rdata...   unused zone
```

### Example: A Record

```json
{
  "kind": 11111,
  "pubkey": "b3e4...f7a1",
  "tags": [
    ["record", "www", "A", "IN", "3600", "203.0.113.42", "", "", "", "", "nostr.cv"]
  ],
  "content": "",
  "created_at": 1749000000
}
```

Produces:
```
DDNS UPDATE:
  DELETE www.npub1b3e4f7a1.nostr.cv. A
  ADD    www.npub1b3e4f7a1.nostr.cv. 3600 A 203.0.113.42
```

### Example: AAAA Record

```json
{
  "kind": 11111,
  "pubkey": "b3e4...f7a1",
  "tags": [
    ["record", "", "AAAA", "IN", "3600", "2001:db8::1", "", "", "", "", "nostr.cv"]
  ]
}
```

Produces:
```
DDNS UPDATE:
  DELETE npub1b3e4f7a1.nostr.cv. AAAA
  ADD    npub1b3e4f7a1.nostr.cv. 3600 AAAA 2001:db8::1
```

### Example: CNAME Record

```json
{
  "kind": 11111,
  "pubkey": "b3e4...f7a1",
  "tags": [
    ["record", "blog", "CNAME", "IN", "3600", "myblog.github.io.", "", "", "", "", "nostr.cv"]
  ]
}
```

### FQDN Construction

The bot constructs FQDNs as:
```
{name}.npub1{first8chars}.{zone}
```

Where `{name}` is the subdomain requested by the user (element [1]). If empty, the apex of the npub subdomain is used.

Examples:
- `""` + npub `b3e4...f7a1` → `npub1b3e4f7a1.nostr.cv`
- `"www"` + npub `b3e4...f7a1` → `www.npub1b3e4f7a1.nostr.cv`
- `"blog"` + npub `b3e4...f7a1` → `blog.npub1b3e4f7a1.nostr.cv`

The truncated npub (first 8 hex chars) is used for human readability in zone files and logs. The full npub is stored in SQLite for validation.

## Policy Engine

### Technical Policies (enforced by bot)

| Policy | Default | Rationale |
|---|---|---|
| Max records per npub | 20 | Prevent zone bloat |
| Rate limit | 5 events/min per npub | Prevent relay spam from affecting DNS |
| Allowed record types | A, AAAA, CNAME, TXT, MX | Limit attack surface |
| Block private IPs | Yes | Prevent DNS rebinding to internal networks |
| Max TXT length | 512 chars | Standard DNS TXT limit |
| Block known-bad IP ranges | Yes | Tor exit nodes, known C2 infrastructure |

### Rate Limiting Implementation

Per-npub sliding window:
- Track event timestamps in SQLite
- Count events in the last 60 seconds for the npub
- Reject if count >= `rate_limit`
- Log rejected events for monitoring

### IP Validation

Block these ranges (configurable):
- `10.0.0.0/8` — RFC 1918
- `172.16.0.0/12` — RFC 1918
- `192.168.0.0/16` — RFC 1918
- `127.0.0.0/8` — Loopback
- `169.254.0.0/16` — Link-local
- `0.0.0.0/8` — "This network"
- `100.64.0.0/10` — CGNAT
- `fc00::/7` — IPv6 ULA
- `fe80::/10` — IPv6 link-local
- `::1/128` — IPv6 loopback

## SQLite Schema

```sql
CREATE TABLE IF NOT EXISTS events (
    event_id TEXT PRIMARY KEY,
    npub TEXT NOT NULL,
    pubkey TEXT NOT NULL,
    name TEXT NOT NULL,
    record_type TEXT NOT NULL,
    ttl INTEGER NOT NULL,
    rdata TEXT NOT NULL,
    zone TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    processed_at INTEGER NOT NULL DEFAULT (unixepoch()),
    deleted INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_events_npub ON events(npub);
CREATE INDEX idx_events_npub_type ON events(npub, record_type);
CREATE INDEX idx_events_fqdn ON events(name, npub, zone);
CREATE INDEX idx_events_created ON events(created_at);

CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Track last seen event timestamp for relay subscription resume
INSERT OR IGNORE INTO meta (key, value) VALUES ('last_seen', '0');
```

## Health Check Endpoint

`GET /health` returns JSON:

```json
{
  "status": "ok",
  "uptime_seconds": 86400,
  "relay_connections": 4,
  "relay_connected": ["wss://relay.damus.io", "wss://nos.lol", "wss://relay.nostr.band"],
  "relay_disconnected": ["wss://nostr.wine"],
  "events_processed": 1234,
  "events_rejected": 5,
  "ddns_successes": 1229,
  "ddns_failures": 0,
  "zone_records": 567,
  "last_event_at": 1749000000,
  "last_reconciliation_at": 1748999700,
  "sqlite_size_bytes": 1048576
}
```

## Error Handling

| Error | Response |
|---|---|
| Relay disconnect | Reconnect with exponential backoff (1s → 60s) |
| DDNS SERVFAIL | Retry 3 times with 100ms delay. Log and alert if persistent |
| DDNS REFUSED | Log error. TSIG key likely mismatched — critical alert |
| SQLite error | Fatal — restart. SQLite is local, should never fail |
| Invalid event | Log and skip. Don't block the subscription |
| Policy rejection | Log reason. Increment rejected counter |
| Knot not responding | Health check fails. Alert. Don't process events until Knot is back |

## Monitoring (Production)

Metrics to expose (Prometheus format or structured logs):

- `nodns_events_received_total` (counter, labels: relay, kind)
- `nodns_events_processed_total` (counter)
- `nodns_events_rejected_total` (counter, labels: reason)
- `nodns_ddns_updates_total` (counter, labels: result)
- `nodns_ddns_latency_seconds` (histogram)
- `nodns_relay_connected` (gauge, labels: relay)
- `nodns_zone_records` (gauge)
- `nodns_reconciliation_drift_total` (counter)
