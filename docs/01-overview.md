# NoDNS for .cv — Project Overview

## What is NoDNS?

NoDNS is a protocol that resolves DNS records from Nostr events. Instead of registering domains through a traditional registrar and configuring DNS through a control panel, users publish cryptographically-signed events to Nostr relays. A NoDNS-compatible nameserver reads these events and serves them as standard DNS responses.

Protocol spec: https://relay.ngit.dev (repos: `no-dns`, `nodns-protocol-spec`)

Core event type: **kind 11111** — DNS record events with a fixed 11-element `record` tag.

## Why .cv?

- **Cape Verde's ccTLD**, operated by ARME (Agência Reguladora Multissetorial da Economia) with technical backend provided by DNS.PT
- **Relaunched globally in 2024** for personal/CV websites
- **ARME is open to innovation** — active talks about implementing NoDNS
- **Small ccTLD** — ideal proving ground before scaling to larger TLDs
- **DNSSEC in progress** — KSK+ZSK published (ECDSAP256SHA256), NSEC3 `1 0 0 -`, DS record pending in root zone via ICANN DNSSEC Roadshow

## DNS Infrastructure Fingerprint

| Server | IP | Software | Notes |
|---|---|---|---|
| `ns.dns.cv` | 41.221.192.220 | Unknown | Primary master, ARME facilities, Cape Verde |
| `curiosity.dns.pt` | 193.137.12.78 | Knot DNS | DNS.PT primary |
| `curiosity2.dns.pt` | 193.137.12.79 | Knot DNS | DNS.PT secondary |
| `ns2.dns.pt` | 193.137.12.80 | Knot DNS | DNS.PT secondary |
| `nsx.dns.pt` | 194.117.18.164 | Knot DNS | DNS.PT secondary |
| `ns2.nic.fr` | 192.93.0.4 | Knot DNS | AFNIC secondary for .cv |
| `ns3.nic.fr` | 192.134.0.49 | Knot DNS | AFNIC secondary for .cv |
| `sns-pb.isc.org` | 192.5.4.1 | BIND 9 | ISC secondary for .cv |
| `sec3.apnic.net` | 202.12.28.140 | Unknown | APNIC secondary for .cv |
| `ns-ext.nlnetlabs.nl` | 194.0.28.53 | Unknown | NLnet Labs secondary for .cv |
| `ns-yv.ipv4.zele.comm.cx` | 91.205.151.241 | Unknown | Secondary for .cv |

**Key finding**: DNS.PT (the technical operator) uses Knot DNS across all their servers. Knot DNS has native DDNS support and RCU-based lock-free zone updates — ideal for dynamic Nostr-driven updates.

## Project Goals

1. **Demo**: Stand up `nostr.cv` as a delegated subdomain running NoDNS — prove the concept works without touching production `.cv` infrastructure
2. **Integration path**: Define how ARME could integrate NoDNS natively into `.cv` zone management
3. **Protocol compliance**: Implement the NoDNS protocol spec (kind 11111 events, 11-element record tags)
4. **DNSSEC**: Sign the NoDNS zone, prepare for chain-of-trust when `.cv` DS lands in root

## Document Index

| Document | Content |
|---|---|
| [01-overview.md](01-overview.md) | This file — project context and goals |
| [02-architecture.md](02-architecture.md) | System design, Knot DNS analysis, DDNS mechanism |
| [03-bot-spec.md](03-bot-spec.md) | nodns-bot detailed specification |
| [04-demo-setup.md](04-demo-setup.md) | Step-by-step demo setup for nostr.cv |
| [05-arme-delegation.md](05-arme-delegation.md) | What ARME needs to do (delegation instructions) |
| [06-rollout-phases.md](06-rollout-phases.md) | Phased rollout from demo to production |
| [07-abuse-philosophy.md](07-abuse-philosophy.md) | Abuse handling philosophy |
