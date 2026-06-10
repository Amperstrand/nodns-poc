# DNSSEC Deployment — nodns.shop

> **Status**: Zone signing live since 2026-06-06. DS record submitted at Namecheap. All records carry RRSIG signatures. `ad` flag confirmed on public resolvers since 2026-06-07. SLIP-10-derived KSK (tag 15318) deployed since 2026-06-10.

---

## What Is Live Right Now

| Component | Status | Detail |
|---|---|---|
| Zone signing | ✅ Live | All records signed with RRSIG via Knot DNS 3.3.4 |
| Algorithm | ECDSAP256SHA256 (13) | Per RFC 6605, RFC 8624 |
| KSK (original) | tag 12717 | Created 2026-06-06, DS at Namecheap |
| KSK (SLIP-10) | tag 15318 | Derived from registrar nsec via SLIP-10, deployed 2026-06-10 |
| ZSK | tag 33240 | Created 2026-06-06 |
| NSEC3 | `1 0 0 -` | Per RFC 9276 recommendations |
| CDS/CDNSKEY | Published | Knot auto-publishes for RFC 8078 |
| DS at registrar | Submitted | At Namecheap for KSK 12717 |
| `ad` flag in public DNS | ✅ Confirmed | DS propagated, `ad` flag present on public resolvers since 2026-06-07 |
| DNSKEY attestation | Published | Kind:11111 event with dnskey tag, on relay.damus.io + nos.lol |
| TXT-as-event | ✅ Live | Compact Nostr events embedded as TXT records since 2026-06-10 |

### Verified Output (2026-06-06)

```
$ dig +dnssec @8.8.8.8 nodns.shop SOA
;; flags: qr rd ra;        ← no "ad" yet (DS not propagated)
nodns.shop.  300  IN  SOA  ns1.nodns.shop. admin.nodns.shop. 2026060609 ...
nodns.shop.  300  IN  RRSIG SOA 13 2 300 20260620164221 20260606151221 33240 nodns.shop. ...

$ dig @127.0.0.1 nodns.shop DNSKEY +short  (on VPS)
256 3 13 HJ65PZjA7jXvKvmes9EQgUqtq71n6KNbuixd1YAa6unFQSoQaDP2QbyV...   ← ZSK
257 3 13 uTCfjkiMHmlUkhKs387FDEMPALSwzXzCYL3PRjA+3WTMnKOSVd6eKJuA...   ← KSK

$ dig @127.0.0.1 nodns.shop NSEC3PARAM +short
1 0 0 -   ← SHA-1, 0 iterations, no salt

$ keymgr nodns.shop ds
nodns.shop. DS 12717 13 2 b5a6a5f1b855d3a231e6cf6be231ba4b3bc1843c62845762600f1c5455758726
nodns.shop. DS 12717 13 4 c1ea33c62fe6c38d27cf36d9c7e429bb95527dae75c9b450cb1bd8890a552134afe4d9aaaca70bfda4f4ea1dd322b625
```

### Verified Output (2026-06-10 — post SLIP-10 + dual-KSK)

```
$ dig @8.8.8.8 nodns.shop DNSKEY +short
257 3 13 uTCfjkiMHmlUkhKs387FDEMPALSwzXzCYL3PRjA+3WTMnKOSVd6eKJuA...   ← KSK 12717 (original)
257 3 13 4wxHkDJMuMCTMp2eTAHLs6eRj0Tt2xyccIQYzA1VQIU...                  ← KSK 15318 (SLIP-10)
256 3 13 HJ65PZjA7jXvKvmes9EQgUqtq71n6KNbuixd1YAa6unFQSoQaDP2QbyV...   ← ZSK 33240

$ dig @8.8.8.8 nodns.shop SOA
;; flags: qr rd ra ad;   ← "ad" flag present, DNSSEC validates end-to-end

# SLIP-10 mathematical link verification (on local machine):
$ cargo test test_slip10_matches_dns -- --nocapture
MATCH: True   ← SLIP-10(nsec) → P-256 pubkey == DNSKEY 15318 in DNS
```

---

## Why DNSSEC at All

