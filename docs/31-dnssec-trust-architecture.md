# 31 — DNSSEC Trust Architecture

> **Status**: ACTIVE. Describes the production trust model for nodns.shop as of 2026-06-10.

## The Three Layers

```
Layer 1: Key Derivation (mathematical link)
  nsec → SLIP-10 → P-256 DNSSEC KSK → signs the zone

Layer 2: Nostr Attestation (protocol link)
  Registrar publishes kind:11111 with dnskey tag → verifiable on Nostr

Layer 3: Standard DNSSEC Chain (internet trust)
  Root → .shop → nodns.shop (DS → KSK → ZSK → records)
```

### Combined Trust Model

```
Nostr identity (npub)
  ↓ (SLIP-10 derivation — anyone can verify mathematically)
DNSKEY for nodns.shop (KSK 15318)
  ↓ (Nostr attestation event — verifiable on relays)
  ↓ (DS record at parent — verifiable via DNSSEC chain)
.shop TLD
  ↓ (DS record at root)
DNS Root (ICANN trust anchor)
```

## The Soft Fork Model

NoDNS enforces stricter DNSSEC rules than standard DNS. This is analogous to a Bitcoin soft fork:

- **Legacy resolvers** (Google, Cloudflare, Quad9) follow standard DNSSEC rules — they validate the DS chain and RRSIG signatures. Everything works normally.
- **NoDNS-aware resolvers** enforce an additional constraint: a DNSKEY must be cryptographically derived from (or attested to by) the registrar's Nostr identity. A DNSKEY that passes standard DNSSEC validation but lacks the Nostr link is rejected by nodns-aware resolvers.

### Why This Matters

A rogue operator could:
1. Generate a new KSK outside of SLIP-10 derivation
2. Submit a new DS at the registrar (they control the Namecheap account)
3. Sign the zone with the rogue key

Standard DNSSEC would accept this — the DS chain is valid. But a nodns-aware resolver would reject it because the new DNSKEY is not derived from the registrar's nsec and has no attestation event.

This is the "soft fork" — legacy resolvers follow the old (less strict) rules, while nodns-aware resolvers enforce stricter rules. The operator can't fool a nodns-aware resolver even with full control of the DNS infrastructure.

### Analogy: Bitcoin Soft Fork

| Bitcoin | NoDNS |
|---|---|
| Old nodes accept blocks under old rules | Legacy resolvers accept DNSKEY under standard DNSSEC |
| New nodes reject blocks that violate new rules | NoDNS-aware resolvers reject DNSKEY without Nostr link |
| Soft fork = tightening rules, not loosening | Same — stricter validation, same chain |
| Old nodes still follow the chain, just less strictly | Legacy resolvers still resolve, just with weaker guarantees |

## Registrar Key Details

| Property | Value |
|---|---|
| Registrar npub | `7effcccb48fc9d091a8cab663a566523c8249d7770d5fd3c31c96a0f2b8db9ed` |
| SLIP-10 P-256 pubkey | `04e30c790326cb8c093329d9e4c01cb3a79120f474df18c9c708418cc0d5540851b3ec104d9888cc9d7a7b1379eddb42a708eada7c403f8f7be7810b749566e39c` |
| Key tag (SLIP-10 KSK) | 15318 |
| DS (digest type 2) | `15318 13 2 15fe3e8c712de06ed097123497938d9185563baf6fecafa5ffe89a322706f580` |

## Dual-KSK State

| Key | Tag | Origin | DS at Registrar | Status |
|---|---|---|---|---|
| Original KSK | 12717 | Knot auto-generated | ✅ Active | Signing |
| SLIP-10 KSK | 15318 | Derived from registrar nsec | ⏳ Pending | Signing |

Both KSKs actively sign the zone. Once DS for 15318 is added at Namecheap (alongside existing 12717 DS), the SLIP-10 KSK will have a full chain of trust. The original KSK can be retired once the SLIP-10 KSK's DS is confirmed propagated.

## Attestation Event

The bot publishes a DNSKEY attestation event at startup:

| Field | Value |
|---|---|
| Event ID | `fd0d8d4399dee87c472c8a5883315cac554bec4c8c5ea77db23f83b2b08ef8cf` |
| Kind | 11111 |
| Signer | Registrar npub |
| Tags | `["dnskey", "nodns.shop", "15318", "13", "<base64 RDATA>"]`, `["dnskey-derivation", "slip10", "Nist256p1 seed"]` |
| Relays | relay.damus.io, nos.lol |

A nodns-aware resolver:
1. Finds the attestation event on relays
2. Verifies the event signature (signed by registrar npub)
3. Extracts the DNSKEY from the event
4. Optionally verifies the SLIP-10 derivation independently
5. Compares with the DNSKEY in DNS — must match

## TXT-as-Event (DNS-as-Relay-Cache)

Every TXT record update also embeds a compact Nostr event as an additional TXT record. This allows nodns-aware resolvers to extract event data from DNS responses without connecting to Nostr.

See [25-censorship-resistance.md](25-censorship-resistance.md) for full details.

## Attack Vectors and Mitigations

### Rogue DS at Registrar

The operator controls the Namecheap account and could submit a DS for a key NOT derived from the registrar nsec.

**Mitigation**: NoDNS-aware resolvers reject DNSKEYs that lack the Nostr link. The rogue DS is accepted by legacy resolvers but rejected by nodns-aware resolvers.

**Limitation**: Users relying solely on legacy resolvers are vulnerable. This is why nodns-aware resolvers (home router, browser extension) are the endgame.

### Compromised nsec

If the registrar nsec is compromised, the attacker can:
1. Derive the SLIP-10 DNSSEC key
2. Sign arbitrary DNS records
3. Publish attestation events for rogue DNSKEYs

**Mitigation**: The nsec is stored in the bot config file on the VPS. Filesystem permissions limit access. NIP-46 remote signing would move the nsec off the server entirely.

**Recovery**: Generate a new nsec, derive new SLIP-10 key, rotate KSK, submit new DS at Namecheap.

### Bot Compromise

An attacker with access to the bot process can publish events signed by the registrar nsec and write arbitrary DNS records.

**Mitigation**: Same as nsec compromise. The bot runs as a dedicated user with limited filesystem access.

## Implementation References

- `nodns-bot-rs/src/dnssec_derivation.rs` — SLIP-10 → P-256 key derivation
- `nodns-bot-rs/src/main.rs` — Key derivation at startup, DNSKEY attestation event publishing
- `nodns-bot-rs/src/event_processor.rs` — `build_compact_event_txt()` for TXT-as-event
- `nodns-bot-rs/src/dns.rs` — `append_record()` for compact TXT embedding
- `docs/12-dnssec-setup.md` — Production DNSSEC deployment details
- `docs/13-nostr-dnssec-derivation.md` — SLIP-10 research and implementation details
