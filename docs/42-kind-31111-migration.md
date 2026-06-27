# Kind 31111 Migration — Historical Note

**Status**: ARCHIVED
**Issue**: [#59](https://github.com/amperstrand/nodns-poc/issues/59)

## Overview

NoDNS DNS record events are staying on kind **11111**. The 31111 idea is recorded here only as historical context; it is not part of the current protocol or roadmap.

## Why 31111?

Kind 31111 is a **parameterized replaceable** event per [NIP-33](https://github.com/nostr-protocol/nips/blob/master/33.md). Relays keep only the latest event per coordinate tuple `(kind, pubkey, d-tag)`, which would provide three benefits over kind 11111:

1. **Automatic deduplication** — Relays discard older events with the same coordinate. No record flooding or stale-event replay.
2. **Stable reference** — The `d` tag creates a persistent identifier for a specific record, enabling clean updates without ambiguity.
3. **Reduced relay load** — Fewer stored events means smaller relay databases and faster subscription responses.

## The `d` Tag Format

For kind 31111 events, a `d` tag is required to form the NIP-33 coordinate:

```json
["d", "{record_type}:{name}.{zone}"]
```

Examples:

| Record | d tag value |
|---|---|
| A record at apex for npub | `["d", "A:@.nodns.shop"]` |
| TXT record for "alice" | `["d", "TXT:alice.nodns.shop"]` |
| AAAA record for "www" | `["d", "AAAA:www.nodns.shop"]` |

The `d` tag value is informational — the bot extracts and logs it for debugging but does not use it for routing. The actual record data comes from the `record` tags, which are identical for both kinds.

## Full Example

```json
{
  "kind": 31111,
  "content": "",
  "tags": [
    ["d", "A:@.nodns.shop"],
    ["record", "A", "", "3600", "1.2.3.4"]
  ]
}
```

## Tag Format (Unchanged)

The `record` tag format is identical for both kinds:

```json
["record", "TYPE", "NAME", "TTL", "RDATA"]
```

All validation (allowed types, private IP blocking, TXT length cap, DNS label rules) applies equally to both kinds.

## Historical Timeline

| Phase | Status | Description |
|---|---|---|
| **1. Stay on 11111** | **Current** | Bot subscribes to and processes 11111 only. This is the live protocol. |
| **2. 31111 idea** | Archived | Considered for relay deduplication, but not pursued. |
| **3. Migration path** | Archived | No migration planned. |

## Client Subscription Guide

Current clients should subscribe to **11111 only**.

```javascript
const filter = {
  kinds: [11111],
  since: lastSeenTimestamp,
};
```

Clients should subscribe to 11111 only.

## Bot Implementation Details

- **`subscriber.rs`**: Subscribes with `kinds: [11111]` in the relay filter.
- **`parser.rs`**: `is_dns_kind()` accepts 11111 only.
- **`event_processor.rs`**: Emits the v1 event kind in proofs and logs.
- **`store.rs`**: A `kind` column (default 11111) is stored for events; non-Nostr updates (RFC 2136, DynDNS HTTP) store kind `0`.
- **`types.rs`**: `KIND_DNS_RECORD = 11111` remains the only protocol kind for v1.
