# 05 — ARME Delegation Instructions

> **Status**: ARCHIVED. Historical — nostr.cv delegation instructions. Superseded by nodns.shop.

This document contains everything ARME needs to do to delegate `nostr.cv` for the NoDNS demo. It is intentionally minimal — one NS record addition, nothing else.

## What is NoDNS?

NoDNS is a protocol that resolves DNS records from Nostr events. Users publish cryptographically-signed events to Nostr relays, and a nameserver reads these events and serves them as standard DNS responses.

For this demo:
- Users publish their desired DNS records as Nostr events (kind 11111)
- A bot reads these events and pushes them to an authoritative DNS server via standard DDNS updates
- The DNS server (Knot DNS) serves the records normally
- No changes to ARME's existing infrastructure

## What ARME Needs to Do

Add **two NS records** to the `.cv` zone file on `ns.dns.cv`:

```
; Delegate nostr.cv to the NoDNS demo server
nostr.cv.    IN  NS  ns1.your-server.com.
nostr.cv.    IN  NS  ns2.your-server.com.
```

Replace `ns1.your-server.com` and `ns2.your-server.com` with the actual nameserver hostnames we provide.

**That's it.** No other changes needed.

If the nameserver hostnames are within `nostr.cv` itself (unlikely), glue A records are also needed:

```
; Glue records (only if NS names are under nostr.cv)
ns1.nostr.cv.    IN  A    203.0.113.10
ns2.nostr.cv.    IN  A    203.0.113.11
```

## What ARME Does NOT Need to Do

- Install any software
- Change any configuration
- Modify their DNS infrastructure
- Accept DDNS updates
- Run any Nostr software
- Sign anything
- Open any firewall ports

The entire NoDNS system runs on our infrastructure. ARME's role is purely to delegate the subdomain via standard DNS.

## Verification

After ARME adds the NS records, we can verify from any machine:

```bash
# Check delegation exists
dig nostr.cv NS

# Expected output:
# nostr.cv.    IN  NS  ns1.your-server.com.
# nostr.cv.    IN  NS  ns2.your-server.com.

# Check that the delegated server responds
dig @ns1.your-server.com nostr.cv SOA

# Test a Nostr-resolved domain
dig npub1b3e4f7a1.nostr.cv A
```

## Security Considerations for ARME

### What ARME is responsible for
- The NS delegation itself (standard DNS operation)
- Ensuring the NS records point to legitimate nameservers

### What ARME is NOT responsible for
- Content served by domains under `nostr.cv`
- DNS records published by Nostr users
- Validating or moderating Nostr events
- Operating the NoDNS infrastructure

### Abuse handling
See [07-abuse-philosophy.md](07-abuse-philosophy.md). In short: NoDNS records are mirrors of data already publicly published on Nostr relays. The DNS layer does not create new content or new abuse vectors. The npub (cryptographic public key) provides built-in accountability — every record is traceable to its publisher.

### Rollback
If ARME ever wants to revoke the delegation, simply remove the two NS records from the `.cv` zone. All `nostr.cv` domains immediately stop resolving globally. No coordination needed with us.

## Questions for ARME

Before proceeding, these are worth discussing:

### 1. Who runs the demo?
- **Option A**: We run on our VPS. ARME observes and evaluates. (Recommended for demo phase)
- **Option B**: ARME provides a server. We configure and operate it. (More control for ARME)
- **Option C**: ARME runs everything themselves. We provide documentation and support.

### 2. Secondary DNS
For the demo, one VPS with two IP addresses is sufficient. For production:
- ARME's existing anycast infrastructure could serve as secondaries
- Knot NOTIFY + IXFR is standard — their existing Knot servers already support this
- No special configuration needed

### 3. DNSSEC timeline
`.cv` has DNSKEY published but no DS in root zone yet (pending ICANN DNSSEC Roadshop).
- For the demo: DNSSEC works locally (Knot signs the zone), but validating resolvers treat `nostr.cv` as unsigned since there's no chain of trust to root
- For production: ARME needs the root DS published. After that, `nostr.cv` DS can be added to the `.cv` zone and the full chain validates
- **This is not blocking for the demo.** Records resolve and serve perfectly without global DNSSEC validation.

### 4. Future: Full .cv integration
After the demo proves the concept, the path to native `.cv` integration is:
1. Install nodns-bot on `ns.dns.cv` (same server as the primary)
2. Configure Knot to accept DDNS from localhost for `.cv` zone
3. Bot subscribes to Nostr and sends DDNS for `npub1*.cv` records
4. Knot re-signs and NOTIFYs secondaries as normal
5. No new infrastructure — the bot is a single Go binary

### 5. Premium names (Phase 2)
The demo starts with `npub1{hex}.cv` names (free, tied to cryptographic identity). Future options:
- `alice.cv` — human-readable names, paid via Cashu (Nostr-native ecash)
- These would require a name registration system (another Nostr event kind)
- This is a product decision for ARME, not a technical requirement

### 6. Legal/regulatory considerations
- ARME is a government regulator — they may have specific obligations regarding domain content
- NoDNS records are DNS data (IP addresses, hostnames), not content
- The actual content is served by the IP addresses in the records, which are outside ARME's DNS infrastructure
- This is the same model as any domain registration — the registrar delegates, the registrant runs their server
