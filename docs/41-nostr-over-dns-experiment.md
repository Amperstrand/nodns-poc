# 41 — Nostr-over-DNS: Event Caching Experiment

> **Status**: DRAFT. Experimental feature. All failures are non-fatal (warn-only).

## Overview

This experiment caches selected Nostr events as DNS TXT records, making them
retrievable via a standard DNS query — no Nostr client, relay subscription,
or websocket required. A `dig` command is sufficient.

The zone operator controls what gets cached by enabling `dns_cache_events`
in their zone config. When enabled, the bot writes compact event JSON to
predictable DNS names.

## How It Works

```
Nostr event (kind 0, 11111, etc.)
        │
        ▼
  event_processor.rs
        │  if zone_config.dns_cache_events == true
        ▼
  DnsEventCache::try_cache(backend, event, zone)
        │
        ├─ kind 0 (profile) → _profile.npub1xxx.{zone} TXT {compact_json}
        ├─ kind 11111 (record) → note1xxx.{zone} TXT {compact_json}
        └─ other kinds → note1xxx.{zone} TXT {compact_json}
        │
        ▼
  DDNS UPDATE to Knot DNS (or Cloudflare API)
        │
        ▼
  Resolvable globally via dig/curl/any DNS client
```

## Querying

### Event by ID

Every cached event is stored at `note1{event_id_bech32}.{zone}`:

```bash
dig note1qyqwk4y9wchz5qpr7u9kmtcewqj5fthenxq9p5lqsl.{zone} TXT +short
```

The response is a compact JSON representation of the event, split into
multiple TXT character-strings (each ≤255 bytes per RFC 1035):

```
{"k":11111,"p":"7eff...","s":"4700...","i":"3407...","t":1234567890,"c":"","tags":[["record","A","","1.2.3.4"]]}
```

### Profile by pubkey

Kind 0 (profile) events are cached at `_profile.npub1{pubkey}.{zone}`:

```bash
dig _profile.npub1qyqwk4y9wchz5qpr7u9kmtcewqj5fthenxq9p5lqsl.{zone} TXT +short
```

## Compact Event JSON Format

```json
{
  "k": 11111,
  "p": "7effcccb48fc9d091a8cab663a566523c8249d7770d5fd3c31c96a0f2b8db9ed",
  "s": "4700c0c16b5d9d0e...",
  "i": "3407e8f7a4c7...",
  "t": 1234567890,
  "c": "",
  "tags": [["record", "A", "", "1.2.3.4", "", "", "", "", "", "", "3600"]]
}
```

| Field | Description |
|---|---|
| `k` | Event kind (u16) |
| `p` | Pubkey (hex, 64 chars) |
| `s` | Signature (hex, 128 chars) |
| `i` | Event ID (hex, 64 chars) |
| `t` | Created at (Unix timestamp) |
| `c` | Content string |
| `tags` | Tag array (same as NIP-01) |

The JSON is split into ≤255-byte segments for DNS TXT wire format. A
resolver reassembles them in order (TXT character-strings are concatenated
per RFC 1035 §3.3.14).

## DNS Label Constraint

Nostr event IDs are 32 bytes. Bech32 encoding with the `note` HRP produces
exactly 63 characters (`note1` + 58 data chars + 1 checksum char) — this
fits within the DNS label limit of 63 characters (RFC 1035).

Similarly, npub bech32 encoding produces 63 characters.

## Configuration

Enable per zone in `config.toml`:

```toml
[[dns.zones]]
zone = "nodns.shop"
knot_address = "127.0.0.1:5353"
tsig_key_name = "nodns-bot"
tsig_key_secret = "base64-secret"
dns_cache_events = true
```

Or with Cloudflare backend:

```toml
[[dns.zones]]
zone = "dns4sats.xyz"
backend = "cloudflare"
cloudflare_api_token = "token"
cloudflare_zone_id = "zone-id"
dns_cache_events = true
```

Default: `false` (experimental).

## Limitations

- **Knot DNS / Cloudflare only**: The cache writes via the zone's configured
  DNS backend (DDNS or Cloudflare API). Other backends are not yet supported.
- **Zone operator controls caching**: Only events seen by the bot's relay
  subscription are cached. The operator chooses which relays to subscribe to.
- **No filtering yet**: Currently caches all events the bot processes. Future
  versions could add proof-of-burn filtering or kind-based selection.
- **TXT size**: Large events with many tags may exceed practical TXT record
  sizes. The bot splits into 255-byte segments, but some resolvers may not
  return all segments.
- **No eviction**: Cached records persist until overwritten or manually
  deleted. No TTL-based eviction is implemented.
- **Experimental**: All cache failures are non-fatal (logged as warnings).
  A cache write failure does not affect the main event processing pipeline.

## Prior Art

| System | What it does | Relationship |
|---|---|---|
| **OpenAlias** | Stores crypto addresses as DNS TXT records | Same mechanism (TXT), different content (payment addresses vs events) |
| **DKIM** | Stores public keys in DNS TXT | DNS-as-key-directory precedent |
| **EIP-1459** | ENS node content hash for discovery | Nostr equivalent: event discovery via DNS |
| **BIP 353** | Bitcoin payment instructions in DNS | DNS-as-lookup for crypto primitives |
| **DNSSEC** | Cryptographic authenticity for DNS | NoDNS zones are DNSSEC-signed; cached events inherit this trust |

## Future Ideas

- **Proof-of-burn filtering**: Only cache events from pubkeys that have
  demonstrated commitment (burned sats, PoW, etc.)
- **Profile lookups**: `_profile.npub1...{zone}` as a decentralized
  identity directory — resolve a user's metadata without Nostr infrastructure
- **Event discovery**: Query DNS to discover which events exist on-chain,
  then fetch full events from relays
- **NIP-05 integration**: Use `_profile.npub1...{zone}` as a NIP-05 source
  for verified identity
- **Selective caching**: Per-kind filtering — cache only kind 0 (profiles),
  kind 10002 (relay lists), etc.
- **Garbage collection**: TTL-based eviction of stale cached events
