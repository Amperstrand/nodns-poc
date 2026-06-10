# 14 — Demo Recipes

> **Status**: ACTIVE. Step-by-step commands to test and demo every feature of NoDNS.

## Prerequisites

- SSH access to VPS: `ssh root@46.224.104.12`
- A Nostr keypair (nsec/npub) for publishing test events
- `dig` installed locally (macOS: pre-installed)
- The Rust bot running on VPS: `nodns-bot.service`

---

## Demo 1: DNS Basics — Verify the Zone is Alive

**What it shows**: nodns.shop is a real DNS zone served by Knot DNS, with a working secondary (puck.nether.net).

```bash
# 1. Check the apex resolves
dig @8.8.8.8 nodns.shop A +short
# Expected: 46.224.104.12

# 2. Check nameserver delegation
dig @8.8.8.8 nodns.shop NS +short
# Expected: ns1.nodns.shop. / puck.nether.net.

# 3. Check glue record
dig @8.8.8.8 ns1.nodns.shop A +short
# Expected: 46.224.104.12

# 4. Check secondary is serving
dig @204.42.254.5 nodns.shop SOA +short
# Expected: ns1.nodns.shop. admin.nodns.shop. <serial> 3600 600 2592000 60

# 5. Check the TXT record
dig @8.8.8.8 nodns.shop TXT +short
# Expected: "NoDNS - DNS records from Nostr events"
```

**What to say**: "nodns.shop is a standard DNS zone — Knot DNS authoritative, secondary at puck.nether.net. Everything you'd expect from a production DNS setup."

---

## Demo 2: DNSSEC — Zone is Cryptographically Signed

**What it shows**: Every record in the zone carries an RRSIG signature. DNSSEC is live.

### 2a. Local Verification (on VPS)

```bash
ssh root@46.224.104.12

# 1. Check signed SOA
dig +dnssec @127.0.0.1 nodns.shop SOA
# Expected: RRSIG SOA 13 2 ... in additional section

# 2. Check DNSKEY records (KSK + ZSK)
dig @127.0.0.1 nodns.shop DNSKEY +short
# Expected: two records — one with flags 257 (KSK), one with flags 256 (ZSK)

# 3. Check NSEC3 parameters
dig @127.0.0.1 nodns.shop NSEC3PARAM
# Expected: NSEC3PARAM record, flags=0, iterations=0, salt=-

# 4. Validate chain with delv
delv @127.0.0.1 nodns.shop SOA
# Expected: "; fully validated"

# 5. List DNSSEC keys
keymgr nodns.shop list
# Expected: KSK tag 12717, ZSK tag 33240
```

### 2b. Public DNS Verification

```bash
# From your local machine — check signed responses from Google DNS
dig +dnssec @8.8.8.8 nodns.shop SOA
# Expected: RRSIG in response, flags line shows "DO" (DNSSEC OK)

# Check a non-existent record (NSEC3 denial of existence proof)
dig +dnssec @8.8.8.8 thisdoesnotexist.nodns.shop A
# Expected: NXDOMAIN + NSEC3 record + RRSIG NSEC3
```

### 2c. Full DNSSEC Chain (after DS submission at Namecheap)

```bash
# After submitting DS record and waiting 1-4 hours:
dig +dnssec @8.8.8.8 nodns.shop SOA
# Expected: "ad" flag in flags line = Authenticated Data = fully validated chain

# Verify from multiple resolvers
dig +dnssec @1.1.1.1 nodns.shop SOA    # Cloudflare
dig +dnssec @9.9.9.9 nodns.shop SOA    # Quad9
# All should show "ad" flag
```

**What to say**: "The zone is signed with ECDSAP256SHA256, 65 records with RRSIG signatures. NSEC3 denial-of-existence is active. The `ad` flag in public DNS means the full chain of trust from root → .shop → nodns.shop is validated."

---

## Demo 3: Nostr → DNS — The Core Pipeline

