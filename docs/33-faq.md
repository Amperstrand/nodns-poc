# 33 — NoDNS FAQ

> **Status**: ACTIVE. Common questions, gotchas, and misconceptions about NoDNS.

## General

### What is NoDNS?

NoDNS is a protocol where your Nostr private key controls a DNS subdomain. You publish kind 11111 events to Nostr relays, and a bot picks them up and creates real DNS records. No API keys, no accounts, no registrar login — your nsec IS your credential.

### How fast do records propagate?

~3-5 seconds. The bot subscribes to relays in real-time and pushes updates directly to the authoritative nameserver (Knot DNS) via RFC 2136 DDNS. No zone file reload needed.

### Is this production?

No. The protocol is an experimental draft. `nodns.shop` is a live demo.

---

## Keys and Names

### Is my npub always a valid DNS label?

Yes, always. All npubs derived from 32-byte x-only Nostr public keys are exactly 63 characters — well within DNS's 63-character label limit (RFC 1035). You do not need to check npub length or generate keys until you get a "short" one.

**The math**: 32 bytes → 256 bits → 52 five-bit bech32 groups → `"npub"` (4) + `"1"` (1) + 52 data + 6 checksum = **63 characters. Every time.**

### Can I use any Nostr keypair?

Yes. Any valid secp256k1 keypair works. Generate one with your Nostr client, or create an ephemeral one on the web UI at [nodns.shop](https://nodns.shop).

### What's the difference between npub-based names and delegated names?

| Feature | npub-based (`npub1xxx.nodns.shop`) | Delegated (`alice.nodns.shop`) |
|---|---|---|
| How you get it | Automatic — your npub IS the subdomain | Registrar delegates via kind 11111 event |
| Payment | Free | May require Cashu payment (operator sets price) |
| DNS label | `npub1f7hnsxh5vk...` | `alice` — human-readable |
| Authority | Cryptographic (your nsec) | Delegation event (signed by registrar) |
| Expiry | Never | Set by registrar (renewal required) |

---

## Protocol (Kind 11111 Events)

### What's the record tag format?

Two formats are accepted:

**New format (recommended)** — 5 elements:
```
["record", "TYPE", "name", "TTL", "rdata"]
```

**Legacy format** — 11 elements:
```
["record", "TYPE", "name", "pos1", "pos2", "pos3", "pos4", "pos5", "pos6", "pos7", "ttl"]
```
Positions 3-9 are joined (space-separated) to form rdata.

Use the 5-element format. The 11-element format exists for backward compatibility.

### How do I set an apex record?

Use `""` or `"@"` as the name field:
```
["record", "A", "", "3600", "1.2.3.4"]
```
This creates a record at `npub1xxx.nodns.shop` (the apex of your subdomain).

### How do I set a subdomain record?

Put the subdomain name in the name field:
```
["record", "A", "www", "3600", "1.2.3.4"]
```
This creates `www.npub1xxx.nodns.shop`.

### How do I delete a record?

Use a delete tag in a kind 11111 event:
```
["delete", "A", ""]
```
This deletes the apex A record under your subdomain.

### Can I put multiple records in one event?

Yes. Include multiple `["record", ...]` tags. They're processed together. You can also mix records and deletes in the same event — deletes run after records, enabling atomic replace semantics.

### What record types are supported?

By default: **A, AAAA, CNAME, TXT, MX**. The operator may have enabled additional types (SRV, NS, PTR) in their config.

### Can I use CNAME at the apex alongside other records?

No. Per RFC 1912, CNAME cannot coexist with other record types at the same name. The parser rejects events containing a CNAME and another type at the same name.

---

## Validation Rules

### What names are allowed?

Lowercase alphanumeric, hyphens, and underscores. Max 63 chars. No leading or trailing hyphens. `@` and `""` are reserved for the apex.

Examples:
- ✅ `www`, `api-v2`, `_acme-challenge`, `sub123`
- ❌ `WWW` (uppercase), `-bad` (leading hyphen), `a.b` (dots)

### What IPs are blocked?

By default, private/reserved IP ranges are blocked to prevent abuse:
- IPv4: `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `127.0.0.0/8`, `169.254.0.0/16`, `0.0.0.0/8`, `100.64.0.0/10`
- IPv6: `fc00::/7`, `fe80::/10`, `::1/128`

The operator can disable this with `block_private_ip = false`.

### What TXT records are blocked?

Anti-spoofing protections block:
- `_dmarc` — DMARC spoofing prevention
- `_domainkey*` — DKIM spoofing prevention
- Apex TXT starting with `v=spf1` — SPF spoofing prevention

These prevent users from impersonating the zone's email infrastructure.

### What's the TTL behavior?

TTL defaults to 3600 (1 hour) if empty or 0. You can set any positive integer. There's currently no minimum enforcement (a future improvement may enforce a floor).

---

## Payments

### Do I need to pay?

npub-based names are free. Delegated custom names may require Cashu payment — the operator sets pricing per zone. Check the zone's pricing via the API:
```
GET /api/zones/{zone}/pricing
```

### How does Cashu payment work?

Include a `["cashu", TOKEN, MINT_URL, AMOUNT]` tag in your kind 11111 event alongside your claim or record tags. The bot verifies the token with the mint before processing.

### What about Lightning/zaps?

NIP-57 zaps are explicitly NOT suitable as payment proof — the NIP-57 spec states that zap receipts are not proof of payment. Use Cashu.

---

## TLS / ACME

### How do I get HTTPS on my subdomain?

Two options:

**Option 1: Caddy/Let's Encrypt HTTP-01** (if you control the server behind the IP)
Point your subdomain to your server's IP, put Caddy in front, and Caddy auto-provisions TLS via HTTP-01 challenge. No NoDNS involvement needed beyond the A record.

**Option 2: NoDNS ACME API** (DNS-01 challenge)
The bot can automate DNS-01 challenges via its built-in ACME service:
1. Publish your DNS record (A/AAAA pointing to your server)
2. Call `POST /api/acme/order` with your domain
3. The bot creates the `_acme-challenge` TXT record automatically
4. Poll `GET /api/acme/order/{id}` for the certificate

### What CAs are supported?

Let's Encrypt (staging and production) and ZeroSSL.

---

## Architecture

### Does the bot run one connection per relay?

No. The bot uses nostr-sdk's shared relay pool — a single `Client` that manages all relay connections internally. Connection management, reconnection, and deduplication are handled by the SDK.

### What happens when the same event arrives from multiple relays?

The event is processed multiple times, but SQLite's `INSERT OR REPLACE` makes it idempotent — the same data is written, no corruption. Metrics may be slightly inflated.

### How does DNSSEC work?

The zone is signed with ECDSAP256SHA256 (algorithm 13). The registrar's DNSSEC key is derived from their Nostr nsec via SLIP-0010 (P-256). The bot publishes a DNSKEY attestation event (kind 11111 with `dnskey` tags) at startup.

### Is NIP-09 (kind 5) event deletion supported?

No. Only the custom `["delete", TYPE, NAME]` tag in kind 11111 events is supported for record deletion. Standard Nostr kind-5 deletion events are not processed.

---

## Delegated Names

### How does a delegated name work?

1. **Registrar creates delegation**: Publishes a kind 11111 event with `["delegation", "alice.nodns.shop", "npub1xxx...", valid_from, valid_until, renew_by]`
2. **User claims the name**: Publishes a kind 11111 event with `["claim", "alice", "nodns.shop", valid_until]` + Cashu payment tag
3. **User publishes DNS records**: Standard kind 11111 record events. The bot detects the delegation and routes records to `alice.nodns.shop` instead of `npub1xxx.nodns.shop`
4. **Renewal**: Before expiry, user publishes `["renewal", "alice", "nodns.shop", new_valid_until]` with payment

### What happens when a delegation expires?

30-day grace period (configurable). During grace, DNS records stay active but only renewals are accepted. After grace, the delegation is marked expired and the name becomes available for re-registration.

---

## Common Mistakes

### ❌ "I need to check if my npub fits in 63 chars"

All npubs are exactly 63 characters. No check needed.

### ❌ "I should set TTL=0 for fast propagation"

TTL=0 defaults to 3600. Even if you could set a very low TTL, it would cause excessive DNS queries. The bot pushes updates to the authoritative nameserver directly — propagation is already near-instant regardless of TTL.

### ❌ "I need to include the zone in my event tags"

No. Events are zone-agnostic. The bot maps events to zones based on its configuration. Publish the same event and it works across all zones running NoDNS bots.

### ❌ "I can use NIP-09 deletion events to remove DNS records"

No. Only `["delete", TYPE, NAME]` tags in kind 11111 events work for DNS record deletion.

### ❌ "The bot opens one WebSocket per relay"

No. It uses nostr-sdk's shared `Client` with a relay pool that manages all connections internally.

### ❌ "Cashu tokens are like API keys"

No. Cashu tokens are one-time-use bearer ecash. Each token can only be spent once. Think of them as digital cash, not credentials.

### ❌ "I need an account to use NoDNS"

No. Your Nostr keypair IS your account. Generate one with any Nostr client, or use the ephemeral key generator on the web UI.

---

## Backwards-Compatible APIs

### Can I use my router's built-in DDNS with NoDNS?

Yes. NoDNS implements the DynDNS v2 protocol (`/nic/update`). Configure your router or ddclient with:
- Server: `nodns.shop`
- Username: your npub
- Password: your nsec

See [34-backwards-compatible-apis.md](34-backwards-compatible-apis.md) for full setup instructions.

### Can I get Let's Encrypt certificates?

Yes. NoDNS implements the acme-dns protocol for DNS-01 challenges. Use the certbot manual hook in `docs/examples/certbot-dns-hook.sh`.

### Can I use nsupdate?

Yes. NoDNS runs an RFC 2136 DNS UPDATE server (UDP, configurable port). See `docs/examples/nsupdate-example.sh`.

### Isn't sending my nsec to a server insecure?

It's the same threat model as every DynDNS provider. Namecheap, Cloudflare, GoDaddy — they all require a password or API key stored in your router. If someone compromises your router, they can update your DNS records regardless of provider.

The difference with NoDNS: **you can run the gateway yourself**. The bot is open source. Run your own instance, configure your own TLS, and never send your nsec to a third party. Self-hosting equivalent infrastructure with traditional DynDNS providers is far more complex.
