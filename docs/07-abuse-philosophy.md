# 07 — Abuse Handling Philosophy

> **Status**: ACTIVE. DNS-as-mirror principle, accountability model.

## Core Principle: DNS is a Mirror, Not a Source

NoDNS does not create new content. Every DNS record served by the NoDNS system is a direct reflection of data already publicly published on Nostr relays. The DNS layer adds resolution capability — not new information, not new attack vectors, not new abuse vectors.

If an npub publishes an A record pointing to `203.0.113.42`, that association (this npub wants to point to this IP) already exists as a public, signed, timestamped Nostr event on multiple relays. The DNS record is just making that same information resolvable via the DNS protocol.

## What NoDNS Records Are

NoDNS records are DNS data — IP addresses, hostnames, text strings. They are not:
- Web content
- Email content
- File hosting
- Communication channels

The actual content lives on the servers at the IP addresses in the records. Those servers are outside the DNS infrastructure and outside the operator's responsibility. This is the same model as every domain registrar — the registrar delegates, the registrant runs their server.

## Built-in Accountability via npub

Every NoDNS record is cryptographically tied to a Nostr public key (npub):

1. **Event is signed**: The Nostr event containing the DNS record is signed with the user's private key. This signature is verifiable by anyone.
2. **Publisher is identifiable**: The npub is a permanent, public cryptographic identity. It's not anonymous — it's pseudonymous with a verifiable identity.
3. **Provenance is transparent**: Every record can be traced back to the exact Nostr event that created it, including timestamp and publisher.
4. **Deletion is self-service**: The publisher can delete their own records at any time by publishing a kind 5 (deletion) event.

This is more accountability than traditional DNS provides. In traditional DNS, a domain registration shows the registrar (or WHOIS privacy service). In NoDNS, the cryptographic signature is the proof of ownership — no intermediary, no privacy shield, no ambiguity.

## Technical Abuse Prevention (Bot Policy Engine)

The bot enforces technical policies to prevent infrastructure-level abuse:

### DNS Rebinding Prevention
Block private/reserved IP ranges in A/AAAA records:
- RFC 1918 (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16)
- Loopback (127.0.0.0/8, ::1)
- Link-local (169.254.0.0/16, fe80::/10)
- CGNAT (100.64.0.0/10)

This prevents DNS rebinding attacks where an attacker points a domain at an internal IP to bypass browser same-origin policies.

### Rate Limiting
Max events per npub per minute (default: 5). Prevents a single user from flooding the system with updates, which would cause excessive DDNS traffic and DNSSEC re-signing.

### Record Limits
Max records per npub (default: 20). Prevents zone bloat from a single user.

### Type Restrictions
Only allowed record types are accepted (default: A, AAAA, CNAME, TXT, MX). Excludes types that could be used for attacks:
- No SRV (could redirect service discovery)
- No PTR (could spoof reverse DNS)
- No NS (could create subdomain delegation chains)

### TXT Length Limits
Max TXT record length (default: 512 chars). Prevents oversized TXT records which could be used for DNS amplification.

## Content Abuse: Not a DNS Problem

If a user publishes an A record pointing to a server hosting illegal content:

1. **The content is already public** on the IP address, accessible directly or via any other domain pointing to it
2. **The Nostr event is already public** — the association between this npub and this IP is already on multiple relays
3. **The DNS record adds nothing new** — it makes the IP resolvable via a domain name, but the IP is already accessible
4. **The correct response is to address the server**, not the DNS record pointing to it

This is identical to traditional DNS: if `example.com` points to a server hosting illegal content, the solution is to address the server, not remove the DNS record. Law enforcement goes after the server, not the DNS.

### What the Operator Can Do (if they choose)

If the operator receives a complaint about a `nodns.shop` domain:

1. **Look up the npub**: The domain name contains the npub (or can be looked up in the bot's SQLite database)
2. **Find the Nostr event**: The event ID is stored in SQLite, the event is on public relays
3. **Contact the publisher**: Via Nostr (they have the npub) or via the IP address's hosting provider
4. **Remove the record**: If the operator has policy justification, they can remove the record from the zone directly, or shut down the DNS server entirely (kills all `nodns.shop` domains instantly)

### The Nuclear Option

The operator can stop serving all `nodns.shop` domains at any time by shutting down the DNS server or removing the zone configuration. This immediately stops all `nodns.shop` domains from resolving globally. No external coordination needed — it's entirely within the operator's control.

This is a stronger kill switch than traditional domain takedowns, which often require cooperation from registrars, hosting providers, and courts. The operator has direct, immediate control.

## Comparison with Traditional DNS

| Aspect | Traditional DNS | NoDNS |
|---|---|---|
| Identity | WHOIS (often privacy-protected) | npub (cryptographic, verifiable) |
| Registration | Through registrar, requires personal info | Self-service via Nostr event |
| Accountability | Registrar → registrant chain | Direct: event signature → npub |
| Content responsibility | Registrar delegates, registrant serves | Bot mirrors relay data, user's server serves |
| Kill switch | Registrar suspension (hours-days) | Delegation removal (seconds) |
| Transparency | WHOIS (often hidden) | All events public on relays |
| Audit trail | Registrar logs | Nostr events (immutable, signed, timestamped) |

## Summary

NoDNS DNS records are mirrors of public Nostr events. The DNS layer provides resolution, not content. The npub provides accountability. The bot's policy engine prevents technical abuse (rebinding, flooding). Content concerns belong at the content layer (the server), not the DNS layer.

For the operator, the risk profile is lower than traditional domain registration: every record is cryptographically attributable, the zone can be shut down instantly, and no personal data is collected or stored.
