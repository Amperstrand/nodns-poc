# NIP-97 vs NoDNS: Technical Comparison

> **Date**: 2026-06-11  
> **Purpose**: Competitive analysis for future decisions on adopting, extending, or staying independent from NIP-97.

## Executive Summary

| Aspect | NIP-97 (Nostr Name System) | NoDNS (Kind 11111) |
|--------|----------------------------|-------------------|
| **Status** | Draft (Optional) | Experimental Draft |
| **Author** | vitorpamplona | Community/Nostr ecosystem |
| **Primary Focus** | IP-based DNS for relays & media servers | Full DNS management with cryptographic ownership |
| **Event Kind** | 30053 | 11111 |
| **Resolution Model** | Client-side interception | Server-side + DNS overlay |
| **Name Classes** | IP addresses only | `$npub.tld` (cryptographic) + `$string.tld` (delegated) |
| **Record Types** | `ip4`, `ip6` (A records) | All standard DNS types (A, AAAA, CNAME, TXT, MX, etc.) |
| **Owner Model** | Addressable events by d-tag | Cryptographic (`$npub`) or delegated (`$string`) |
| **Zones** | Implicit (relay-specific) | Zone-agnostic, multi-zone support |
| **Payment Model** | None specified | Cashu (250 sats/record), NIP-57 Zap support |
| **DNSSEC Support** | Not mentioned | Optional (ECDSAP256SHA256) |
| **CNAME Support** | Proposed in PRs | Fully specified |
| **Delegation** | Not specified | Built-in delegation system |
| **Resolution Complexity** | Low (client-side) | Medium (server-side + optional DNS overlay) |
| **Adoption** | Low (unimplemented, draft) | Medium (live demo: nodns.shop) |

---

## 1. Event Structure

### NIP-97 (Kind 30053)

```json
{
  "kind": 30053,
  "tags": [
    ["d", "<subdomain-like-name or empty for root>"],
    ["ip4", "230.22.120.232", "3600"],
    ["ip4", "230.22.120.233", "3600"],
    ["ip6", "FE80:0000:0000:0000:0202:B3FF:FE1E:8329"]
  ]
}
```

**Key characteristics**:
- Simple IP-based records only
- Optional TTL field
- Focus on relay and media server addresses
- Multiple IPs for load balancing

### NoDNS (Kind 11111)

```json
{
  "kind": 11111,
  "content": "DNS record update",
  "tags": [
    ["record", "A", "", "193.99.144.80", "", "", "", "", "", "", "3600"],
    ["record", "TXT", "", "v=spf1 include:_spf.google.com ~all", "", "", "", "", "", "", "3600"],
    ["delegation", "alice.cv", "npub1...", "1749168000", "1780704000", "1778025600"],
    ["cashu", "cashuA...", "https://mint.example.com", "250"]
  ]
}
```

**Key characteristics**:
- Full DNS record support (A, AAAA, CNAME, TXT, MX, etc.)
- Delegation system for custom names
- Payment integration (Cashu, Zap)
- Zone-agnostic (works across multiple zones)
- Record deletion supported
- Multiple record types in single event

---

## 2. Record Types Supported

### NIP-97

| Record Type | Support | Notes |
|-------------|---------|-------|
| `ip4` | ✅ Primary | Represents DNS A record |
| `ip6` | ✅ Primary | Represents DNS AAAA record |
| `cname` | 🔄 Proposed | Discussed in PRs, not finalized |
| `txt` | ❌ Not supported | Would require extension |
| `mx` | ❌ Not supported | Would require extension |
| Other types | ❌ Not supported | Not part of design |

**Limitation**: NIP-97 is intentionally IP-focused for relays and media servers. It doesn't support the full DNS record ecosystem.

### NoDNS

| Record Type | Support | Notes |
|-------------|---------|-------|
| `A` | ✅ Primary | IPv4 addresses |
| `AAAA` | ✅ Primary | IPv6 addresses |
| `CNAME` | ✅ Primary | Domain aliases |
| `TXT` | ✅ Primary | Text records (SPF, DKIM, etc.) |
| `MX` | ✅ Primary | Mail exchange records |
| Other types | ⚠️ Extension | Can be added as needed |

**Advantage**: NoDNS is designed as a general-purpose DNS management system, not just IP addressing.

---

## 3. Ownership Model

### NIP-97: Addressable Events

**Concept**: Uses the `d` tag to create addressable events.

```
d-tag = subdomain-like-name
Event reference = naddr1... (from NIP-19) with kind 30053
```

