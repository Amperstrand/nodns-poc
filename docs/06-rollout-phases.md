# Rollout Phases

## Phase 0: Local Development (Week 1)

**Goal**: Bot works locally, resolves records via Knot on localhost.

| Task | Status |
|---|---|
| Write nodns-bot Go code | Pending |
| Set up local Knot DNS instance | Pending |
| Test DDNS updates from bot to Knot | Pending |
| Test DNSSEC signing after DDNS | Pending |
| Test kind 5 deletion handling | Pending |
| Test reconciliation loop | Pending |
| Write unit tests | Pending |

**Exit criteria**: `dig @127.0.0.1 npub1xxx.nostr.cv A` returns the correct IP after publishing a kind 11111 event to a local Nostr relay.

## Phase 1: Demo on VPS (Week 2-3)

**Goal**: `nostr.cv` resolves globally via delegation from ARME.

| Task | Status |
|---|---|
| Provision VPS | Pending |
| Install and configure Knot DNS | Pending |
| Deploy nodns-bot with systemd | Pending |
| Test full pipeline: Nostr event → bot → DDNS → Knot → resolution | Pending |
| Send delegation instructions to ARME | Pending |
| ARME adds NS records to .cv zone | Pending (ARME action) |
| Verify global resolution via public resolvers | Pending |
| Invite 5-10 Nostr users to test | Pending |

**Exit criteria**: Any internet user can `dig npub1xxx.nostr.cv A` and get a response. Multiple Nostr users have successfully published records.

### VPS Cost Estimate

| Component | Monthly Cost |
|---|---|
| VPS (2 vCPU, 2GB RAM) | $5-10 |
| Optional: second IP for ns2 | $1-2 |
| Domain for NS names | $1-2 (annual) |
| **Total** | **~$7-14/month** |

## Phase 2: DNSSEC Chain of Trust (When ARME is ready)

**Goal**: Full DNSSEC validation from root to `nostr.cv` records.

**Prerequisite**: `.cv` DS record in root zone. This is ARME's responsibility via ICANN DNSSEC Roadshow.

| Task | Status |
|---|---|
| Generate KSK + ZSK for nostr.cv | Pending |
| Add DS record to .cv zone (ARME action) | Blocked (waiting for .cv DS in root) |
| Verify chain: root → .cv → nostr.cv → record | Pending |
| Test with DNSSEC-validating resolvers (8.8.8.8, 1.1.1.1) | Pending |

**Exit criteria**: `delv npub1xxx.nostr.cv A` returns "; fully validated" using the root trust anchor.

### DNSSEC Key Details

Current `.cv` DNSSEC configuration:
- Algorithm: ECDSAP256SHA256 (13)
- KSK + ZSK: Both published
- NSEC3: `1 0 0 -` (no opt-out, no salt)
- DS in root: **Not yet** — ARME working on it

For `nostr.cv` demo zone:
- Knot DNS handles all key generation and signing automatically
- `dnssec-signing: on` + `dnssec-policy: default` in knot.conf
- KSK rollover, ZSK rollover, signature refresh — all automatic
- We just need the DS record published in the parent zone for chain of trust

## Phase 3: Multi-User Beta (Week 4-6)

**Goal**: Open to wider Nostr community, prove scalability.

| Task | Status |
|---|---|
| Set up monitoring (health endpoint, structured logs) | Pending |
| Load test: 1000+ records, 100+ concurrent queries | Pending |
| Test with multiple relays (failover, reconnection) | Pending |
| Write user documentation (how to publish kind 11111) | Pending |
| Create a simple web tool for publishing records | Pending |
| Gather feedback from beta users | Pending |

**Exit criteria**: 100+ active Nostr users with resolving domains. Bot uptime >99.5% over 2 weeks.

## Phase 4: Secondary DNS (When production-ready)

**Goal**: Add DNS secondaries for redundancy.

Options for secondaries:
1. **DNS.PT servers** — If ARME agrees, their Knot DNS servers can AXFR/IXFR from our primary
2. **Commercial secondary DNS** — EasyDNS, DNSMadeEasy, etc. (~$5-20/month)
3. **Community secondaries** — Ask the DNS/Nostr community for donated anycast

| Task | Status |
|---|---|
| Configure Knot to NOTIFY secondaries | Pending |
| Configure secondaries to AXFR from primary | Pending |
| Test zone transfers | Pending |
| Test failover (stop primary, verify secondaries still serve) | Pending |

**Exit criteria**: Domain resolves even when primary VPS is down. SOA serial consistent across all servers.

## Phase 5: Production Integration with ARME (Future)

**Goal**: NoDNS integrated natively into `.cv` zone management.

This requires ARME to run the bot on their infrastructure alongside `ns.dns.cv`.

| Task | Status |
|---|---|
| ARME installs nodns-bot on ns.dns.cv | Pending (ARME action) |
| Configure Knot on ns.dns.cv to accept DDNS from localhost | Pending (ARME action) |
| Remove nostr.cv delegation (no longer a subdomain — records go directly in .cv zone) | Pending (ARME action) |
| Bot subscribes to Nostr and sends DDNS for npub1*.cv records | Pending |
| Existing secondaries (AFNIC, ISC, APNIC, etc.) receive NOTIFY/IXFR as normal | Pending |
| Full DNSSEC chain: root → .cv → npub1*.cv | Pending (requires .cv DS in root) |

**Exit criteria**: `dig npub1xxx.cv A` resolves directly from the `.cv` zone, served by ARME's infrastructure, with full DNSSEC chain of trust.

### What changes for ARME in Phase 5

| Before | After |
|---|---|
| `.cv` zone: static, human-edited | `.cv` zone: static + dynamic DDNS section for `npub1*` subdomains |
| No Nostr software running | One Go binary (nodns-bot) as a systemd service |
| No outbound connections to Nostr | Bot connects to Nostr relays (outbound WebSocket) |
| No changes to existing records | Existing records untouched — bot only manages `npub1*` records |

## Phase 6: Premium Names (Future — Product Decision)

**Goal**: Human-readable names like `alice.cv` available for purchase via Cashu.

This is a product and policy decision for ARME. Technical implementation:

1. Define a new Nostr event kind for name registration (e.g., kind 30011)
2. Event includes: desired name, Cashu proof of payment
3. Bot validates: name available? payment valid? (via Cashu mint)
4. Bot sends DDNS: `alice.cv A 203.0.113.42`
5. Name registration stored in SQLite with expiration

| Name format | Price | Duration |
|---|---|---|
| `npub1{hex}.cv` | Free | Permanent (tied to npub) |
| `{name}.cv` (3+ chars) | Cashu payment | Annual renewal |

**This is entirely optional.** The free tier (npub-based names) works without any payment system.

## Timeline Summary

```
Week 1:        Phase 0 — Local dev
Week 2-3:      Phase 1 — Demo on VPS + ARME delegation
When ready:    Phase 2 — DNSSEC chain of trust (blocked on .cv DS in root)
Week 4-6:      Phase 3 — Multi-user beta
When ready:    Phase 4 — Secondary DNS
Future:        Phase 5 — Production integration with ARME
Future:        Phase 6 — Premium names (product decision)
```

Phases 0-3 can proceed immediately. Phase 2 is blocked on ARME/ICANN. Phases 4-6 depend on demo success and ARME's decision to move forward.
