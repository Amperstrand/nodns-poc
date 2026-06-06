# Architecture

## Design Principle: Zero Modifications to Existing DNS Infrastructure

The entire approach is built on one constraint: ARME/DNS.PT should not need to change their existing Knot DNS setup. No plugins, no custom builds, no patches. The NoDNS layer is a separate daemon that feeds zone data into Knot through standard DNS protocols.

## System Diagram

```
                          ┌──────────────────────────────────────────┐
                          │              VPS / Server                │
                          │                                          │
Nostr Relays              │  ┌─────────────┐     ┌───────────────┐  │
 wss://relay.damus.io     │  │  nodns-bot  │     │   Knot DNS    │  │
 wss://nos.lol        ────┼──│  (Go daemon) │────▶│               │  │
 wss://relay.nostr.band   │  │             │DDNS │  nostr.cv zone │──┼──▶ Internet
 wss://nostr.wine         │  │ - Subscribe │     │  DNSSEC signed │  │
                          │  │ - Validate  │     │  RCU (lockless)│  │
                          │  │ - Parse     │     └───────┬───────┘  │
                          │  │ - knsupdate │             │NOTIFY    │
                          │  └─────────────┘             │          │
                          └──────────────────────────────┼──────────┘
                                                        │
                                                        ▼
                                              ┌──────────────────┐
                                              │   Secondaries    │
                                              │ (via AXFR/IXFR)  │
                                              │ - DNS.PT servers │
                                              │ - AFNIC          │
                                              │ - ISC            │
                                              │ - Netnod anycast │
                                              └──────────────────┘
```

## Architecture Models Evaluated

### Model A: Live Resolution (Rejected)
Every DNS query triggers a Nostr relay lookup. This is what the existing `nodns-server` does.

**Rejected because**:
- 5-second timeout on relay lookups — unacceptable for DNS
- Zero caching — every query is a network request
- Relays are single points of failure for DNS resolution
- DNS resolvers expect <100ms responses

### Model B: Nostr Listener Bot + DNS Server (Selected)
A separate daemon subscribes to Nostr relays, parses events, and pushes DNS records to an authoritative server via DDNS.

**Selected because**:
- Zero modifications to existing DNS infrastructure
- DNS queries served from local zone data — <1ms latency
- Standard DDNS protocol — works with Knot, BIND, PowerDNS, any authoritative server
- Knot's RCU mechanism means zero query interruption during updates
- Nostr events pushed via subscriptions, not pulled per-query

### Model C: Centralized API (Not evaluated)
DNS control panel API that reads from Nostr. Essentially a traditional DNS management layer with Nostr as input. Doesn't leverage the protocol's decentralized nature.

### Model D: Hybrid (Future consideration)
Live resolution for cold records + prefetched for hot records. Adds complexity without clear benefit for a ccTLD use case where all records fit in memory.

## Caching Strategies Evaluated

### JIT (Just-In-Time) — What nodns-server does
- On query → lookup relay → cache result → return
- First query is slow (5s timeout)
- Existing implementation has zero caching

### Cached JIT
- On query → check cache → if miss, lookup relay → cache → return
- Better for hot domains, cold domains still slow
- Cache invalidation is complex

### Prefetched via Subscription (Selected)
- Bot subscribes to all kind 11111 events for the zone
- Events arrive within seconds of publication
- Bot pushes to Knot via DDNS immediately
- All DNS queries served from local zone data
- Latency: <1ms for any query

**This is the clear winner for a ccTLD operator.** All queries are fast. The only latency is the Nostr → bot → DDNS propagation (1-3 seconds from event publication to global DNS resolution).

## Knot DNS Hot Reload

### Why Knot DNS

DNS.PT uses Knot DNS across all their authoritative servers. Knot DNS is specifically designed for high-performance authoritative DNS serving with excellent dynamic update support.

### Read-Copy-Update (RCU) — The Key Mechanism

Knot uses RCU (same technique as the Linux kernel) for zone updates:

1. **Read phase**: Queries continue hitting the current zone — no locking
2. **Copy phase**: New zone data built in separate memory
3. **Update phase**: Atomic pointer swap — old zone replaced with new zone
4. **Reclaim phase**: Old zone memory freed after all in-flight queries complete

**Result**: Zero queries are ever blocked or dropped during a zone update. No locks. No read pauses.

From the Knot docs:
> "Knot DNS employs the Read-Copy-Update mechanism instead of locking and thus requires twice the amount of memory for the duration of incoming transfers."

### DDNS (Dynamic DNS Updates) — The Update Mechanism

The bot sends DNS UPDATE messages (RFC 2136) to Knot. This is the standard protocol for dynamic zone updates:

1. Bot sends TSIG-signed UPDATE message to Knot on `127.0.0.1`
2. Knot validates the TSIG key
3. Knot applies the update atomically
4. Knot re-signs only the affected records (incremental DNSSEC)
5. Knot sends NOTIFY to configured secondaries
6. Secondaries pull IXFR (incremental transfer)

### Why DDNS Over Zone File Writes

| Factor | Zone File + Reload | DDNS |
|---|---|---|
| Update granularity | Full zone | Individual records |
| DNSSEC re-signing | Full zone | Changed records only |
| File I/O | Write + parse | None (in-memory) |
| Race conditions | Possible (file write vs read) | None (atomic protocol) |
| Server compatibility | Server-specific paths | Any RFC 2136 server |
| Latency | File write + reload (~100ms) | Protocol message (~1ms) |

### DNSSEC After DDNS

Knot automatically re-signs after every DDNS update:

> "The signing is initiated on the following occasions: Start of the server, Zone reload, Reaching signature refresh period, Key set changed, NSEC3 salt changed, **Received DDNS update**, Forced re-sign"

Steps after DDNS:
1. Update DNSKEY records if needed
2. Fix NSEC/NSEC3 chain
3. Remove expired signatures
4. Create new signatures for changed records only
5. Update and re-sign SOA record

For a single A record update: sub-millisecond signing overhead.

### Propagation Timeline

```
T+0s     User publishes kind 11111 to Nostr relay
T+1-3s   Bot receives event from relay subscription
T+3s     Bot validates and sends DDNS update
T+3.001s Knot processes update, re-signs, serves new record
T+3.1s   Knot sends NOTIFY to secondaries
T+3.2s   Secondaries pull IXFR
T+3.5s   New record resolvable from all nameservers
T+60s    Negative cache expires (if domain was previously NXDOMAIN)
```

Compare to traditional DNS: change zone file → reload → NOTIFY → IXFR → wait for TTL. Same mechanism, different input source (Nostr event vs human edit).

## TSIG Authentication

DDNS updates are authenticated via TSIG (Transaction SIGnature, RFC 2845):

- Shared secret between bot and Knot
- HMAC-SHA256 (or stronger)
- Only localhost connections allowed
- Knot validates every UPDATE message before applying

This prevents unauthorized zone modifications — only the bot (with the correct TSIG key, connecting from localhost) can update records.

## Zone Transfer to Secondaries

Knot supports two transfer modes:

- **AXFR** (full zone transfer) — used for initial sync or major changes
- **IXFR** (incremental transfer) — used for DDNS updates, only changed records transferred

For the demo (single VPS): no secondaries needed.
For production: Knot NOTIFYs secondaries, they pull IXFR automatically. This is standard DNS — no special configuration needed for NoDNS.
