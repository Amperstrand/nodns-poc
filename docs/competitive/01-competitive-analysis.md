# NoDNS Competitive Analysis

## Executive Summary

NoDNS occupies a unique niche: **event-driven DNS management over Nostr**. No other system uses a pub/sub event model for DNS record management. While ENS, Handshake, and Unstoppable Domains focus on *decentralizing the namespace*, NoDNS focuses on *decentralizing the management layer* while staying fully compatible with the existing DNS resolution pipeline.

**NoDNS's core innovation**: DNS records are created by publishing signed Nostr events — no control panel, no API keys, no human intervention. Records propagate globally in ~3 seconds via standard DNS.

---

## Comparison Table

| Feature | **NoDNS** | **ENS** | **Unstoppable Domains** | **Handshake** | **dns0.eu / NextDNS** | **nip.io / sslip.io** | **Namecoin** |
|---|---|---|---|---|---|---|---|
| **Domain format** | `npub.nodns.shop` or delegated custom names | `.eth` or DNS names via DNSSEC | `.crypto`, `.x`, `.nft`, etc. + now DNS TLDs | Custom TLDs (e.g., `.iam/`) | Standard DNS | `anything.IP.nip.io` | `.bit` |
| **Record management** | Nostr events (kind 11111) | Smart contracts on Ethereum | Smart contracts on Polygon | Blockchain covenants | Web dashboard | Automatic from hostname | Blockchain name_update |
| **DNS compatibility** | Full — standard DNS resolvers | Partial — requires CCIP-Read gateway | Partial — needs browser extension/gateway | Requires custom root resolver | Full — standard DNS | Full — standard DNS | Requires ncdns local resolver |
| **DNSSEC** | Yes — ECDSAP256SHA256, full chain of trust | Via DNSSEC proofs for DNS import | No native DNSSEC | Yes — blockchain replaces CA system | Validates DNSSEC | No | Via TLSA records |
| **Resolution speed** | ~3 seconds (standard DNS) | ~1-5s (gateway-dependent) | ~1-5s (gateway-dependent) | Varies; requires HSD resolver | <10ms (anycast) | <10ms (simple lookup) | Requires local node |
| **Payment model** | Cashu (250 sats/record, one-time) | ETH gas fees + annual rent ($5-$640/yr) | One-time purchase ($10-$100+) | Auction-based (HNS tokens) + biennial renew | Free (NextDNS: 300K queries/mo free) | Free | NMC per name_update |
| **Record types** | A, AAAA, CNAME, TXT, MX | Address, content hash, text records | Crypto addresses, redirect URLs | Full DNS records (GLUE4/6, NS, DS, TXT, SYNTH) | N/A (resolver, not authoritative) | A (IPv4), AAAA (IPv6 on sslip.io) | Arbitrary (up to 520 bytes) |
| **Centralization** | Zone operator runs bot + Knot DNS | Ethereum network (decentralized consensus) | Polygon network | Handshake blockchain | Centralized operator | Centralized (single operator) | Bitcoin merge-mined chain |
| **Anti-spam** | Cashu micropayments + rate limits | Gas fees (economic disincentive) | Purchase cost | Auction cost | N/A | N/A | Transaction fees |
| **Identity model** | Nostr keypair (npub/nsec) | Ethereum wallet | Ethereum/Polygon wallet | Handshake wallet | Account-based | Anonymous | Namecoin wallet |
| **Setup complexity** | Publish a Nostr event | Enable DNSSEC + TXT records + claim | Purchase via marketplace | Run auction + configure covenants | Change DNS resolver | Just use the hostname | Install ncdns + configure |
| **Privacy** | Ephemeral keypairs supported | Public on-chain | Public on-chain | Public on-chain | Varies (see below) | No logging (minimal) | Public on-chain |
| **Status** | Live at nodns.shop | Live, GoDaddy integration (2024) | Live, ICANN accredited (Oct 2024) | Live but declining activity | dns0.eu discontinued (2025); NextDNS live | Live | Live but low adoption |

