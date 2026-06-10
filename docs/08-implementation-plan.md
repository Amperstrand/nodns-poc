# 08 — Implementation Plan (Original)

> **Status**: SUPERSEDED by [27-implementation-plan.md](27-implementation-plan.md).

## Protocol Spec Reference

The NoDNS protocol is defined in two repos:
- **Spec**: `nodns-protocol-spec` at `relay.ngit.dev` — `record_events.md`, `record_events_examples.md`, `cert_events.md`
- **Reference implementation**: `nodns-server` at `relay.ngit.dev` — JIT resolver (queries relay per DNS query)

### Tag Format: Two Versions to Support

**New format (5-element)** — defined in `record_events.md`:
```json
["record", "TYPE", "name", "TTL", "rdata"]
```
- `TYPE`: DNS record type (A, AAAA, CNAME, MX, TXT, SRV, etc.)
- `name`: "@" for root domain, subdomain name, or FQDN
- `TTL`: seconds as string (default 3600 if empty)
- `rdata`: zone-file-style data (e.g., `"192.168.1.1"` for A, `"10 mail.example.com."` for MX)

**Legacy format (11-element)** — used in `record_events_examples.md` and parsed by reference `nodns-server`:
```json
["record", "TYPE", "name", "pos1", "pos2", "pos3", "pos4", "pos5", "pos6", "pos7", "ttl"]
```
- Data fields in positions 3-9, TTL in position 10

The spec says: "Implementations SHOULD support both formats during a transition period." **Our bot must parse both.**

### Key Protocol Rules

1. Kind 11111 events only. No other kinds for DNS records.
2. `content` field MUST be empty string `""`
3. Only `record` tags allowed (no other tag types)
4. Event signature proves authenticity — the pubkey (npub) IS the domain identity
5. Kind 5 events are deletion requests (reference a kind 11111 event ID)
6. The `_nostr` subdomain is reserved for returning the raw event as a NULL record

### Domain Mapping

Per the spec: `{npub}.nostr` maps to the zone managed by that pubkey's events.

For our system (`nodns.shop`): `{npub}.nodns.shop` maps to the zone managed by that pubkey.

## Architecture: How Our Bot Differs from Reference

| Aspect | Reference `nodns-server` | Our `nodns-bot` |
|---|---|---|
| Resolution model | JIT — query relay on every DNS request | Prefetched — subscribe to relays, push to Knot |
| DNS serving | Custom Go DNS server (`miekg/dns`) | Knot DNS (production-grade authoritative server) |
| Caching | None | Zone-level (Knot RCU) |
| Latency | 1-5 seconds (relay round-trip per query) | <1ms (served from Knot memory) |
| DNSSEC | No | Yes (Knot handles automatically) |
| Secondaries | N/A | NOTIFY → puck.nether.net (IXFR) |
| Protocol formats | 11-element only | Both 5-element and 11-element |
| Zone file | No zone file | Knot zone, updated via DDNS |

## Implementation Phases

### Phase 1: Minimal Working Bot (MVP)

**Goal**: Subscribe to kind 11111 events, parse records, push to Knot via DDNS. One npub resolves.

**Files to create:**

```
nodns-bot/
├── main.go
├── internal/
│   ├── config/
│   │   └── config.go          # Config loading (TOML)
│   ├── nostr/
│   │   ├── subscriber.go      # Relay pool + subscription
│   │   ├── parser.go          # Parse both 5-element and 11-element tags
│   │   └── npub.go            # npub ↔ hex conversion
│   ├── dns/
│   │   ├── updater.go         # DDNS client (miekg/dns, TSIG)
│   │   └── records.go         # Record type constants + conversion
│   └── store/
│       └── sqlite.go          # SQLite persistence
├── config.toml
├── go.mod
└── go.sum
```

**Dependencies:**
- `github.com/nbd-wtf/go-nostr` — Nostr relay pool, event parsing, signature verification
- `github.com/miekg/dns` — DNS UPDATE messages, TSIG signing (same as reference implementation)
- `github.com/BurntSushi/toml` — Config
- `modernc.org/sqlite` — SQLite (no CGo)

**Reference implementation files to reuse patterns from:**
- `nodns-server/internal/nostr/client.go` — Event fetching, npub extraction, record parsing, validation
- `nodns-server/internal/dns/server.go` — DNS record type conversion (`convertSingleRecord`)
- `nodns-server/internal/config/config.go` — Config structure and defaults

### Phase 1 Implementation Steps