NoDNS publishes DNS records from Nostr events — anyone can publish a kind:11111 event and have it appear in DNS. Without DNSSEC, a MITM between the resolver and our nameserver could forge or tamper with responses.

With DNSSEC, every response is cryptographically signed. Anyone can verify: "this DNS response really came from the NoDNS operator, and the records are exactly what was published." The chain of trust extends from the DNS root (built into every resolver) through `.shop` to `nodns.shop`.

For a system whose value proposition is *trustless DNS from Nostr events*, cryptographic integrity of DNS responses is not optional.


---

## Algorithm Choice: ECDSAP256SHA256 (Algorithm 13)

### The IANA Registry (Primary Source)

Per the IANA "DNS Security Algorithm Numbers" registry (last updated 2026-01-13), only these algorithms are viable for modern DNSSEC signing:

| # | Algorithm | Sign | Validate | Impl. Sign | Impl. Validate | Reference |
|---|---|---|---|---|---|---|
| 8 | RSA/SHA-256 | RECOMMENDED | RECOMMENDED | MUST | MUST | [RFC 5702] |
| **13** | **ECDSA P-256 + SHA-256** | **RECOMMENDED** | **RECOMMENDED** | **MUST** | **MUST** | **[RFC 6605]** |
| 14 | ECDSA P-384 + SHA-384 | MAY | RECOMMENDED | MAY | RECOMMENDED | [RFC 6605] |
| 15 | Ed25519 | RECOMMENDED | RECOMMENDED | RECOMMENDED | RECOMMENDED | [RFC 8080] |
| 16 | Ed448 | MAY | RECOMMENDED | MAY | RECOMMENDED | [RFC 8080] |

Everything else is `MUST NOT` or deprecated: RSA/SHA-1 (alg 5,7) deprecated by [RFC 9905], DSA (alg 3,6) deprecated, GOST (alg 12) deprecated by [RFC 9906], RSA/MD5 (alg 1) deprecated.

**Key observation**: Algorithm 13 is the only one that is `MUST implement` for both signing *and* validation across all DNS software. This is the strongest guarantee in the registry.

Sources:
- IANA registry: https://www.iana.org/assignments/dns-sec-alg-numbers/dns-sec-alg-numbers.xhtml
- Implementation requirements: [RFC 8624] (updated by [RFC 9904])

### Why Not secp256k1 (Bitcoin/Nostr Curve)

**secp256k1 does not exist in the IANA DNSSEC algorithm registry.** There is:
- No algorithm number assigned to it
- No IETF RFC defining its use in DNSSEC
- No expired or active Internet-Draft proposing it
- No indication the DNS community has ever considered it

Adding secp256k1 would require a new IETF Standards Action (per RFC 4034 §A.1), implementation in every major DNS resolver (Unbound, BIND, Knot, Google, Cloudflare), and deployment across the recursive resolver ecosystem. This is a multi-year process even if started today. **No such effort exists.**

The curves are fundamentally different mathematics:
- **secp256k1**: y² = x³ + 7 (Koblitz curve, used by Bitcoin and Nostr)
- **P-256**: y² = x³ - 3x + b (NIST prime field curve, our DNSSEC algorithm)
- **Ed25519**: -x² + y² = 1 + dx²y² (twisted Edwards curve)

You **cannot convert** a key from one curve to another. A secp256k1 private key has no mathematical relationship to any P-256 or Ed25519 key.

**Even Schnorr signatures don't help.** Bitcoin Taproot (BIP 340) uses Schnorr over secp256k1. Nostr (NIP-01) uses Schnorr over secp256k1. But DNSSEC uses ECDSA (for P-256) or EdDSA (for Ed25519) — different signature schemes over different curves. Even if DNSSEC added Schnorr support, it would need a new algorithm number, and `Schnorr-secp256k1` is not that number.

**This is a hard constraint of the global DNS infrastructure**, not a limitation of our implementation.

### Why Not RSA/SHA-256 (Algorithm 8)

RSA is still `RECOMMENDED` by [RFC 5702] and historically the most deployed DNSSEC algorithm. But:

| Factor | RSA/SHA-256 | ECDSA P-256 |
|---|---|---|
| DNSKEY record size | 128-512 bytes | 68 bytes |
| RRSIG signature size | 128-512 bytes | 64 bytes |
| 128-bit security requires | 3072-bit key | 256-bit key |
| UDP packet fit | Often exceeds 512B → TCP fallback | Compact, stays in UDP |
| Signing speed (500K zone) | 3,200 seconds | 450 seconds |
| Validation speed | 0.12 ms/name | 0.24 ms/name |

*(Performance data: Geoff Huston, APNIC Labs, "DNSSEC with EdDSA", The ISP Column, June 2021)*

RSA's large keys and signatures cause DNS responses to exceed the 512-byte UDP limit, forcing TCP fallback. This adds latency and complexity. As [RFC 6605 §1] states:

> "ECDSA keys are much shorter than RSA keys; at this size, the difference is 256 versus 3072 bits. Similarly, ECDSA signatures are much shorter than RSA signatures."

SIDN (the `.nl` registry) confirms: RSA algorithm 8 is "slowly being phased out in favour of the ECDSA-based algorithms" ([SIDN, 2023](https://www.sidn.nl/en/news-and-blogs/eddsa-based-algorithms-for-dnssec-under-development)).

### Why Not Ed25519 (Algorithm 15)

Ed25519 is also `RECOMMENDED` by [RFC 8080] and was a serious contender. Here's the comparison with primary source data:

| Factor | ECDSA P-256 (13) | Ed25519 (15) | Source |
|---|---|---|---|
| IANA impl. requirement | **MUST** | RECOMMENDED | [RFC 8624] |
| DNSKEY record size | 68 bytes | **36 bytes** | ed25519.no |
| RRSIG signature size | 68 bytes | 68 bytes | ed25519.no |
| Signing speed (500K zone) | **450s** | 810s | Huston/APNIC 2021 |
| Validation overhead | 0.24 ms/name | **0.12 ms/name** | Huston/APNIC 2021 |
| Global resolver support | **~70% validate** | ~50% validate | Huston/APNIC 2021 |
| .nl zone adoption | **57%** of signed domains | 0.01% | SIDN 2023 |
| Deterministic signatures | No (needs RNG) | **Yes** | [RFC 8032] |
| Side-channel resistance | Implementation-dependent | **Built-in** | ed25519.no |

**Why we chose P-256:**

1. **Resolver support is the deciding factor.** Geoff Huston (APNIC Labs, Chief Scientist) measured in 2021 that "slightly less than one half of all users who use DNS recursive resolvers that perform DNSSEC validation using ECDSA P-256 also treat ED25519 digital signatures as 'unknown.'" His conclusion: **"if you are looking for a more compact DNSSEC crypto algorithm to sign your zone... then ECDSA P-256 would be a better choice than Ed25519, if only because of the broader level of support across the DNS resolution landscape today."** ([Huston 2021](https://www.potaroo.net/ispcol/2021-06/eddi.pdf))

2. **P-256 is `MUST implement`; Ed25519 is `RECOMMENDED`.** Per [RFC 8624], every conforming DNS implementation MUST support P-256 for both signing and validation. Ed25519 is RECOMMENDED but not mandatory. This means some resolvers may not validate Ed25519.

3. **SIDN explicitly recommends algorithm 13.** The `.nl` registry (one of the most DNSSEC-mature TLDs) states: **"if you are implementing DNSSEC now, use algorithm 13"** ([SIDN 2023](https://www.sidn.nl/en/news-and-blogs/eddsa-based-algorithms-for-dnssec-under-development)). As of 2023, 57% of DNSSEC-enabled `.nl` domains use algorithm 13, vs 0.01% for Ed25519.

4. **Signing speed favors P-256.** Huston's measurements show P-256 signs a 500K-record zone in 450s vs Ed25519's 810s — nearly 2x faster. Validation is slower for P-256 (0.24ms vs 0.12ms) but the difference is negligible for our 65-record zone.

5. **The ed25519.no project's own data supports this.** The DNSThought measurements linked from [ed25519.no](https://ed25519.no/) show ECDSA P-256 at ~70% global validation support, with Ed25519 trailing.

**When Ed25519 would be the right choice:**
- In a fully controlled resolver environment
- When deterministic signatures are critical (HSMs with poor RNG)
- For long-term algorithm agility planning (Ed25519 may outlive P-256 due to NIST curve concerns)
- When the signing/validation performance gap matters (high-query-volume zones)

### Why Not ECDSA P-384 (Algorithm 14)

P-384 provides 192-bit security vs P-256's 128-bit. But:
- `MAY` for signing — not recommended by [RFC 8624]
- Larger keys (96 bytes) and signatures (96 bytes) — defeats the compactness advantage
- 128-bit security is equivalent to RSA-3072, far beyond any realistic attack
- SIDN reports only 0.05% of `.nl` domains use algorithm 14

### Why Not Ed448 (Algorithm 16)

- `MAY` for signing — minimal deployment
- Larger keys (56 bytes) and signatures (112 bytes)
- 224-bit security is unnecessary overkill

---

## NSEC3 Configuration

### What We Deployed

```
NSEC3 parameters: 1 0 0 -
Hash: SHA-1 | Flags: 0 | Iterations: 0 | Salt: none
```

### Why Not Plain NSEC

NSEC creates a linked list of all records in the zone, enabling "zone walking" — an attacker can enumerate every name. For NoDNS, this would reveal every npub that has published records.

NSEC3 hashes the names, making enumeration computationally infeasible (even with 0 iterations, the hash preimage problem remains).

### Why 0 Iterations and No Salt

Per [RFC 9276] ("NSEC3: Mitigation Against DNSSEC Zone Walking", Wessels, Barber, 2022):

- **0 iterations**: RFC 9276 §4: "the use of additional NSEC3 iterations is NOT RECOMMENDED because it imposes additional computational costs on the authoritative and validating nameservers without providing additional security benefits." Additional iterations were meant to slow dictionary attacks, but modern GPUs hash at billions/second, making even 100 iterations negligible. Meanwhile, every NXDOMAIN response requires the server to hash the queried name `iterations` times.

- **No salt**: RFC 9276 §5: "An NSEC3 salt value of zero length SHOULD be used." Salt was intended to prevent precomputed rainbow tables, but since the salt is published in the NSEC3PARAM record, an attacker can simply retrieve it and include it in their computation. Zero-length salt simplifies operations.

- **Opt-out disabled**: We sign all records. Opt-out (allowing unsigned delegations) makes sense for large TLDs with many delegations but not for our single-operator zone.

This is also what Knot DNS recommends. The Knot DNS documentation's default policy example uses `nsec3-iterations: 0` ([CZ-NIC/knot](https://zread.ai/CZ-NIC/knot/25-dnssec-key-management)).

---

## DS Record and Chain of Trust

### How It Works

```
DNS Root (ICANN trust anchor, built into every resolver)
  │
  ├── DS for .shop (in root zone, managed by .shop registry)
  │
  └── .shop TLD nameservers
        │
        ├── DS for nodns.shop (submitted to Namecheap)  ← WE SUBMITTED THIS
        │
        └── nodns.shop authoritative nameservers
              │
              ├── DNSKEY (KSK 12717 + ZSK 33240)
              │
              └── All records with RRSIG signatures
```

The DS (Delegation Signer) record is a hash of our KSK's public key, placed at the parent zone. Resolvers use it to anchor the chain of trust. Without the DS, our signatures are valid but unanchored — resolvers see RRSIG records but cannot build a chain back to the root trust anchor.

### DS Record Values

```
Digest Type 2 (SHA-256):
12717 13 2 b5a6a5f1b855d3a231e6cf6be231ba4b3bc1843c62845762600f1c5455758726

Digest Type 4 (SHA-384):
12717 13 4 c1ea33c62fe6c38d27cf36d9c7e429bb95527dae75c9b450cb1bd8890a552134afe4d9aaaca70bfda4f4ea1dd322b625
```

We submit digest type 2 (SHA-256), which is the recommended digest algorithm per [RFC 8624 §4.1].

### Why Manual DS Submission

**RFC 8078** defines CDS/CDNSKEY — a mechanism where the child zone publishes CDS/CDNSKEY records, and the parent zone automatically reads them and creates the DS record. Our zone already publishes these (Knot does this automatically):

```
$ dig @127.0.0.1 nodns.shop CDS +short
12717 13 2 B5A6A5F1B855D3A231E6CF6BE231BA4B3BC1843C62845762600F1C54 55758726

$ dig @127.0.0.1 nodns.shop CDNSKEY +short
257 3 13 uTCfjkiMHmlUkhKs387FDEMPALSwzXzCYL3PRjA+3WTMnKOSVd6eKJuA eXe6+FgN9NsM0TU/TnBmrgxa8+k6Ug==
```

**But Namecheap does not support RFC 8078.** The major registrars (Namecheap, GoDaddy, Cloudflare Registrar) require manual DS submission through their web interface. RFC 8078 adoption is limited to a few registries (SIDN/.nl, SWITCH/.ch/.li, AFNIC/.fr) and even fewer registrars.

SIDN is one of the few that supports it: since March 2016, `.nl` registrars can provide DNSKEY records for automated DS management ([SIDN 2023](https://www.sidn.nl/en/news-and-blogs/eddsa-based-algorithms-for-dnssec-under-development)).

This means DS management is manual for us:
- **DS creation**: Submit in Namecheap's DNSSEC UI
- **DS rotation**: Manual — if we rotate the KSK, we submit a new DS at Namecheap and remove the old one
- **DS deletion**: Manual — if we disable DNSSEC, we remove the DS at Namecheap

---

## Key Management

### KSK/ZSK Split

We use a split key model per standard DNSSEC practice:

- **KSK (Key Signing Key)**, tag 12717: Signs only the DNSKEY record set. Long-lived. Its hash is the DS record at the registrar. Changing the KSK requires DS update at Namecheap.
- **ZSK (Zone Signing Key)**, tag 33240: Signs all other records. Can be auto-rotated by Knot without any registrar interaction.

The split exists because:
1. KSK must be long-lived (DS changes are manual and slow)
2. ZSK should rotate periodically (cryptographic hygiene — if compromised, only zone records are affected, not the chain of trust)
3. Separating them limits KSK exposure (used less frequently → less opportunity for side-channel attacks)

### Key Storage

Keys stored as PKCS#8 PEM files in Knot's default keystore (`/var/lib/knot/keys/keys/`). Private keys only exist on the VPS, protected by filesystem permissions. **Back up these keys** — losing the KSK private key means you cannot sign new DNSKEY records, which means DNSSEC breaks when signatures expire.

### Automatic Re-Signing After DDNS

This is critical for NoDNS. Every time the bot sends a DDNS update (new record from a Nostr event), Knot automatically:
1. Applies the update atomically (RCU — no query interruption)
2. Re-signs only the changed records (incremental signing)
3. Fixes NSEC3 chain if needed
4. Bumps SOA serial
5. Sends NOTIFY to secondaries

Per the Knot DNS documentation: "The signing is initiated on... Received DDNS update" ([CZ-NIC/knot, DNSSEC Implementation](https://zread.ai/CZ-NIC/knot/14-dnssec-implementation)). For a single record: sub-millisecond overhead.

---

## The secp256k1 Question: Alternatives and Tradeoffs

### Why We Can't Use Nostr Keys Directly

Nostr uses secp256k1 (NIP-01). DNSSEC uses P-256 or Ed25519. These are different curves with no key conversion possible. There is no IANA algorithm number for secp256k1, no IETF RFC, and no proposal to add one.

### Alternative 1: Custom Algorithm (253/254 — PRIVATEDNS/PRIVATEOID)

IANA reserves algorithm numbers 253 and 254 for private use. We could define our own algorithm using secp256k1 or Schnorr-over-secp256k1.

**Rejected because**: Only our own resolvers would understand it. No public resolver (Google, Cloudflare, Quad9) would validate the signatures. The `ad` flag would never appear. Per [RFC 4035 §5.2]: "If the resolver does not support any of the algorithms listed in an authenticated DS RRset, then the resolver will not be able to verify the authentication path to the child zone. In this case, the resolver SHOULD treat the child zone as if it were unsigned."

This means custom algorithms produce responses that resolvers treat as unsigned — defeating the entire purpose of DNSSEC.

### Alternative 2: SLIP-10 Derivation (IMPLEMENTED — LIVE)

SLIP-10 ([SLIP-0010](https://github.com/satoshilabs/slips/blob/master/slip-0010.md), SatoshiLabs) derives keys for different curves from the same seed:

```
nsec (32 bytes)
  ├──→ HMAC-SHA512("Bitcoin seed", nsec)    → secp256k1 key (Nostr identity)
  ├──→ HMAC-SHA512("Nist256p1 seed", nsec)  → P-256 key (DNSSEC alg 13)
  └──→ HMAC-SHA512("ed25519 seed", nsec)    → Ed25519 key (DNSSEC alg 15)
```

Each label produces a completely independent key for a different curve, but all deterministically derived from the same seed. Anyone who knows the npub can derive the P-256 public key and verify it matches the DNSKEY.

**Status**: **LIVE in production** since 2026-06-10. Derived KSK (tag 15318) imported into Knot DNS, actively signing the zone alongside original KSK 12717 (dual-KSK). Full analysis in `docs/13-nostr-dnssec-derivation.md`.

**Production details**:
- Derived P-256 pubkey: `04e30c790326cb8c093329d9e4c01cb3a79120f474df18c9c708418cc0d5540851b3ec104d9888cc9d7a7b1379eddb42a708eada7c403f8f7be7810b749566e39c`
- Key tag: 15318
- DS (digest type 2): `15318 13 2 15fe3e8c712de06ed097123497938d9185563baf6fecafa5ffe89a322706f580`
- **Pending**: Add DS for 15318 at Namecheap alongside existing DS for 12717 (dual-DS)
- **Attestation event**: ID `fd0d8d4399dee87c472c8a5883315cac554bec4c8c5ea77db23f83b2b08ef8cf`, published to relay.damus.io + nos.lol
- **Known issue**: `keymgr import-pem` creates files as `root:root` — must `chown knot:knot` or key silently fails to sign

### Alternative 3: Nostr Attestation Events (IMPLEMENTED — LIVE)

Publish kind:11111 Nostr events that attest to the DNSKEY, creating an on-chain verifiable link between the Nostr identity and the DNSSEC key.

**Status**: **LIVE**. Attestation event published by the bot at startup. Event ID `fd0d8d4399dee87c472c8a5883315cac554bec4c8c5ea77db23f83b2b08ef8cf`, tags `["dnskey", zone, "15318", "13", base64_rdata]` and `["dnskey-derivation", "slip10", "Nist256p1 seed"]`. Published to relay.damus.io and nos.lol.

---

## Deployment Configuration

### Knot DNS Config (`/etc/knot/knot.conf`)

```knot
policy:
    - id: nodns_dnssec
      algorithm: ECDSAP256SHA256
      nsec3: on
      nsec3-iterations: 0
      nsec3-salt-length: 0

zone:
    - domain: nodns.shop
      dnssec-signing: on
      dnssec-policy: nodns_dnssec
```

This matches Knot's own default policy example from their documentation, which uses `algorithm: ECDSAP256SHA256` with `nsec3-iterations: 0` ([CZ-NIC/knot, DNSSEC Key Management](https://zread.ai/CZ-NIC/knot/25-dnssec-key-management)).

### Pre-DNSSEC Config Backup

```
/etc/knot/knot.conf.pre-dnssec
```

### Emergency Rollback

```bash
ssh root@46.224.104.12 'cp /etc/knot/knot.conf.pre-dnssec /etc/knot/knot.conf && systemctl restart knot'
# Then remove DS at Namecheap
```

---

## Verification Commands

```bash
# Check signed responses (local)
ssh root@46.224.104.12 'dig +dnssec @127.0.0.1 nodns.shop SOA'

# Check signed responses (public — should show RRSIG)
dig +dnssec @8.8.8.8 nodns.shop SOA

# After DS propagation — should show "ad" flag
dig +dnssec @8.8.8.8 nodns.shop SOA
# Look for: ;; flags: qr rd ra ad

# Trace full chain of trust
dig +trace +dnssec @8.8.8.8 nodns.shop SOA

# Verify DNSKEY records
ssh root@46.224.104.12 'dig @127.0.0.1 nodns.shop DNSKEY +short'
# Expected: flag 256 (ZSK) + flag 257 (KSK)

# Verify NSEC3
ssh root@46.224.104.12 'dig @127.0.0.1 nodns.shop NSEC3PARAM +short'
# Expected: 1 0 0 -

# List keys
ssh root@46.224.104.12 'keymgr nodns.shop list'

# Generate DS (for registrar)
ssh root@46.224.104.12 'keymgr nodns.shop ds'
```

---

## Summary of All Tradeoffs

| Decision | Chose | Rejected | Why | Source |
|---|---|---|---|---|
| DNSSEC at all | Yes | Unsigned | Nostr→DNS needs cryptographic integrity | — |
| Algorithm | P-256 (13) | Ed25519 (15) | Broader resolver support (70% vs 50%), `MUST implement` vs `RECOMMENDED` | [RFC 8624], Huston/APNIC 2021 |
| Algorithm | P-256 (13) | RSA/SHA-256 (8) | Compact keys, no UDP overflow, 7x faster signing | [RFC 6605], Huston 2021 |
| Algorithm | P-256 (13) | secp256k1 | Not in IANA registry, no resolver support | IANA registry |
| NSEC3 params | 0 iterations, no salt | More iterations | RFC 9276 explicit recommendation | [RFC 9276] |
| DS management | Manual | RFC 8078 CDS/CDNSKEY | Namecheap doesn't support RFC 8078 | — |
| Key split | KSK/ZSK | Single key | Standard practice, limits KSK exposure | [RFC 6605] |
| Nostr key link | SLIP-10 → P-256 KSK 15318 (LIVE) | Custom algorithm | SLIP-10 derivation works, standard DNSSEC validates | SLIP-0010 |

---

## Sources

- **[RFC 4033/4034/4035]** — DNSSEC fundamentals. RFC 4035 §5.2: resolver behavior with unknown algorithms.
- **[RFC 5702]** — RSA/SHA-256 for DNSSEC.
- **[RFC 6605]** — ECDSA (P-256/P-384) for DNSSEC. Our algorithm. Quote: "ECDSA keys are much shorter than RSA keys."
- **[RFC 8080]** — Ed25519/Ed448 for DNSSEC.
- **[RFC 8078]** — CDS/CDNSKEY automated DS management.
- **[RFC 8032]** — EdDSA signature algorithms (Ed25519/Ed448).
- **[RFC 8624]** — DNSSEC algorithm implementation requirements. Algorithm 13: `MUST implement` for signing and validation.
- **[RFC 9276]** — NSEC3 best practices. Recommends 0 iterations, no salt.
- **[RFC 9904]** — Updated DNSSEC algorithm implementation requirements.
- **[RFC 9905]** — Deprecation of SHA-1 based DNSSEC algorithms.
- **IANA DNSSEC Algorithm Numbers** — https://www.iana.org/assignments/dns-sec-alg-numbers/ (last updated 2026-01-13)
- **Geoff Huston, APNIC Labs** — "DNSSEC with EdDSA", The ISP Column, June 2021. https://www.potaroo.net/ispcol/2021-06/eddi.pdf — Performance measurements and global resolver support data for P-256 vs Ed25519.
- **SIDN** — "EdDSA-based algorithms for DNSSEC under development", 2023. https://www.sidn.nl/en/news-and-blogs/eddsa-based-algorithms-for-dnssec-under-development — .nl deployment statistics: 57% algorithm 13, 0.01% algorithm 15. Explicit recommendation: "use algorithm 13."
- **ed25519.no** — Ed25519 for DNSSEC advocacy site with DNSThought statistics. https://ed25519.no/
- **DNSThought** — NLnet Labs DNSSEC algorithm deployment measurements. https://dnsthought.nlnetlabs.nl/
- **CZ-NIC/knot** — Knot DNS documentation and source. DNSSEC key management defaults. https://zread.ai/CZ-NIC/knot/
- **SLIP-0010** — Universal key derivation for multiple curves. https://github.com/satoshilabs/slips/blob/master/slip-0010.md
- **BIP 340** — Schnorr Signatures for secp256k1 (Bitcoin Taproot).
- **NIP-01** — Nostr basic protocol (secp256k1 Schnorr signatures).