---

## Detailed Competitor Analysis

### 1. ENS (Ethereum Name Service)

**What it is**: Decentralized naming on Ethereum. Maps `.eth` names to addresses, content hashes, and metadata. Also integrates DNS names via DNSSEC proofs.

**DNS approach**: 
- DNS domain owners can import names into ENS using DNSSEC proofs
- Gasless DNSSEC (Jan 2024) via CCIP-Read: no on-chain gas for DNS names
- GoDaddy partnership (2024): one-click crypto wallet on DNS domains
- Resolution requires CCIP-Read gateway (not standard DNS resolution)

**Pricing**: Length-based annual rent — 5+ char names: $5/yr, 4-char: $160/yr, 3-char: $640/yr. Multi-year discounts available. DNS name import is free (gasless).

**DNSSEC**: Uses DNSSEC proofs to verify DNS ownership. Does not serve DNS records — serves blockchain records through ENS resolver.

**NoDNS advantage**: ENS doesn't actually serve DNS records. It's a parallel namespace that happens to accept DNS names. NoDNS creates *actual DNS records* resolvable by any DNS client globally, no gateway needed.

**ENS advantage**: Massive ecosystem (2.7M+ names), browser support (Brave, Opera), DeFi integrations, established governance.

---

### 2. Unstoppable Domains

**What it is**: Web3 domain provider offering `.crypto`, `.x`, `.nft`, `.wallet` and 150+ TLDs. ICANN-accredited registrar since October 2024.

**DNS approach**:
- Web3 domains resolve via browser extensions or blockchain-enabled browsers
- Now an ICANN registrar — can sell `.com`, `.net` etc. with full DNS management
- DNS records managed via web dashboard for traditional domains
- Web3 domain resolution does NOT use standard DNS

**Pricing**: One-time purchase (no renewal) for Web3 domains ($10-$100+). Traditional DNS domains at standard pricing (~$10.99/yr for .com).

**DNSSEC**: Not natively for Web3 domains. Traditional DNS domains would follow standard DNSSEC practices.

**NoDNS advantage**: NoDNS is instant — publish an event, DNS record is live in 3 seconds. No account, no marketplace, no purchase flow. Unstoppable requires account creation, purchase, and manual record configuration.

**UD advantage**: Human-readable names, large marketplace, ICANN accreditation gives real DNS legitimacy, one-time pricing (no renewals for Web3 names).

---

### 3. Handshake

**What it is**: Decentralized root zone replacing ICANN's root. Users auction for TLDs. Every full node acts as a root server.

**DNS approach**:
- Replaces the root zone file with a blockchain
- Full nodes serve as authoritative root servers
- Requires either local HSD resolver or third-party gateway (HDNS) for resolution
- DNSSEC native — TLS keys pinned to blockchain, eliminates CA system
- Backwards compatible: existing ICANN TLDs (.com, .org) are reserved

**Pricing**: Vickrey (blind second-price) auctions using HNS tokens. No renewal fees, but biennial "heartbeat" transaction required. Names expire after ~2 years without renewal.

**DNSSEC**: Core feature — replaces CA system with blockchain-rooted trust.

**NoDNS advantage**: Handshake requires running an HSD node or trusting a gateway. NoDNS works with standard DNS resolvers immediately. NoDNS is simpler — no auction, no blockchain sync, no special resolver. ~3 second propagation vs. Handshake's 6-hour urkel tree commit interval.

**Handshake advantage**: Truly decentralized root zone. Replaces ICANN governance. Can create arbitrary TLDs. Eliminates CA system. NoDNS still relies on .shop TLD (ICANN-controlled).

---

### 4. dns0.eu / NextDNS

**What they are**: Alternative DNS *resolvers* with filtering — not authoritative DNS providers. dns0.eu is now discontinued (announced 2025); NextDNS is live.

**DNS approach**: Recursive resolvers with malware/adult content filtering. Not relevant as DNS *publishing* competitors.