#### 1.1 Config (`internal/config/config.go`)

```go
type Config struct {
    Nostr  NostrConfig
    DNS    DNSConfig
    Policy PolicyConfig
    Store  StoreConfig
}

type NostrConfig struct {
    Relays        []string
    Zone          string // "nodns.shop"
    ReconnectMin  time.Duration
    ReconnectMax  time.Duration
}

type DNSConfig struct {
    KnotAddress   string // "127.0.0.1:53"
    Zone          string // "nodns.shop"
    TSIGKeyName   string
    TSIGSecret    string // base64
    TSIGAlgorithm string // "hmac-sha256"
    DefaultTTL    uint32
    NegativeTTL   uint32
}

type PolicyConfig struct {
    MaxRecords     int
    RateLimit      int // per npub per minute
    AllowedTypes   []string
    BlockPrivateIP bool
    MaxTXTLength   int
}

type StoreConfig struct {
    Path string // "/var/lib/nodns-bot/records.db"
}
```

#### 1.2 Nostr Subscriber (`internal/nostr/subscriber.go`)

Use `go-nostr` RelayPool for persistent connections to multiple relays.

```go
// Subscribe creates a persistent subscription for kind 11111 events
// matching our zone. Returns a channel of verified events.
func (s *Subscriber) Subscribe(ctx context.Context) (<-chan *nostr.Event, error)
```

Filter: `{kinds: [11111], since: lastSeenTimestamp}`

The reference implementation (`client.go`) shows how to:
- Connect to relays (`nostr.RelayConnect`)
- Subscribe with filters
- Collect events with timeout
- Handle `EndOfStoredEvents`

Key difference: we use `go-nostr` RelayPool for persistent connections instead of per-query connections.

#### 1.3 Event Parser (`internal/nostr/parser.go`)

Must support BOTH tag formats:

```go
type DNSRecord struct {
    Type  string   // A, AAAA, CNAME, TXT, MX, etc.
    Name  string   // "@", "www", "blog", etc.
    TTL   uint32   // seconds
    RData []string // type-specific data
}

// ParseRecordTag parses both 5-element and 11-element formats
func ParseRecordTag(tag []string) (*DNSRecord, error) {
    switch len(tag) {
    case 5:  return parseNewFormat(tag)   // ["record", "TYPE", "name", "TTL", "rdata"]
    case 11: return parseLegacyFormat(tag) // ["record", "TYPE", "name", ..., "ttl"]
    default: return nil, fmt.Errorf("invalid tag length: %d", len(tag))
    }
}
```

**5-element format** (new spec):
```json
["record", "A", "@", "3600", "192.168.1.1"]
["record", "MX", "@", "3600", "10 mail.example.com."]
["record", "SRV", "_sip._tcp", "3600", "10 5 5060 sip.example.com."]
```
Parse: `rdata` is zone-file-style. For simple types (A, AAAA, CNAME) it's a single value. For multi-field types (MX, SRV, SOA) it's space-separated.

**11-element format** (legacy, used by reference implementation):
```json
["record", "A", "@", "192.168.1.1", "", "", "", "", "", "", "3600"]
["record", "MX", "@", "10", "mail.example.com", "", "", "", "", "", "3600"]
["record", "SRV", "_sip._tcp", "10", "5", "5060", "sip.example.com", "", "", "", "3600"]
```
Parse: data in positions 3-9, TTL in position 10. Reference: `parseRecordTag()` in `client.go`.

**Validation** (from reference `validateDNSRecord()`):
- A: must have valid IPv4 in data[0]
- AAAA: must have valid IPv6 in data[0]
- CNAME/NS/PTR: must have target domain
- TXT: must have text content
- MX: must have priority + mail server
- SRV: must have priority + weight + port + target

#### 1.4 npub Handling (`internal/nostr/npub.go`)

From reference `client.go`:
```go
// ExtractNpubFromDomain extracts npub from domain
// "npub1abc.nodns.shop" → "npub1abc"
// "www.npub1abc.nodns.shop" → "npub1abc"
func ExtractNpubFromDomain(domain string) (string, error)

// ConvertNpubToPubkey converts npub (bech32) to hex pubkey
func ConvertNpubToPubkey(npub string) (string, error)
```

Use `github.com/nbd-wtf/go-nostr/nip19` for bech32 encoding/decoding.

#### 1.5 DNS Updater (`internal/dns/updater.go`)

