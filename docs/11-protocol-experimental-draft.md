# NoDNS Protocol Specification (EXPERIMENTAL DRAFT)

> **Status**: EXPERIMENTAL DRAFT. No version number. Kind numbers chosen arbitrarily. Not discussed with Nostr community.
> This is a proof-of-concept protocol for Nostr-native DNS management. Nothing here is production.
> See also: [Arjen's nodns-nameserver](https://gitworkshop.dev/npub1hw6amg8p24ne08c9gdq8hhpqx0t0pwanpae9z25crn7m9uy7yarse465gr/nos.lol/nodns-nameserver) — the `$npub.nostr` reference implementation.

## Overview

NoDNS uses Nostr events to manage DNS records. Three operations are supported:

1. **DNS Record Update** — publish/modify DNS records for a domain you control
2. **Domain Delegation** — a registrar assigns a domain name to a Nostr pubkey
3. **Payment Proof** — attach proof of payment (Cashu or Zap) to an event

All operations use **kind 11111** events. The event type is determined by the tags present.

## Zone-Agnostic Events

The core design principle: **events contain no zone information**. The wire format is the same regardless of which zone (`.nostr`, `.nodns.shop`, or any other) the records end up in.

A kind 11111 record tag:
```
["record", "A", "", "1.2.3.4", "", "", "", "", "", "", "3600"]
```

Contains no zone identifier. Zone assignment is an **infrastructure-layer concern**:

- A bot configured for `.nostr` writes records to the `.nostr` zone
- A bot configured for `.nodns.shop` writes records to the `.nodns.shop` zone
- A bot configured for both writes the same records to both zones
- The same event produces identical records across all zones

This means:
- **Users publish once** — they don't need to know which zones exist
- **Zones are additive** — adding `.nostr` doesn't change the protocol
- **The domain is not the protocol** — `nodns.shop` is a temporary convenience. If lost, the same events resolve under `.nostr` or any zone running a NoDNS bot
- **Multiple operators** — anyone can run a bot for their own zone and process the same events

### The `.nostr` TLD

`.nostr` is a special-use TLD reserved for NoDNS's purest consensus rule: `$npub.nostr`.

**Rules:**
- Only `$npub.nostr` is supported — your public key is your subdomain
- `$string.nostr` returns **NXDOMAIN** — there is no consensus mechanism for string ownership under `.nostr`
- Subdomains work: `www.{npub}.nostr`, `mail.{npub}.nostr` etc.
- Resolution via DoH endpoint (e.g. `https://dns.nodns.shop/dns-query`) or local resolver

**Why no `$string.nostr`?**
Naming requires consensus — who gets to own `alice.nostr`? Under `$npub.nostr`, the answer is purely mathematical: nobody can forge a signature for a public key they don't control. For string names, you need a social or economic mechanism (delegation, proof-of-burn, web-of-trust, etc.), and those mechanisms don't belong under `.nostr` because they'd require an operator or authority figure. String names belong under parent domains where an operator has opted in.

**Future options for `$string.nostr`:**
- Proof-of-burn (mining-style economic commitment)
- Web-of-trust (subjective resolution)
- These are documented in the consensus models but **not decided** — `.nostr` stays npub-only until consensus emerges

**Resolution methods:**
1. **DoH endpoint** — `https://dns.nodns.shop/dns-query` — users point their DNS at this endpoint for one-click `.nostr` resolution
2. **Local resolver** — Arjen's `nodns-nameserver` — subscribes to Nostr relays directly
3. **Bot-mirrored DNS** — same records also appear under `.nodns.shop` for standard DNS resolution

## Kind 11111 Event Types

### Type 1: DNS Record Update

The most common event. A pubkey publishes DNS records for domains they control.

### Type 1b: DNS Record Delete

Removes a specific DNS record. The sender must be the same pubkey that created the record (or the delegated owner).

**Authority rules:**
- `npub1*.zone` — always allowed (free names, bound to your pubkey). Anti-spam fee may apply.
- `alice.zone` — only allowed if a delegation event exists assigning `alice.zone` to your pubkey, and the delegation has not expired.

**Tags:**

```
["record", TYPE, NAME, RDATA, "", "", "", "", "", "", TTL]
```

- Position 0: literal string `"record"`
- Position 1: DNS record type (`"A"`, `"AAAA"`, `"CNAME"`, `"TXT"`, `"MX"`)
- Position 2: Subdomain name (`""` or `"@"` for apex, `"www"` for www.subdomain)
- Position 3: Record data (IP address, hostname, text content)
- Positions 4-9: Reserved (empty strings)
- Position 10: TTL in seconds (string representation of uint32)

**Example:**

```json
{
  "kind": 11111,
  "content": "DNS record update",
  "tags": [
    ["record", "A", "", "193.99.144.80", "", "", "", "", "", "", "3600"],
    ["record", "TXT", "", "v=spf1 include:_spf.google.com ~all", "", "", "", "", "", "", "3600"]
  ]
}
```

This creates:
- `npub1ykal2...pa3dl.nodns.shop` → `A 193.99.144.80`
- `npub1ykal2...pa3dl.nodns.shop` → `TXT v=spf1 include:_spf.google.com ~all`

With a subdomain:
```json
["record", "CNAME", "www", "npub1ykal2...pa3dl.nodns.shop", "", "", "", "", "", "", "3600"]
```
Creates: `www.npub1ykal2...pa3dl.nodns.shop` → `CNAME npub1ykal2...pa3dl.nodns.shop`

### Type 1b: DNS Record Delete

Removes a specific DNS record. Only the pubkey that owns the subdomain (or the delegated owner) can delete records.

**Tags:**

```
["delete", TYPE, NAME]
```

- Position 0: literal string `"delete"`
- Position 1: DNS record type to delete (`"A"`, `"AAAA"`, `"CNAME"`, `"TXT"`, `"MX"`)
- Position 2: Subdomain name (`""` or `"@"` for apex, `"www"` for www.subdomain)

**Authority rules:** Same as Type 1 — the sender must own the subdomain via npub binding or active delegation.

**Example:**

```json
{
  "kind": 11111,
  "content": "DNS record delete",
  "tags": [
    ["delete", "A", ""],
    ["delete", "TXT", "www"]
  ]
}
```

This removes:
- `A` record at `npub1ykal2...pa3dl.nodns.shop` (apex)
- `TXT` record at `www.npub1ykal2...pa3dl.nodns.shop`

Multiple delete tags may be included in a single event. Deletes are processed after records in the same event (if both are present), allowing atomic replace semantics.

### Type 2: Domain Delegation

A registrar (or anyone with authority over a zone) delegates a domain name to a Nostr pubkey.

**This is irrevocable.** Once signed, the delegation cannot be cancelled by the registrar. The only way it ends is expiration (`valid_until`). This is the core consensus rule: the signed Nostr event IS the authority, not the registrar's DNS server.

**Tags:**

```
["delegation", DOMAIN, NPUB, VALID_FROM, VALID_UNTIL, RENEW_BY]
```

- Position 0: literal string `"delegation"`
- Position 1: Domain being delegated (e.g., `"alice.cv"`, `"mystore.nodns.shop"`)
- Position 2: Nostr pubkey (npub) receiving the delegation
- Position 3: Unix timestamp (string) — delegation starts
- Position 4: Unix timestamp (string) — delegation expires
- Position 5: Unix timestamp (string) — renewal deadline (must renew before this to keep priority)

**Example — registrar delegates `alice.cv`:**

```json
{
  "kind": 11111,
  "pubkey": "<registrar-nostr-pubkey-hex>",
  "content": "Domain delegation: alice.cv → npub1ykal2...pa3dl",
  "tags": [
    ["delegation", "alice.cv", "npub1ykal28...pa3dl", "1749168000", "1780704000", "1778025600"]
  ]
}
```

This says: "I, the .cv registrar, delegate `alice.cv` to `npub1ykal28...pa3dl` from 2025-06-06 to 2026-06-06. Renewal deadline: 2026-05-31."

**Authority chain:**
1. Zone operator publishes their Nostr pubkey via DNS TXT record: `nodns-authority._zone.nodns.shop TXT "nostr:npub1xxx..."` (or via config)
2. Only that pubkey can sign delegation events for that zone
3. The delegated pubkey can then publish DNS records (Type 1) for `alice.cv`
4. Even if the zone operator removes the DNS records, the Nostr event remains valid — any NoDNS-compliant resolver honors it

**Delegation validation rules:**
- Must be signed by the zone's authorized registrar pubkey
- `VALID_FROM` must be ≤ current time
- `VALID_UNTIL` must be > current time
- Domain must be within the signer's zone authority
- One domain → one active delegation (latest valid event wins)

### Type 3: Registrar Key Publication

A zone operator publishes the Nostr pubkey that has authority to sign delegations for their zone.

**Tags:**

```
["registrar", ZONE, PUBKEY_HEX]
```

- Position 0: literal string `"registrar"`
- Position 1: Zone name (e.g., `"cv"`, `"nodns.shop"`)
- Position 2: Nostr pubkey (hex) authorized to sign delegations

**Example:**

```json
{
  "kind": 11111,
  "content": "Registrar key for nodns.shop",
  "tags": [
    ["registrar", "nodns.shop", "a1b2c3d4...hex"]
  ]
}
```

This can also be published as a DNS TXT record for bootstrapping:
```
nodns-registrar.nodns.shop TXT "nostr:npub1xxx..."
```

## Payment Integration

### Cashu Payment Tag

Attach a Cashu token as proof of payment for anti-spam or registration fees.

**Tags:**

```
["cashu", TOKEN, MINT_URL, AMOUNT]
```

- Position 0: literal string `"cashu"`
- Position 1: Cashu token string (starts with `cashuA...`)
- Position 2: Mint URL that issued the token
- Position 3: Amount in sats (string)

**Example:**

```json
{
  "kind": 11111,
  "content": "DNS update with payment",
  "tags": [
    ["record", "A", "", "193.99.144.80", "", "", "", "", "", "", "3600"],
    ["cashu", "cashuAeyJwcm9vZnMiOlt...", "https://mint.example.com", "250"]
  ]
}
```

### NIP-57 Zap Payment Tag

Reference a zap receipt as proof of payment.

**Tags:**

```
["zap", ZAP_RECEIPT_EVENT_ID, AMOUNT]
```

- Position 0: literal string `"zap"`
- Position 1: Event ID of the kind 9735 zap receipt
- Position 2: Amount in sats (string)

**Example:**

```json
["zap", "abc123...eventid", "250"]
```

## Payment Policy

### Free names (npub1*.zone)

| Action | Fee | Notes |
|--------|-----|-------|
| Create new record | 250 sats | Per record. Anti-spam measure. |
| Update existing record | 0 sats | Overwrite existing name+type, no fee |
| Delete record | 0 sats | Removing records is free |

### Custom names (alice.cv)

| Action | Fee | Notes |
|--------|-----|-------|
| Register (1 year) | ~$10-15 equivalent in sats | Registrar receives payment, signs delegation |
| Renew | ~$10-15 equivalent in sats | Must renew before `renew_by` deadline |
| Create DNS record | 250 sats | Same anti-spam as free names |
| Update DNS record | 0 sats | Free once record exists |
| Transfer | Requires new delegation | Old delegation revoked by expiry, new one signed |

## Event Processing Rules

### For Zone Operators (Bot Behavior)

1. **Receive kind 11111 event**
2. **Classify by tags**:
   - Has `"delegation"` tag → Type 2 (Domain Delegation)
   - Has `"registrar"` tag → Type 3 (Registrar Key Publication)
   - Has `"record"` tags → Type 1 (DNS Record Update)
3. **For Type 1 (DNS Record Update)**:
   a. Determine FQDN: `npub1*.zone` or `customname.zone`
   b. For `npub1*.zone` names:
      - Verify payment tag present if creating new record
      - Verify Cashu token is valid and unspent (or zap receipt exists)
      - Apply rate limits
      - Push DDNS update
   c. For custom names:
      - Find active delegation event for this domain → this npub
      - Verify delegation is valid (not expired, signed by registrar)
      - Apply same payment rules as npub names
      - Push DDNS update
4. **For Type 2 (Domain Delegation)**:
   a. Verify signer is the authorized registrar for the zone
   b. Verify domain is within the zone
   c. Store delegation in database
   d. Do NOT push DNS records yet (that's a separate Type 1 event)
5. **For Type 3 (Registrar Key Publication)**:
   a. Verify signer matches bootstrap config or previous registrar key
   b. Store as active registrar for the zone

### Conflict Resolution

- **Same npub, same record type, same name**: Latest event wins (by `created_at`)
- **Multiple delegations for same domain**: Latest valid (non-expired) delegation wins
- **DNS update vs delegation mismatch**: DNS update is rejected if no valid delegation exists

### Irrevocability

The core consensus rule: **a signed delegation event cannot be revoked**. Once a registrar signs `alice.cv → npub1xxx` for a time period, it stands. If the registrar removes the DNS records from their server, NoDNS resolvers continue to honor the Nostr event.

This means:
- Zone operators should only sign delegations they intend to honor
- Disputes are resolved by the delegation expiry mechanism
- NoDNS resolvers trust the Nostr event layer, not the DNS layer

## Bootstrap Flow

1. Zone operator generates a Nostr keypair
2. Zone operator publishes registrar key:
   - Via DNS TXT: `nodns-registrar.zone TXT "nostr:npub1xxx"`
   - Via Nostr event (Type 3)
   - Via bot config file (static)
3. Bot uses this key to verify delegation events
4. Users can register custom names by:
   a. Paying the registrar (off-band or via Cashu/zap)
   b. Registrar signs delegation event (Type 2)
   c. User publishes DNS records (Type 1) referencing their delegated domain

## DNS Resolver Behavior (Future)

A NoDNS-compliant resolver:
1. Receives a query for `alice.cv`
2. Checks for active delegation events for `alice.cv`
3. Checks for DNS record events from the delegated npub
4. Returns the DNS records from Nostr, regardless of what the traditional DNS hierarchy says
5. Falls back to traditional DNS if no Nostr events exist

This creates a dual-layer system: traditional DNS for normal resolution, Nostr as an authoritative overlay that cannot be censored.

## Wire Format Summary

```
kind: 11111

Record tag:        ["record",    TYPE, NAME, RDATA, "", "", "", "", "", "", TTL]
Delete tag:        ["delete",    TYPE, NAME]
Delegation tag:    ["delegation", DOMAIN, NPUB, VALID_FROM, VALID_UNTIL, RENEW_BY]
Registrar tag:     ["registrar", ZONE, PUBKEY_HEX]
Cashu payment tag: ["cashu",     TOKEN, MINT_URL, AMOUNT]
Zap payment tag:   ["zap",       ZAP_RECEIPT_EVENT_ID, AMOUNT]
```

All timestamps are Unix epoch strings. All amounts are in satoshis as strings.

**Note:** Record and delete tags contain no zone identifier. For `$npub` registrations, zone assignment is purely infrastructure — the bot decides which zone it governs, and the same event is portable across zones. For `$string` registrations, zone awareness lives in the claim and delegation tags (`["claim", NAME, ZONE, ...]`, `["delegation", DOMAIN, ...]`), which explicitly reference the zone to establish ownership.