**dns0.eu status**: Service discontinued as of 2025. Was a French non-profit by NextDNS co-founders.

**NextDNS**: Free tier (300K queries/month), customizable block lists, analytics. Available in Firefox/Chromium by default.

**Relevance**: These are resolvers, not competitors to NoDNS's authoritative DNS model. They resolve NoDNS records just fine. However, they're worth noting as part of the "alternative DNS" ecosystem.

**NoDNS advantage**: NoDNS *publishes* records. dns0/NextDNS *resolves* them. Complementary, not competing.

---

### 5. nip.io / sslip.io

**What they are**: Magic DNS services that encode IP addresses in hostnames. `10.0.0.1.nip.io` resolves to `10.0.0.1`.

**Pattern similarity to NoDNS**: This is the closest conceptual match! NoDNS's `npub.nodns.shop` pattern is analogous to nip.io's `IP.nip.io` pattern — both encode identity information in the subdomain.

**How it works**: PowerDNS with a custom PipeBackend. Parses IP from hostname format (dot, dash, or hex notation). No DNSSEC. Centralized service.

**Key difference**: nip.io is *stateless* — the IP is encoded in the name itself, no records are stored. NoDNS is *stateful* — users publish events that create stored DNS records with arbitrary data.

**NoDNS advantage**: 
- Supports multiple record types (A, AAAA, CNAME, TXT, MX) — nip.io only does A/AAAA
- DNSSEC signed — nip.io has no DNSSEC
- Custom delegation — users can claim named subdomains
- TXT records — enables verification, identity, arbitrary data

**nip.io advantage**: Zero configuration. Instant. No payment needed. Perfect for dev/testing. Stateless means no storage, no expiry.

---

### 6. Namecoin

**What it is**: First decentralized DNS (2011). Bitcoin fork with merged mining. Provides `.bit` domains.

**DNS approach**: 
- Names registered via blockchain transactions (name_update)
- Requires ncdns (local DNS resolver) to resolve .bit domains
- Supports arbitrary DNS record data (up to 520 bytes)
- TLSA records for decentralized TLS certificate validation
- Actively developed (2025: post-quantum TLS research, Encaya off-chain records)

**Pricing**: NMC per transaction. Minimal — essentially just transaction fees.

**DNSSEC**: Via TLSA records and blockchain-anchored trust, not traditional DNSSEC.

**NoDNS advantage**: NoDNS resolves via standard DNS — no special software needed. Namecoin requires ncdns or gateway. NoDNS has a web UI for easy access. NoDNS supports standard DNS record types natively.

**Namecoin advantage**: Truly decentralized (merge-mined with Bitcoin). Longest track record. No central operator. Identity system beyond just DNS. Post-quantum research underway.

---

### 7. Nostr-Based Naming (NIP-05)

**What it is**: NIP-05 maps Nostr keys to DNS-based internet identifiers (like `alice@example.com`). Requires a `.well-known/nostr.json` file on a web server.

**How it works**: 
- User publishes kind 0 event with `nip05` field pointing to `name@domain`
- Client fetches `https://domain/.well-known/nostr.json?name=name`
- Response maps `name` to a Nostr pubkey
- Used for verification/identity, not DNS records

**nip05.social**: A service providing free NIP-05 identifiers. User publishes to their relay, service checks and serves the nostr.json.

**Relationship to NoDNS**: NIP-05 is *identity verification* (maps name → npub). NoDNS is *DNS record management* (maps npub → DNS records). They're complementary:
- NIP-05 gives you `alice@example.com` → `npub1...`
- NoDNS gives you `npub1....nodns.shop` → `A 1.2.3.4`

**NoDNS advantage**: NoDNS creates real DNS records. NIP-05 only creates a mapping file. NoDNS supports A/AAAA/CNAME/TXT/MX records. NIP-05 only maps names to pubkeys.

---

### 8. Emerging: Blockchain-Based DDNS (2025 Research)