**Owner verification**:
- Anyone can create an event with any `d` tag
- Multiple pubkeys can point to the same IP address
- No cryptographic link between the `d` tag and the pubkey
- Ownership is **implicit** — whoever publishes the event "owns" it

**Limitation**: This is fundamentally an address allocation problem, not an identity problem. Anyone can claim any subdomain, just like traditional DNS but without the registry layer.

### NoDNS: Two Name Classes

#### Class 1: `$npub.tld` (Cryptographic Ownership)

```
Example: npub1ykal2...pa3dl.nodns.shop
```

**Properties**:
- **Owner**: Whoever holds the nsec (private key)
- **Trust anchor**: Cryptography (mathematical impossibility to forge)
- **Enforcement**: Impossible to override
- **Cost**: Free to claim
- **Irrevocable**: Operator cannot seize

**Verification**:
```json
{
  "kind": 11111,
  "pubkey": "<npub-holder>",
  "tags": [["record", "A", "", "1.2.3.4", ...]]
}
```
- The event's pubkey IS the owner
- Mathematical impossibility to forge
- No delegation needed

#### Class 2: `$string.tld` (Delegated Ownership)

```
Example: alice.nodns.shop
```

**Properties**:
- **Owner**: Whoever the operator delegates to (via signed event)
- **Trust anchor**: Reputation (operator's honesty)
- **Enforcement**: Social (operator loses reputation if they cheat)
- **Cost**: Paid lease (~$10-15 equivalent per year)
- **Lease-based**: Not permanent property

**Delegation mechanism**:
```json
{
  "kind": 11111,
  "pubkey": "<registrar>",
  "tags": [["delegation", "alice.cv", "npub1...", "1749168000", "1780704000", "1778025600"]]
}
```

**Verification**:
- Must be signed by the zone's authorized registrar
- Domain must be within signer's zone authority
- Can be revoked only by expiration
- One domain → one active delegation

**Advantage**: NoDNS provides both **cryptographic** (npub) and **social** (delegated) ownership models, addressing different use cases.

---

## 4. Zone Model

### NIP-97: Implicit Zones

**Concept**: Zones are implicit and relay-specific.

**Key points**:
- No explicit zone identifier in events
- Zones are determined by context (which relay you're publishing to)
- Multiple relays can host different "zones"
- No standard way to discover zone boundaries
- Zone awareness is purely infrastructure-layer concern

**Limitation**: No clear consensus on what constitutes a zone, making interoperability difficult.

### NoDNS: Zone-Agnostic Events

**Concept**: Events contain no zone information.

```json
{
  "kind": 11111,
  "tags": [["record", "A", "", "1.2.3.4", ...]]
  // No zone identifier here
}
```

**Key points**:
- Same event format works across all zones
- Zone assignment is infrastructure-layer concern
- A bot for `.nostr` can process events meant for `.nodns.shop`
- Adding new zones doesn't change the protocol
- Same event produces identical records across all zones

**Example**:
```json
{
  "kind": 11111,
  "tags": [["record", "A", "", "1.2.3.4", ...]]
}
```
This single event can be processed by:
- Bot configured for `.nostr` → creates records under `npub*.nostr`
- Bot configured for `.nodns.shop` → creates records under `npub*.nodns.shop`
- Bot configured for both → creates records in both zones

**Advantage**: Zone-agnostic design enables easy multi-zone support and portability.

---

## 5. Payment Model

### NIP-97

**Status**: Payment model not specified in the draft.

**Implications**:
- No anti-spam mechanism
- No revenue model for operators
- Any relay operator can host NIP-97 events
- Risk of spam without economic barriers

### NoDNS

**Status**: Payment model explicitly designed.

#### Cashu Payment Tag

```json
["cashu", TOKEN, MINT_URL, AMOUNT]
```

**Pricing**:
- Free names (`npub*.zone`): 250 sats per record creation (anti-spam)
- Custom names (`alice.zone`): ~$10-15 equivalent per year lease
- Updates: Free for existing records

#### NIP-57 Zap Payment Tag

```json
["zap", ZAP_RECEIPT_EVENT_ID, AMOUNT]
```

**Use case**: Optional proof of payment for non-record operations.

**Advantage**: NoDNS has a built-in, explicit payment model that addresses spam and provides revenue for operators.

---

## 6. Resolution Architecture

### NIP-97: Client-Side Resolution

**Mechanism**:
1. Clients intercept DNS resolution of `naddr1` addresses
2. Convert `naddr1` to IP using latest NIP-97 event
3. Make direct IP-based connection

**Fallback resolution** (if local event not found):
1. Connect to IP in the `relay` field of the `naddr1`
2. Download potentially outdated NNS record
3. Retrieve latest version from NIP-65 WRITE relays

**Broadcasting**:
- NNS events must be broadcasted to same relays as kind 10002 (relay lists)

**Pros**:
- Simple client-side implementation
- No server infrastructure needed
- True peer-to-peer

**Cons**:
- Requires client-side changes
- DNS interception not possible in all environments (e.g., some browsers)
- Caching challenges (latest version vs outdated)
- Not integrated with traditional DNS

### NoDNS: Dual-Resolution Model

**Primary mechanism**: Server-side resolution (NoDNS resolvers)

1. Bot subscribes to Nostr relays
2. Validates events (signature, authority, payment)
3. Pushes DDNS updates to authoritative DNS server
4. DNS server serves records with DNSSEC

**Secondary mechanism**: DNS overlay (optional)

1. NoDNS resolvers watch Nostr events directly
2. Return Nostr-based DNS records
3. Falls back to traditional DNS if no Nostr events exist

**Resolution flow**:
```
User queries dns.nodns.shop or local resolver
    ↓
NoDNS resolver checks for Nostr events
    ↓
If found: Returns Nostr-based DNS records
If not found: Falls back to traditional DNS
```

**Live demo**: [nodns.shop](https://nodns.shop) - DoH endpoint at `/dns-query`

**Pros**:
- Works with standard DNS clients
- No client-side changes needed
- Full DNSSEC integration
- Integrated with existing DNS ecosystem

**Cons**:
- Requires server infrastructure (bot + DNS server)
- More complex than NIP-97
- Relies on operator's honesty (for traditional DNS overlay)

---

## 7. DNSSEC Support

### NIP-97

**Status**: Not mentioned in the draft.

**Implication**: If NIP-97 is used with traditional DNS, DNSSEC must be handled separately. No native DNSSEC integration.

### NoDNS

**Status**: Optional but fully designed.

**Implementation**:
- DNSSEC signing with ECDSAP256SHA256
- NSEC3 with 0 iterations (RFC 9276)
- DS record at registrar
- TSIG for DDNS updates

**Benefits**:
- Security-integrated design
- Interoperable with standard DNS
- Optional (can disable if needed)

**Advantage**: NoDNS is designed with DNSSEC as a first-class citizen, not an afterthought.

---

## 8. Custom Name Support

### NIP-97

**Status**: Not addressed in the draft.

**Implication**: The `d` tag can be used for any string, but there's no mechanism for:
- Name registration/ownership
- Delegation
- Renewal
- Dispute resolution

**Result**: Custom names would work like traditional DNS but without a registry layer — anyone can claim any name.

### NoDNS

**Status**: Fully specified.

**Custom name flow**:
1. **Registration**: User pays registrar, registrar signs delegation event
2. **Delegation**: Registrar publishes delegation event (kind 11111, delegation tag)
3. **Record management**: User publishes DNS records (kind 11111, record tags)
4. **Renewal**: Must renew before `renew_by` deadline
5. **Lease-based**: Names are leases, not property

**Example delegation**:
```json
{
  "kind": 11111,
  "pubkey": "<registrar>",
  "content": "Domain delegation: alice.cv → npub1...",
  "tags": [
    ["delegation", "alice.cv", "npub1...", "1749168000", "1780704000", "1778025600"]
  ]
}
```

**Advantage**: NoDNS provides a complete custom name system with registration, delegation, and renewal mechanics.

---

## 9. Maturity & Adoption

### NIP-97

| Metric | Status |
|--------|--------|
| **Published as NIP** | Yes (draft, optional) |
| **Reference Implementation** | No official implementation |
| **Live Demo** | None found |
| **Client Support** | None detected |
| **Relay Support** | None detected |
| **Adoption Level** | 0% (spec only) |
| **Status** | Draft, not merged into master |

**Git history**:
- Last update: March 2026 (PR #1968 discussion)
- Branch: `relay-hints-v2` (not master)
- Not merged into official NIPS repository

### NoDNS

| Metric | Status |
|--------|--------|
| **Published as Spec** | Yes (docs/11-protocol-experimental-draft.md) |
| **Reference Implementation** | Yes (nodns-bot-rs, noddns-bot-rs) |
| **Live Demo** | Yes (nodns.shop, beta.nodns.shop) |
| **Client Support** | Not needed (works with standard DNS) |
| **Relay Support** | Not required (bot-based) |
| **Adoption Level** | Medium (internal community) |
| **Status** | Experimental draft, live demo available |

**Live deployments**:
- [nodns.shop](https://nodns.shop) - Main production demo
- [beta.nodns.shop](https://beta.nodns.shop) - Beta deployment
- [nodns-poc pages](https://amperstrand.github.io/nodns-poc/) - Documentation site

**Advantage**: NoDNS has a working implementation and live demo, while NIP-97 remains a speculative draft.

---

## 10. Key Differences Summary

| Feature | NIP-97 | NoDNS |
|---------|--------|-------|
| **Purpose** | IP addresses for relays/media servers | Full DNS management |
| **Event Kind** | 30053 | 11111 |
| **Record Types** | ip4, ip6 only | A, AAAA, CNAME, TXT, MX, etc. |
| **Owner Model** | Addressable events (implicit) | Cryptographic ($npub) or delegated ($string) |
| **Custom Names** | Not addressed | Fully specified with delegation |
| **Delegation** | Not specified | Built-in system |
| **Payment** | Not specified | Cashu + Zap support |
| **Resolution** | Client-side interception | Server-side + DNS overlay |
| **DNSSEC** | Not mentioned | Optional but supported |
| **Zones** | Implicit, relay-specific | Zone-agnostic, multi-zone |
| **Status** | Draft, optional | Experimental draft, live demo |
| **Adoption** | 0% (spec only) | Medium (working implementation) |

---

## 11. Alignment Areas

### Shared Principles

1. **Nostr-Native DNS Management**: Both systems use Nostr events to manage DNS records
2. **Censorship Resistance**: Neither depends on traditional DNS registries
3. **Decentralized**: No central authority for name ownership
4. **Nostr-First**: Both prioritize Nostr integration over traditional DNS

### Where They Coincide

1. **Address Allocation Problem**: Both attempt to solve how to address services in a Nostr-first network
2. **Client/Server Separation**: NIP-97 leans heavily client-side; NoDNS balances server-side and client-side
3. **Use Case**: Both target relay and media server addressing

---

## 12. Where NoDNS is More Capable

### 1. Complete DNS Ecosystem

NIP-97 only supports IP addresses. NoDNS supports all standard DNS record types:
- **Email**: MX, SPF, DKIM, DMARC
- **Web**: A, AAAA, CNAME
- **Security**: TLSA, DNSSEC
- **Service Discovery**: SRV, SSHFP, etc.

**Impact**: NoDNS can serve as a drop-in replacement for traditional DNS for most use cases.

### 2. Two Ownership Models

NoDNS provides:
- **Cryptographic ownership** (`$npub.tld`) — mathematically irrevocable
- **Social/delegated ownership** (`$string.tld`) — reputation-based leases

NIP-97 has no ownership model — it's pure address allocation.

**Impact**: NoDNS can serve both users who want cryptographic ownership (like email handles) and users who want traditional domain-like names.

### 3. Built-in Payment System

NoDNS has:
- **Cashu integration** (250 sats/record anti-spam)
- **NIP-57 Zap support** (optional payments)

NIP-97 has no payment model specified.

**Impact**: NoDNS can sustain itself economically; NIP-97 would likely struggle with spam without external mechanisms.

### 4. Delegation System

NoDNS provides:
- Formal delegation mechanism
- Renewal deadlines
- Registrar key publication
- Authority chains

NIP-97 has no delegation mechanism.

**Impact**: NoDNS can support traditional domain-like usage (e.g., `alice.cv` managed by registrar). NIP-97 cannot.

### 5. DNSSEC Integration

NoDNS:
- Explicit DNSSEC design (ECDSAP256SHA256)
- NSEC3 configuration
- DS record management
- TSIG for DDNS updates

NIP-97:
- No DNSSEC mentioned

**Impact**: NoDNS provides out-of-the-box DNSSEC security; NIP-97 would require external DNSSEC integration if used with traditional DNS.

### 6. Zone-Agnostic Design

NoDNS:
- Same event format works across all zones
- Multi-zone support (`.nostr`, `.nodns.shop`, custom zones)
- Portability between zones

NIP-97:
- Implicit zones, no clear zone boundaries
- No multi-zone support

**Impact**: NoDNS is more flexible and easier to extend. Adding new zones doesn't change the protocol.

### 7. Live Implementation

NoDNS:
- Live demo: [nodns.shop](https://nodns.shop)
- Working bot implementation (Rust)
- DNSSEC deployed
- Real users testing

NIP-97:
- No official implementation
- No live demo
- Spec only

**Impact**: NoDNS has production-ready components; NIP-97 is still theoretical.

---

## 13. Where NIP-97 Has Advantages

### 1. Simplicity

NIP-97 is simpler:
- Single kind (30053)
- Minimal tags (ip4, ip6, d)
- Client-side resolution
- No server infrastructure needed

NoDNS is more complex:
- Multiple event types (record, deletion, delegation, registrar, payment)
- Server-side resolution required
- Bot infrastructure needed

**Impact**: NIP-97 may be easier to implement for simple use cases (relay addressing). NoDNS is more complex but more powerful.

### 2. True Peer-to-Peer

NIP-97:
- Client-side resolution
- No server required
- Pure peer-to-peer

NoDNS:
- Requires bot infrastructure
- Server-side processing
- Traditional DNS overlay

**Impact**: NIP-97 achieves true peer-to-peer resolution. NoDNS requires some server infrastructure.

### 3. DNS Interception

NIP-97:
- Intercepts DNS resolution at the client
- Can work with standard DNS clients without changes
- Seamless integration

NoDNS:
- Requires NoDNS resolvers or custom DNS config
- Not seamless for all users
- Requires DoH or custom resolver

**Impact**: NIP-97 integrates more seamlessly with existing DNS infrastructure. NoDNS requires adoption of new resolver infrastructure.

### 4. Open Source Ecosystem Support

NIP-97 (potentially):
- Standard Nostr ecosystem
- Could leverage existing Nostr client infrastructure
- No special client requirements

NoDNS:
- Works with standard DNS
- No special client requirements
- But doesn't leverage Nostr clients directly

**Impact**: Both work with standard tools, but NIP-97 is more "Nostr-native" in concept.

---

## 14. Areas Where NoDNS Could Learn from NIP-97

### 1. Client-Side Resolution

**NoDNS weakness**: Relies on bot infrastructure.

**NIP-97 strength**: True client-side resolution.

**Learning opportunity**: Consider a hybrid approach where:
- Bot infrastructure handles initial resolution
- Clients cache and resolve locally
- Periodic background updates from bots

### 2. DNS Interception Patterns

**NoDNS weakness**: Requires custom DNS resolvers.

**NIP-97 strength**: Uses DNS interception for seamless integration.

**Learning opportunity**: Investigate:
- Browser extensions for DNS interception
- Local resolver integration
- Transparent DNS proxy integration

### 3. Simple Address Allocation

**NoDNS weakness**: Complex delegation system for simple cases.

**NIP-97 strength**: Simple address allocation via `d` tag.

**Learning opportunity**: Consider adding:
- Simplified delegation mode for single-operator zones
- Drop-in `d` tag support for backwards compatibility
- Optional complexity reduction

### 4. Load Balancing

**NIP-97 strength**: Multiple IPs with TTL support.

**NoDNS weakness**: Load balancing requires manual CNAME configuration.

**Learning opportunity**: Consider built-in load balancing:
- Automatic round-robin for multiple IPs
- Client-side load balancing
- TTL-based rotation

---

## 15. Recommendation: Adopt, Extend, or Stay Independent?

### Current State Assessment

| Criteria | NIP-97 | NoDNS | Recommendation |
|----------|--------|-------|----------------|
| **Capability** | Limited (IP only) | Comprehensive (full DNS) | **NoDNS** |
| **Maturity** | Draft (no implementation) | Experimental with live demo | **NoDNS** |
| **Adoption** | 0% | Medium (internal community) | **NoDNS** |
| **Economic Model** | None | Built-in (Cashu, Zap) | **NoDNS** |
| **Simplicity** | High | Medium | **NIP-97** |
| **Peer-to-Peer** | Yes | Partial | **NIP-97** |
| **Integration** | Client-side | Server-side | **Both valid approaches** |

### Recommendation: **STAY INDEPENDENT, EXTEND NoDNS**

**Rationale**:

1. **NIP-97 is too limited**: IP-only addressing is insufficient for general-purpose DNS. You need full record types (MX, TXT, etc.) for real-world use.

2. **NoDNS is more capable**: Provides both cryptographic (`$npub`) and delegated (`$string`) ownership models, delegation system, payment integration, and DNSSEC support.

3. **Maturity gap**: NoDNS has a live demo and working implementation. NIP-97 has no official implementation.

4. **Economic viability**: NoDNS has a built-in payment model. NIP-97 does not.

5. **Future-proof**: NoDNS is designed to be zone-agnostic and extensible. NIP-97 is narrow by design.

### Extending NoDNS (Learning from NIP-97)

Consider adding NIP-97's strengths to NoDNS:

1. **Client-side resolution mode**: Optional mode where clients cache and resolve events locally, with background updates from bots.

2. **Simplified delegation**: Add a lightweight `d` tag support for single-operator zones or backwards compatibility.

3. **DNS interception**: Explore browser extensions or local DNS proxy integration for seamless user experience.

4. **Load balancing**: Add built-in support for multiple IPs with TTL-based rotation.

5. **Hybrid approach**: Combine NIP-97's simplicity with NoDNS's capabilities.

### Potential Integration Points

1. **Addressing**: Use NoDNS's cryptographic ownership (`$npub`) as the base for NIP-97-like address allocation.

2. **Resolution**: Consider a hybrid where:
   - Bots handle initial resolution and distribution
   - Clients cache and resolve locally (NIP-97 approach)
   - Periodic background refresh from bots

3. **Protocol design**: Use NoDNS's zone-agnostic design as a foundation, then add NIP-97's client-side resolution features as extensions.

### Decision Framework

**Adopt NIP-97 if**:
- You only need IP addresses for relays
- You want true peer-to-peer resolution
- Simplicity is more important than capability
- You're okay with no economic model

**Adopt NoDNS if**:
- You need full DNS functionality
- You want cryptographic or delegated ownership models
- You want DNSSEC integration
- You want a payment model
- You need a live, working implementation

**Extend NoDNS if**:
- You like NoDNS's capabilities but want NIP-97's simplicity
- You want client-side resolution
- You want DNS interception capabilities

**Stay Independent** (recommended):
- Use NoDNS as your foundation
- Add NIP-97's simple addressing features as extensions
- Maintain independent protocol with clear extension points

---

## 16. Open Questions & Future Research

### For NoDNS

1. **Client-side resolution**: Should NoDNS support optional client-side resolution like NIP-97?
2. **DNS interception**: Should we explore browser extensions or local DNS proxy integration?
3. **Load balancing**: Should we add automatic load balancing for multiple IPs?
4. **Simplicity reduction**: Should we add a simplified mode for single-operator zones?

### For NIP-97

1. **Record types**: Should NIP-97 be extended to support TXT, MX, etc.?
2. **Ownership model**: Should NIP-97 add an ownership mechanism (cryptographic or delegated)?
3. **Delegation**: Should NIP-97 include a delegation system?
4. **Payment**: Should NIP-97 specify a payment model?
5. **Implementation**: Should someone implement NIP-97 and create a working prototype?

### For the Ecosystem

1. **Interoperability**: Can NoDNS and NIP-97 coexist or need to be merged?
2. **Standardization**: Should one approach be standardized as the "Nostr Name System"?
3. **Adoption**: Which approach is more likely to gain ecosystem adoption?
4. **Infrastructure**: What infrastructure is needed to support either approach at scale?

---

## 17. Conclusion

NoDNS and NIP-97 represent two different philosophies for Nostr-native naming:

- **NIP-97**: Simple, IP-focused, client-side resolution, pure address allocation
- **NoDNS**: Comprehensive, full DNS, server-side resolution, two ownership models

**Our recommendation**: Stay independent with NoDNS, but consider incorporating NIP-97's simpler addressing features as extensions. NoDNS's capabilities make it more suitable for general-purpose DNS management, while NIP-97's simplicity may be valuable for specific use cases.

The key insight is that **no single approach solves all problems**. A hybrid solution that combines NoDNS's comprehensive features with NIP-97's simplicity and client-side resolution may be optimal.

---

**Next steps**:
1. Decide on NoDNS extension priorities (client-side resolution, DNS interception, etc.)
2. Create specification for extensions
3. Implement pilot features
4. Test with live deployment
5. Gather community feedback

---

**References**:

- [NIP-97 Draft](https://github.com/vitorpamplona/nips/blob/relay-hints-v2/97.md) (vitorpamplona fork)
- [NoDNS Protocol Spec](docs/11-protocol-experimental-draft.md)
- [NoDNS Name Classes](docs/21-name-classes.md)
- [NIP-97 PR Discussions](https://github.com/nostr-protocol/nips/pull/1330)
- [NIP-97 Alternative PR](https://github.com/nostr-protocol/nips/pull/1968)

**Last updated**: 2026-06-11