Uses `miekg/dns` to send TSIG-signed DNS UPDATE messages to Knot.

```go
type Updater struct {
    knotAddr string
    tsigKey  string
    tsigAlg  string
    zone     string
    client   *dns.Client
}

// AddRecord sends a DDNS update to add a DNS record
func (u *Updater) AddRecord(fqdn string, ttl uint32, recordType string, rdata []string) error

// DeleteRecord sends a DDNS update to remove records
func (u *Updater) DeleteRecord(fqdn string, recordType string) error
```

The reference implementation's `convertSingleRecord()` shows how to build `dns.RR` for each type:
- A → `dns.A{Hdr, A: ip.To4()}`
- AAAA → `dns.AAAA{Hdr, AAAA: ip}`
- CNAME → `dns.CNAME{Hdr, Target: name}`
- TXT → `dns.TXT{Hdr, Txt: []string{data}}`
- MX → `dns.MX{Hdr, Preference: uint16, Mx: name}`
- SRV → `dns.SRV{Hdr, Priority, Weight, Port, Target}`

For DDNS, we build UPDATE messages instead of response messages:
```go
msg := new(dns.Msg)
msg.SetUpdate("nodns.shop.")
msg.RemoveName([]dns.RR{...}) // delete old
msg.Insert([]dns.RR{...})     // add new
// Sign with TSIG
msg.SetTsig("nodns-bot", dns.HmacSHA256, 300, time.Now().Unix())
```

#### 1.6 FQDN Construction

From the event's pubkey (hex), construct the domain:
```
pubkey hex: b3e4...f7a1
npub: npub1b3e4...f7a1 (63 chars, bech32 encoded)

For the zone nodns.shop:
  Root:     npub1b3e4...f7a1.nodns.shop
  Subdomain: www.npub1b3e4...f7a1.nodns.shop
```

The npub is 63 characters (bech32), which is within the DNS label limit of 63.

The event's `name` field maps to:
- `"@"` or `""` → root of the npub zone: `npub1{hex}.nodns.shop`
- `"www"` → `www.npub1{hex}.nodns.shop`
- `"blog"` → `blog.npub1{hex}.nodns.shop`

#### 1.7 SQLite Store (`internal/store/sqlite.go`)

Minimal schema for Phase 1:
```sql
CREATE TABLE events (
    event_id TEXT PRIMARY KEY,
    npub TEXT NOT NULL,
    pubkey TEXT NOT NULL,
    name TEXT NOT NULL,
    record_type TEXT NOT NULL,
    ttl INTEGER NOT NULL,
    rdata TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    processed_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE TABLE meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

### Phase 1 Event Processing Flow

```
1. Bot starts
   ├─ Load config
   ├─ Open SQLite
   ├─ Test DDNS connection to Knot (TSIG auth)
   └─ Subscribe to relays: {kinds: [11111]}

2. Event arrives
   ├─ Verify signature (go-nostr does this)
   ├─ Check content == ""
   ├─ Parse all "record" tags (both 5-element and 11-element)
   ├─ For each record:
   │   ├─ Validate: type allowed? valid IP? not private?
   │   ├─ Compute FQDN: {name}.npub1{hex}.{zone}
   │   ├─ DDNS DELETE old records for this FQDN+type
   │   ├─ DDNS ADD new record
   │   └─ Store in SQLite
   └─ Log result

3. Knot handles:
   ├─ RCU swap (atomic, lock-free)
   ├─ DNSSEC re-sign (incremental, changed records only)
   └─ NOTIFY puck.nether.net → IXFR
```

### Phase 2: Deletion Handling

Kind 5 events delete previously published events:

```go
// On kind 5 event:
// 1. Extract "e" tag (referenced event ID)
// 2. Look up in SQLite
// 3. Verify deletion publisher == original event publisher
// 4. DDNS DELETE all records from original event
// 5. Mark deleted in SQLite
```

### Phase 3: Custom Domain Leasing

This is the feature the user described: `alice.nodns.shop` instead of `npub1b3e4...f7a1.nodns.shop`.

**How it works:**

A new event type (or convention within kind 11111) that claims a custom name. The bot maintains a name registry that maps `alice.nodns.shop → npub1abc...`.

```
User publishes a "name claim" event:
{
  "kind": 30000,  // or a custom kind
  "pubkey": "abc...",
  "tags": [
    ["d", "alice"],           // the name being claimed
    ["payment", "cashu:..."], // Cashu proof of payment
    ["zone", "nodns.shop"]
  ]
}