**What it shows**: Publishing a Nostr event creates a DNS record within seconds.

### 3a. Check the Bot is Running

```bash
ssh root@46.224.104.12

# Check bot status
systemctl status nodns-bot
# Expected: active (running)

# Check bot health endpoint
curl -s http://127.0.0.1:9090/health | python3 -m json.tool
# Expected: JSON with status "ok", events_processed counter

# Check recent bot logs
journalctl -u nodns-bot -n 20 --no-pager
```

### 3b. Publish a Test DNS Event

Using any Nostr client that supports kind 11111, publish:

```json
{
  "kind": 11111,
  "tags": [
    ["record", "", "TXT", "IN", "3600", "hello from nostr!"]
  ],
  "content": ""
}
```

**Or via `nak` CLI** (if installed):
```bash
nak event --kind 11111 --tag '["record","","TXT","IN","3600","hello from nostr!"]' wss://relay.damus.io
```

### 3c. Verify DNS Resolution

```bash
# Wait 3-5 seconds, then:
ssh root@46.224.104.12 'dig @127.0.0.1 <npub>.nodns.shop TXT +short'
# Expected: "hello from nostr!"

# From public DNS
dig @8.8.8.8 <npub>.nodns.shop TXT +short
# Expected: "hello from nostr!" (may take a few more seconds)
```

### 3d. Verify DNSSEC on the New Record

```bash
dig +dnssec @8.8.8.8 <npub>.nodns.shop TXT
# Expected: TXT record + RRSIG TXT — automatically signed by Knot after DDNS
```

**What to say**: "I published a Nostr event, and within 3 seconds the DNS record appeared — fully signed with DNSSEC. No human intervention, no control panel, no API key. Just a Nostr event."

---

## Demo 4: The Update Flow — Modify and Delete

**What it shows**: Updating and deleting DNS records via Nostr events.

### 4a. Update a Record

Publish a new event from the **same npub** with the **same record name** but different content:

```json
{
  "kind": 11111,
  "tags": [
    ["record", "myapp", "A", "IN", "3600", "203.0.113.42"]
  ],
  "content": ""
}
```

Then update it:

```json
{
  "kind": 11111,
  "tags": [
    ["record", "myapp", "A", "IN", "3600", "198.51.100.7"]
  ],
  "content": ""
}
```

```bash
# Verify the update
dig @8.8.8.8 myapp.<npub>.nodns.shop A +short
# Expected: 198.51.100.7
```

### 4b. Delete a Record

Publish a kind 5 (deletion) event referencing the event ID:

```json
{
  "kind": 5,
  "tags": [
    ["e", "<event_id_of_record_to_delete>"]
  ],
  "content": "Delete this record"
}
```

```bash
# Verify deletion
dig @8.8.8.8 myapp.<npub>.nodns.shop A
# Expected: NXDOMAIN (after 60s negative cache TTL)
```

---

## Demo 5: Record Types

**What it shows**: NoDNS supports standard DNS record types — A, AAAA, TXT, CNAME, MX.

```json
// A record
{ "kind": 11111, "tags": [["record", "web", "A", "IN", "3600", "203.0.113.42"]] }

// AAAA record
{ "kind": 11111, "tags": [["record", "web", "AAAA", "IN", "3600", "2001:db8::1"]] }

// TXT record
{ "kind": 11111, "tags": [["record", "", "TXT", "IN", "3600", "v=spf1 include:_spf.google.com ~all"]] }

// CNAME record
{ "kind": 11111, "tags": [["record", "blog", "CNAME", "IN", "3600", "myblog.github.io."]] }

// MX record
{ "kind": 11111, "tags": [["record", "", "MX", "IN", "3600", "10 mail.example.com."]] }
```

