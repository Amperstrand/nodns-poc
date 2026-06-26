# Kind 31111 Migration — Parameterized Replaceable DNS Events

**Status**: ACTIVE
**Issue**: [#59](https://github.com/amperstrand/nodns-poc/issues/59)

## Overview

NoDNS DNS record events are migrating from kind **11111** (regular) to kind **31111** (NIP-33 parameterized replaceable). The bot now accepts both kinds simultaneously during the transition period.

## Why 31111?

Kind 31111 is a **parameterized replaceable** event per [NIP-33](https://github.com/nostr-protocol/nips/blob/master/33.md). Relays keep only the latest event per coordinate tuple `(kind, pubkey, d-tag)`, which provides three benefits over kind 11111:

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

## Migration Timeline

| Phase | Status | Description |
|---|---|---|
| **1. Accept both** | **Current** | Bot subscribes to and processes both 11111 and 31111. Clients may publish either. |
| **2. Prefer 31111** | Planned | Frontend and CLI switch to publishing 31111 by default. 11111 still accepted. |
| **3. Deprecate 11111** | Future | Bot stops subscribing to 11111. Only 31111 accepted. Existing 11111 records remain in DNS until updated. |

## Client Subscription Guide

During the transition, clients should subscribe to **both** kinds:

```javascript
const filter = {
  kinds: [11111, 31111],
  since: lastSeenTimestamp,
};
```

After deprecation, subscribe to 31111 only:

```javascript
const filter = {
  kinds: [31111],
};
```

## Bot Implementation Details

- **`subscriber.rs`**: Subscribes with `kinds: [11111, 31111]` in the relay filter.
- **`parser.rs`**: `is_dns_kind()` accepts both kinds. The `d` tag is extracted into `ParsedEvent.d_tag` for logging.
- **`event_processor.rs`**: Logs the `d` tag coordinate for 31111 events. Proof TXT records include the actual event kind (`k=31111` or `k=11111`).
- **`store.rs`**: A `kind` column (default 11111) is added to the `events` table via migration. Non-Nostr updates (RFC 2136, DynDNS HTTP) store kind `0`.
- **`types.rs`**: `KIND_DNS_REPLACEABLE = 31111` constant alongside existing `KIND_DNS_RECORD = 11111`.