A 2025 paper (IACR 2025/1381) proposes a PoW blockchain DDNS with:
- 15-second propagation time
- 20 standard DNS record types
- Free `.ddns` domains
- IPFS for distributed storage
- 1,111 TPS theoretical throughput

**Status**: Research paper with limited deployment (3 cities). Not a production competitor yet.

**NoDNS advantage**: Already live and working. Standard DNS (no custom blockchain needed). DNSSEC signed. 3-second propagation.

---

## NoDNS Unique Advantages

1. **Event-driven model**: Only system using pub/sub events for DNS management. No control panel, no API keys, no account. Publish an event, get a DNS record.

2. **Instant propagation**: ~3 seconds from event to globally resolvable DNS record. Faster than any blockchain-based system (ENS: gateway-dependent, Handshake: 6-hour tree commits, Namecoin: block time dependent).

3. **Standard DNS resolution**: Works with every DNS client on earth. No browser extensions, no special resolvers, no gateway. `dig @8.8.8.8 npub.nodns.shop` just works.

4. **DNSSEC end-to-end**: Full chain of trust (Root → .shop → nodns.shop). Verified across Google, Cloudflare, Quad9 resolvers.

5. **Nostr-native**: Leverages existing Nostr infrastructure (relays, keypairs, clients). Any Nostr user already has the tools to create DNS records.

6. **Cashu anti-spam**: Micropayment-based spam prevention without requiring on-chain transactions. 250 sats per record — cheaper than ENS gas fees.

7. **Privacy-by-default**: Ephemeral keypairs in the frontend. No email, no account, no KYC.

8. **Custom delegation**: Users can delegate named subdomains (e.g., `alice.nodns.shop`) to other Nostr pubkeys.

---

## Gaps to Address

| Gap | Priority | Notes |
|---|---|---|
| **Readability** | High | `npub1abc...nodns.shop` is ugly. Custom names (delegation) help but require payment/setup. Consider short aliases or NIP-05 integration. |
| **Zone centralization** | High | Single zone operator (you) runs bot + Knot DNS. If it goes down, all records are unreachable. Consider multi-bot federation. |
| **TLD dependency** | Medium | Depends on `.shop` (ICANN-controlled). Handshake-style alternative root is not addressed. |
| **Record expiry** | Medium | No automatic record expiry. Records persist until overwritten. Consider TTL-based expiry or renewal events. |
| **Multi-bot / federation** | Medium | Single bot is a SPOF. Protocol supports multiple bots (registrar tags exist) but not deployed. |
| **SLD management** | Medium | Users can only manage records under their npub subdomain or delegated names. Can't manage arbitrary zones. |
| **Browser/OS integration** | Low | No browser extension or OS-level integration. Users must use `dig` or the web UI. |
| **Reverse DNS** | Low | No PTR record support. Not critical for the target use case. |
| **DDoS resilience** | Low | Single authoritative server. Consider anycast or multiple authoritative servers. |
| **Audit trail** | Low | Records are stored in SQLite. No public audit trail beyond Nostr events themselves. |

---

## Positioning Statement

NoDNS is the **fastest, simplest way to create DNS records without an account**. It doesn't try to replace DNS or create a parallel namespace — it makes DNS management accessible through Nostr events. Its closest analog is nip.io (magic DNS from hostnames) but with stateful records, DNSSEC, and a payment model.

**Where NoDNS wins**: Developer tooling, testing, ephemeral DNS needs, Nostr ecosystem integration, privacy-first DNS management.

**Where NoDNS doesn't compete**: Human-readable domain names (ENS/UD win), decentralized root zone (Handshake wins), production hosting at scale (Cloudflare/Route53 win).

**Adjacent opportunities**: 
- NIP-05 hosting (alice@example.com → npub) — natural extension
- Nostr identity → DNS bridge (use NIP-05 to get readable name, NoDNS for the records)
- Dev/staging DNS automation (CI/CD publishes events, DNS records appear)
- Tor .onion → human-readable name mapping via TXT records