Bot receives the claim:
  ├─ Validate: name available? valid format? payment valid?
  ├─ Record: alice.nodns.shop → npub1abc...
  └─ Now when DNS queries arrive for alice.nodns.shop:
      ├─ Bot knows this maps to npub1abc...
      ├─ Fetches kind 11111 events for npub1abc...
      └─ Serves the records for alice.nodns.shop
```

The DDNS flow for custom names:

```
1. User claims "alice.nodns.shop" (via name claim event)
2. User publishes kind 11111 with records for name "alice"
3. Bot:
   ├─ Sees kind 11111 from npub1abc...
   ├─ Looks up: does npub1abc... have a custom name? → "alice"
   ├─ Creates DDNS: alice.nodns.shop A 1.2.3.4
   └─ NOT npub1abc.nodns.shop — the custom name replaces the npub name
```

**Payment via Cashu:**
- Cashu is Nostr-native ecash (Chaumian blinding)
- The user includes a Cashu token as proof of payment
- Bot verifies the token with the Cashu mint
- If valid, the name is leased for a period (e.g., 1 year)
- Renewal requires another payment

**Name registry in SQLite:**
```sql
CREATE TABLE name_claims (
    name TEXT PRIMARY KEY,         -- "alice"
    fqdn TEXT NOT NULL,            -- "alice.nodns.shop"
    npub TEXT NOT NULL,            -- owner's npub
    pubkey TEXT NOT NULL,          -- owner's hex pubkey
    payment_token TEXT,            -- Cashu proof
    claimed_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);
```

### Phase 4: TLS Certificates

The protocol defines kind 30003 for TLS certificate distribution. From `cert_events.md`:
- Events contain PEM-encoded certificates in the `content` field
- The `d` tag identifies the TLD
- The `expiry` tag contains the expiration timestamp

The bot could use these to automate TLS for `nodns.shop` subdomains via ACME (Let's Encrypt) + DNS-01 challenge, but this is a later phase.

## Testing Strategy

### Unit Tests
- `parser_test.go`: Test both 5-element and 11-element tag parsing with all record types
- `npub_test.go`: Test npub extraction from various domain formats
- `records_test.go`: Test DNS record construction (reuse patterns from reference implementation)

### Integration Tests
- Start a local Knot DNS instance on a test port
- Send DDNS updates from bot
- Verify records resolve via `dig @127.0.0.1`

### End-to-End Test
- Publish a kind 11111 event to a test relay
- Bot receives and processes it
- `dig @46.224.104.12 npub1xxx.nodns.shop A` returns the correct IP
- Verify puck.nether.net also serves it via `dig @204.42.254.5`

## Key Differences from Reference Implementation

The reference `nodns-server` is a JIT resolver — it queries Nostr relays on every DNS request. This has several problems:
- 5-second timeout on relay lookups
- Zero caching
- Relays become DNS bottlenecks
- No DNSSEC

Our approach (prefetched bot + Knot DNS) solves all of these:
- DNS queries served from memory (<1ms)
- Knot handles DNSSEC automatically
- Knot handles NOTIFY/AXFR to secondaries
- Knot is battle-tested production software
- Bot is a simple bridge: Nostr → DDNS → Knot

The bot doesn't need to be a DNS server at all. It's a Nostr client that speaks DDNS.

## Migration: What to Reuse from Reference

| From Reference | What to Reuse |
|---|---|
| `client.go`: `ExtractNpubFromDomain()` | npub extraction logic (handles hex subdomain format too) |
| `client.go`: `ConvertNpubToPubkey()` | bech32 → hex conversion |
| `client.go`: `parseRecordTag()` | Legacy 11-element tag parsing (adapt for our struct) |
| `client.go`: `validateDNSRecord()` | Record validation per type |
| `server.go`: `convertSingleRecord()` | Record type → dns.RR construction (adapt for DDNS instead of DNS response) |
| `server.go`: `extractSubdomain()` | Subdomain extraction from full domain |
| `config.go`: relay defaults | Default relay list |

## Build and Deploy

```bash
# Build
cd nodns-bot
go build -o nodns-bot .

# Deploy to VPS
scp nodns-bot root@inr2.cashu.exchange:/opt/nodns-bot/
scp config.toml root@inr2.cashu.exchange:/etc/nodns-bot/config.toml

# Or build on the VPS
ssh root@inr2.cashu.exchange "apt install -y golang-go"
# Then clone and build on VPS
```