Verify each:
```bash
dig @8.8.8.8 web.<npub>.nodns.shop A +short
dig @8.8.8.8 web.<npub>.nodns.shop AAAA +short
dig @8.8.8.8 <npub>.nodns.shop TXT +short
dig @8.8.8.8 blog.<npub>.nodns.shop CNAME +short
dig @8.8.8.8 <npub>.nodns.shop MX +short
```

---

## Demo 6: DNSSEC Verification Deep Dive

**What it shows**: How DNSSEC validation works under the hood.

```bash
# 1. Get the DNSKEY
dig @8.8.8.8 nodns.shop DNSKEY +dnssec

# 2. Get the DS record from .shop
dig @8.8.8.8 nodns.shop DS +short
# Expected: 12717 13 2 b5a6a5f1...55758726

# 3. Trace the chain of trust
dig +trace @8.8.8.8 nodns.shop SOA +dnssec
# Shows: root → .shop → nodns.shop delegation chain

# 4. Verify a specific record's RRSIG
dig @8.8.8.8 <npub>.nodns.shop TXT +dnssec +multiline
# Shows: TXT record, RRSIG with signer name, algorithm, key tag, timestamps

# 5. Prove non-existence with NSEC3
dig @8.8.8.8 nonexistent123.nodns.shop A +dnssec +multiline
# Shows: NXDOMAIN, NSEC3 record proving no such name exists, RRSIG on NSEC3
```

**What to say**: "Every response is signed. Non-existence is also proven — NSEC3 provides cryptographic proof that a record doesn't exist. You can't lie about what's in the zone."

---

## Demo 7: Health Check and Monitoring

```bash
ssh root@46.224.104.12

# Bot health
curl -s http://127.0.0.1:9090/health | python3 -m json.tool

# Bot metrics (events processed, DDNS successes/failures)
curl -s http://127.0.0.1:9090/metrics 2>/dev/null || echo "Metrics endpoint may not exist yet"

# Knot zone status
knotc zone-status nodns.shop

# Knot zone stats
knotc zone-stats nodns.shop

# Recent DDNS activity in Knot logs
journalctl -u knot --since "1 hour ago" | grep -i "update\|ddns\|sign"

# Zone serial (to verify updates are being applied)
dig @127.0.0.1 nodns.shop SOA +short | head -1 | awk '{print $3}'
```

---

## Quick Reference: SSH One-Liners

```bash
# Bot status
ssh root@46.224.104.12 'systemctl status nodns-bot --no-pager'

# Bot logs (last 20 lines)
ssh root@46.224.104.12 'journalctl -u nodns-bot -n 20 --no-pager'

# Bot health
ssh root@46.224.104.12 'curl -s http://127.0.0.1:9090/health'

# DNSSEC keys
ssh root@46.224.104.12 'keymgr nodns.shop list'

# Zone serial
ssh root@46.224.104.12 'dig @127.0.0.1 nodns.shop SOA +short'

# Knot status
ssh root@46.224.104.12 'knotc zone-status nodns.shop'

# Restart bot
ssh root@46.224.104.12 'systemctl restart nodns-bot'

# Restart Knot
ssh root@46.224.104.12 'systemctl restart knot'
```

---

## Troubleshooting

### Record not appearing after Nostr event

1. Check bot received the event: `journalctl -u nodns-bot --since "2 min ago"`
2. Check DDNS was sent: look for "DDNS update" in logs
3. Check Knot has the record: `dig @127.0.0.1 <name>.nodns.shop <type>`
4. Check public DNS: `dig @8.8.8.8 <name>.nodns.shop <type>` (may take seconds)

### DNSSEC validation failing publicly

1. DS record not submitted → submit at Namecheap
2. DS submitted recently → wait 1-4 hours
3. Digest mismatch → regenerate: `ssh root@46.224.104.12 'keymgr nodns.shop ds'`

### Bot not receiving events

1. Check relay connectivity in logs
2. Check the event kind is 11111
3. Check the event has the correct zone tag
4. Restart bot: `ssh root@46.224.104.12 'systemctl restart nodns-bot'`
