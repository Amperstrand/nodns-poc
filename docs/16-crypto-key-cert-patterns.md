# NoDNS: Cryptographic Key Derivation & Certificate Management — Pattern Menu

> **Status**: Research synthesis. Decisions needed before implementation.
> **Date**: 2026-06-07

## Table of Contents

1. [NoDNS Constraints](#constraints)
2. [Menu of Options (9 patterns)](#menu)
3. [Comparison Matrix](#matrix)
4. [Top 3 Recommendations](#recommendations)
5. [PoC Design](#poc-design)
6. [Research Sources](#sources)

---

## Constraints <a name="constraints"></a>

NoDNS operates under these fixed constraints:

| Constraint | Detail |
|---|---|
| **User keypair** | secp256k1 (nsec/npub), nsec never leaves browser |
| **Bot sees** | Only the pubkey from Nostr events |
| **Current record types** | A, AAAA, CNAME, TXT, MX, SRV |
| **DNSSEC** | Zone signed with ECDSAP256SHA256, `ad` flag confirmed |
| **TLSA support** | Not yet — Knot DNS would need TLSA record handling |
| **Browser reality** | No mainstream browser validates DANE/TLSA natively |
| **Frontend** | Next.js, nostr-tools, ephemeral keypair by default |

---

## Menu of Options <a name="menu"></a>

### Option A: TXT Record with Self-Signed Certificate Hash

**Description**: User generates a self-signed X.509 cert in the browser (P-256 key). The SHA-256 hash of the cert (or SPKI) is published as a TXT record via the existing Nostr event protocol. Verifiers (custom software, not browsers) fetch the TXT record and compare it against the TLS cert presented by the user's server.

**How it works**:
1. Frontend: `WebCrypto.generateKey({name:"ECDSA", namedCurve:"P-256"})` → keypair
2. Frontend: `@peculiar/x509` creates self-signed cert with SAN = `npub1xxx.nodns.shop`
3. Frontend: compute `SHA-256(cert-der)` or `SHA-256(SPKI)`
4. Frontend: publish TXT record via existing Nostr event: `["record", "TXT", "_tls", "3600", "sha256=<hex>"]`
5. Verifier: fetch TXT, compare against server's TLS cert

**Pros**:
- ✅ Works with **existing infrastructure** — no bot changes, no Knot DNS changes
- ✅ Browser-only — WebCrypto + `@peculiar/x509` (or `pkijs`)
- ✅ TXT records universally supported by all resolvers
- ✅ Standard DNSSEC protection on the TXT record

**Cons**:
- ❌ No browser will validate this automatically
- ❌ Custom verifier software required
- ❌ No link to Nostr identity (anyone can publish any hash)
- ❌ No revocation mechanism beyond overwriting the TXT record

**Implementation complexity**: Low (~200 lines frontend)
**Bot changes**: None
**Knot DNS changes**: None
**Security**: ⚠️ DNSSEC authenticates the TXT record, but trust is in the publisher (anyone who controls the npub). No cryptographic link between nsec and the TLS key.

---

### Option B: SLIP-10 Derived TLS Key + TXT/SPKI Hash

**Description**: User's nsec is used to derive a P-256 key via SLIP-10 (`HMAC-SHA512("Nist256p1 seed", nsec)`). A self-signed cert is created from this derived key. The SPKI hash is published as a TXT record. Because the derivation is deterministic, anyone who knows the npub can verify the SPKI hash matches the expected derived public key — creating a **cryptographic link** between Nostr identity and TLS key.

**How it works**:
1. Frontend: decode nsec → 32 bytes
2. Frontend: `hmac(sha512, "Nist256p1 seed", nsec_bytes)` → IL (private key) + IR (chain code)
3. Frontend: P-256 public key from IL
4. Frontend: `@peculiar/x509` creates self-signed cert with SAN
5. Frontend: compute `SHA-256(SPKI)` of derived P-256 key
6. Frontend: publish TXT record: `["record", "TXT", "_tls", "3600", "nodns-tlsa=3-1-1 <hex>"]`
7. Verifier: derive P-256 public key from npub → compute expected SPKI hash → compare with TXT

**Pros**:
- ✅ **Cryptographic link** between Nostr identity and TLS key
- ✅ Deterministic — same nsec → same TLS key, every time
- ✅ Works with existing infrastructure (TXT record)
- ✅ Anyone can verify: "this TLS key was derived from npub X"
- ✅ SLIP-10 is battle-tested (Trezor, Ledger, millions of devices)

**Cons**:
- ❌ No browser validation of the Nostr↔TLS link
- ❌ nsec compromise = TLS key compromise (shared root of trust)
- ❌ SLIP-10 P-256 derivation not available in standard JS libraries (custom ~80 lines)
- ❌ Custom verifier needed

**Implementation complexity**: Medium (~300 lines frontend — SLIP-10 + cert generation + hash)
**Bot changes**: None (optional: bot could verify the SPKI hash matches expected derivation)
**Knot DNS changes**: None
**Security**: ✅ Cryptographically sound. HMAC-SHA512 is one-way: knowing the P-256 key does NOT reveal nsec.

---

### Option C: SLIP-10 Derived TLS Key + TLSA Record (DANE-EE)

**Description**: Same as Option B, but instead of a TXT record, publish a proper **TLSA record** (RFC 6698) with DANE-EE semantics (`3 1 1` = domain-issued cert, SPKI selector, SHA-256 match). This requires adding TLSA record type support to the bot and Knot DNS.

**TLSA record format**: `_443._tcp.npub1xxx.nodns.shop. TLSA 3 1 1 <sha256-of-spki>`

**How it works**:
1. Steps 1-4 from Option B (derive P-256 key, create self-signed cert)
2. Frontend: compute SHA-256 of SPKI DER
3. Frontend: publish via new tag format: `["record", "TLSA", "_443._tcp", "3 1 1 <hex>", "3600"]`
4. Bot: parse TLSA record, push DDNS UPDATE to Knot DNS
5. Knot DNS: serves TLSA record, DNSSEC-signed
6. DANE-aware clients (Postfix, OpenSSL, custom) validate TLS cert against TLSA

**Pros**:
- ✅ Uses **standard protocol** (DANE, RFC 6698)
- ✅ TLSA records returned by all major resolvers (Google, Cloudflare)
- ✅ DNSSEC-signed → authenticated TLSA data
- ✅ Works with DANE-aware software (OpenSSL `SSL_CTX_dane_enable`, Postfix, Exim)
- ✅ Cryptographic link to Nostr identity via SLIP-10
- ✅ DANE-EE (3.x.x) works with self-signed certs — no PKIX validation needed

**Cons**:
- ❌ **No browser support** — Firefox, Chrome, Safari do not validate DANE
- ❌ Requires **bot changes** (parse TLSA tag, DDNS UPDATE for TLSA type)
- ❌ Requires **Knot DNS changes** (or at minimum: TLSA record support in DDNS)
- ❌ DANE validation requires DNSSEC → TLSA chain (works, but complex)
- ❌ Only useful with custom/DANE-aware clients

**Implementation complexity**: Medium-High (bot: ~150 lines, frontend: same as B, Knot config: TLSA type)
**Bot changes**: Yes — add TLSA record parsing and DDNS support
**Knot DNS changes**: Minimal — TLSA is a standard type, hickory supports it
**Security**: ✅ Strong — DNSSEC + TLSA + SLIP-10 derivation = three-layer trust

**TLSA record size**: SHA-256 SPKI = 32 bytes → hex string ~64 chars. Very small, no UDP issues.

---

### Option D: Ephemeral TLS Key + Nostr Signature (Mini Certificate Chain)

**Description**: Instead of deriving the TLS key from nsec, generate an **ephemeral** P-256 keypair per session/period. Sign the ephemeral public key with the nsec (Schnorr signature over secp256k1). Publish the signed public key as a TXT record. Verifiers check: (1) Nostr signature is valid for this npub, (2) TLS cert matches the signed public key.

**How it works**:
1. Frontend: `WebCrypto.generateKey({P-256})` → ephemeral keypair
2. Frontend: sign the P-256 public key bytes with nsec using Schnorr
3. Frontend: publish TXT record: `["record", "TXT", "_tls", "3600", "nodns-sig=<npub>:<sig>:<spki-hex>"]`
4. Verifier: fetch TXT → verify Schnorr sig with npub → compare SPKI with TLS cert

**Pros**:
- ✅ **No nsec↔TLS shared root of trust** — compromising TLS key doesn't compromise nsec
- ✅ Key rotation is trivial — just publish a new signed key
- ✅ Nostr-native — uses standard Nostr signature verification
- ✅ Works with existing TXT infrastructure
- ✅ No specialized libraries needed (nostr-tools already has Schnorr signing)

**Cons**:
- ❌ TXT record is larger (signature + public key)
- ❌ Custom verifier required
- ❌ No browser validation
- ❌ More complex verification logic (signature check + SPKI match)
- ❌ Ephemeral key management (when to rotate? what about offline validation?)

**Implementation complexity**: Medium (~250 lines frontend)
**Bot changes**: None (optional: bot could verify the Nostr signature)
**Knot DNS changes**: None
**Security**: ✅ Good — no shared root of trust, but relies on TXT record integrity (DNSSEC helps).

---

### Option E: Let's Encrypt ACME DNS-01 Automation

**Description**: NoDNS bot automates the Let's Encrypt ACME DNS-01 challenge. When a user requests a TLS certificate, the bot publishes the `_acme-challenge` TXT record, Let's Encrypt verifies it, and a proper CA-signed certificate is issued. The cert is returned to the user.

**How it works**:
1. User clicks "Get TLS Certificate" in frontend
2. Frontend sends request to bot API (or publishes a Nostr event)
3. Bot: generates ACME account, creates order for `npub1xxx.nodns.shop`
4. Bot: receives DNS-01 challenge, publishes `_acme-challenge.npub1xxx.nodns.shop TXT`
5. Bot: tells Let's Encrypt to verify
6. Let's Encrypt: queries DNS, verifies TXT record, DNSSEC validates it
7. Bot: receives issued certificate, returns it to user (or stores it)
8. Bot: cleans up `_acme-challenge` TXT record

**Pros**:
- ✅ **Browser-trusted** — Let's Encrypt certs are trusted by all browsers
- ✅ Standard WebPKI — no custom verifier needed
- ✅ DNSSEC chain already in place (Let's Encrypt validates via DNS)
- ✅ Automatic renewal possible
- ✅ Free

**Cons**:
- ❌ Requires **significant bot changes** (ACME client, cert management)
- ❌ Let's Encrypt **rate limits** (5 certs/week per domain for duplicate names)
- ❌ `npub1xxx.nodns.shop` = 63-char subdomain limit concerns
- ❌ Bot needs to hold ACME account private key
- ❌ Certificate lifecycle management (renewal, revocation)
- ❌ No cryptographic link to Nostr identity
- ❌ Depends on Let's Encrypt availability

**Implementation complexity**: High (~500+ lines bot, new dependency: `acme2` or similar)
**Bot changes**: Major — ACME client, cert storage, renewal scheduler
**Knot DNS changes**: None (TXT records already supported)
**Security**: ✅ Standard WebPKI security. DNS-01 is the most secure ACME challenge when DNSSEC is present.

**Rate limit note**: Let's Encrypt allows 50 certificates per registered domain per week. With `nodns.shop` as the registered domain, this means 50 unique subdomains per week. For a PoC this is fine; for production, may need careful planning.

---

### Option F: NoDNS Private CA (mkcert-like)

**Description**: NoDNS operates its own Certificate Authority. The CA root certificate is published via a Nostr event (or well-known URL). Users generate CSRs, the NoDNS bot signs them. Users who install the NoDNS CA root in their trust store get browser-trusted TLS.

**How it works**:
1. NoDNS generates a root CA keypair (P-256)
2. Root CA public key published as a Nostr event (attestation)
3. User: generate keypair in browser, create CSR
4. User: publish CSR as a Nostr event (or submit to bot API)
5. Bot: verify user owns the subdomain (via Nostr event signature)
6. Bot: sign CSR with CA key → issue certificate
7. User: install NoDNS CA root in trust store (one-time)

**Pros**:
- ✅ Once CA root is trusted, **all subdomains get browser-trusted TLS**
- ✅ No rate limits (own CA)
- ✅ Cryptographic link to Nostr (CA attested via Nostr event)
- ✅ Can issue wildcards, long-validity certs

**Cons**:
- ❌ **Every user must install the CA root** — huge UX barrier
- ❌ CA private key is a **high-value target** — compromise = trust broken for everyone
- ❌ Significant implementation effort (CA logic, CRL/OCSP, cert lifecycle)
- ❌ Not trusted by default — only useful for managed/enterprise scenarios
- ❌ Mobile browsers make root CA installation difficult

**Implementation complexity**: Very High (CA infrastructure, cert lifecycle, trust store installation guides)
**Bot changes**: Major — CA signing, cert storage
**Knot DNS changes**: None
**Security**: ⚠️ CA model — single point of failure. If CA key is compromised, all trust is broken.

---

### Option G: Cloudflare-Origin-CA Model (Edge Proxy)

**Description**: NoDNS runs an edge proxy (like Cloudflare). The proxy terminates TLS with its own valid cert. The connection from proxy to user's origin uses a self-signed cert (or NoDNS-issued cert). Only the edge needs a trusted cert.

**How it works**:
1. NoDNS edge proxy holds a valid cert for `*.nodns.shop` (from Let's Encrypt wildcard)
2. User's origin server uses a self-signed cert
3. Clients connect to edge → edge validates origin cert against TLSA or known key
4. Edge proxies traffic to origin

**Pros**:
- ✅ **Browser-trusted** — edge has valid wildcard cert
- ✅ Self-signed certs work for origin↔edge
- ✅ Centralized TLS management

**Cons**:
- ❌ NoDNS becomes a **MITM** by design — users must trust the edge
- ❌ Requires running a reverse proxy for all traffic
- ❌ Adds latency and a single point of failure
- ❌ No cryptographic link to Nostr identity
- ❌ Defeats the purpose of decentralized DNS management

**Implementation complexity**: Very High (reverse proxy infrastructure, wildcard cert management)
**Bot changes**: None (this is separate infrastructure)
**Knot DNS changes**: None
**Security**: ⚠️ Centralized trust model — contradicts NoDNS's decentralized ethos.

---

### Option H: Full Cert in DNS (TLSA usage 3 0 0)

**Description**: Publish the **entire self-signed certificate** as a TLSA record with matching type 0 (exact match, no hash). Client retrieves the full cert from DNS and compares it to the TLS cert presented by the server.

**How it works**:
1. User: generate self-signed cert in browser
2. Publish TLSA record with `3 0 0 <full-cert-der-hex>`
3. Client: fetch TLSA, compare full cert

**Pros**:
- ✅ Standard DANE protocol
- ✅ No hash computation needed on verifier side

**Cons**:
- ❌ **TLSA record too large** — a P-256 cert is ~400-500 bytes DER, ~800-1000 hex chars
- ❌ Exceeds DNS UDP message size → forces TCP
- ❌ RFC 7671 §10.1 explicitly warns: "a single certificate is often too large for DNS delivery via UDP"
- ❌ Not recommended by any DANE guidance

**Implementation complexity**: Same as Option C but with larger records
**Security**: ✅ Same as C, but larger records = more DNS surface area

**Verdict**: ❌ Not recommended. Use SPKI hash (3 1 1) instead.

---

### Option I: Hybrid — SLIP-10 Derivation + Nostr-Signed Attestation Event

**Description**: Combines SLIP-10 derivation (Option B) with a dedicated Nostr attestation event. The user publishes a self-signed cert's SPKI hash as a TXT record AND publishes a Nostr event that attests "I, npub X, certify that TLS key Y is mine for domain Z." This creates both a DNS-layer and a Nostr-layer binding.

**How it works**:
1. Frontend: SLIP-10 derive P-256 key from nsec
2. Frontend: create self-signed cert, compute SPKI hash
3. Frontend: publish TXT record (DNS-layer binding)
4. Frontend: publish attestation Nostr event:
   ```json
   {
     "kind": 11111,
     "tags": [
       ["tlsa", "npub1xxx.nodns.shop", "3", "1", "1", "<sha256-hex>"],
       ["tlsa-derivation", "slip10", "Nist256p1 seed", "m"]
     ],
     "content": "TLS key attestation"
   }
   ```
5. Verifier can check EITHER: DNS TXT record OR Nostr event OR both
6. Cross-verification: derive P-256 key from npub → verify SPKI hash matches both sources

**Pros**:
- ✅ **Dual-layer trust** — DNS + Nostr
- ✅ DNSSEC protects DNS layer; Nostr signatures protect Nostr layer
- ✅ If DNS is tampered with, Nostr event still exists as proof
- ✅ If Nostr relays are censored, DNS record still exists
- ✅ Cryptographic link to Nostr identity (SLIP-10)
- ✅ Nostr-native applications can verify without touching DNS

**Cons**:
- ❌ More complex (two publishing steps)
- ❌ Still no browser validation
- ❌ Custom verifier needed (but simpler: just verify Nostr signature + SPKI match)

**Implementation complexity**: Medium (~350 lines total)
**Bot changes**: Minimal — parse `tlsa` tag for logging/verification (optional)
**Knot DNS changes**: None
**Security**: ✅✅ Strongest option — dual-layer, cryptographically linked, censorship-resistant.

---

## Comparison Matrix <a name="matrix"></a>

| | A: TXT Hash | B: SLIP-10 + TXT | C: SLIP-10 + TLSA | D: Ephemeral + Sig | E: ACME DNS-01 | F: Private CA | G: Edge Proxy | I: Hybrid |
|---|---|---|---|---|---|---|---|---|
| **Nostr↔TLS link** | ❌ | ✅ SLIP-10 | ✅ SLIP-10 | ✅ Signature | ❌ | ❌ | ❌ | ✅✅ Dual |
| **Browser trust** | ❌ | ❌ | ❌ (DANE clients only) | ❌ | ✅ | ⚠️ (install root) | ✅ | ❌ |
| **Bot changes** | None | None | Yes (TLSA) | None | Major | Major | None | Minimal |
| **Knot changes** | None | None | Minimal | None | None | None | None | None |
| **Implementation** | Low | Medium | Med-High | Medium | High | V. High | V. High | Medium |
| **DNSSEC leveraged** | ✅ | ✅ | ✅✅ | ✅ | ✅ | N/A | N/A | ✅ |
| **Key rotation** | Easy | Re-derive | Re-derive | Trivial | Auto-renew | Re-issue | N/A | Re-derive |
| **nsec↔TLS coupling** | None | Shared root | Shared root | Decoupled | None | None | None | Shared root |
| **Standard protocol** | Custom | Custom | DANE (RFC 6698) | Custom | ACME (RFC 8555) | X.509 CA | TLS proxy | Custom + Nostr |
| **Censorship resistant** | ✅ DNSSEC | ✅ DNSSEC | ✅ DNSSEC | ✅ DNSSEC | ⚠️ LE dep. | ⚠️ CA dep. | ❌ Centralized | ✅✅ Dual |

---

## Top 3 Recommendations <a name="recommendations"></a>

### 🥇 1st: Option B — SLIP-10 Derived TLS Key + TXT Record

**Why first**: The best balance of cryptographic elegance and implementation simplicity.

- Creates a **mathematical link** between Nostr identity and TLS key
- Uses **existing infrastructure** — no bot or Knot changes needed
- SLIP-10 is well-understood and already implemented in the bot (`dnssec_derivation.rs`)
- Frontend implementation is straightforward with `@noble/hashes` + `@peculiar/x509`
- DNSSEC protects the TXT record, so the SPKI hash is authenticated
- Demonstrates the core idea: "your Nostr identity IS your TLS identity"

**What it proves**: That NoDNS can establish cryptographic bindings between Nostr keys and TLS keys without any infrastructure changes.

### 🥈 2nd: Option E — Let's Encrypt ACME DNS-01 Automation

**Why second**: The only option that gives **actual browser-trusted TLS**.

- Users get real, browser-trusted certificates
- DNSSEC already in place makes DNS-01 the strongest ACME challenge
- NoDNS already controls DNS → automating `_acme-challenge` TXT records is natural
- Rate limits (50/week) are sufficient for a PoC

**What it proves**: That NoDNS can provide practical, browser-trusted TLS as a service. This is the "useful product" path.

### 🥉 3rd: Option I — Hybrid SLIP-10 + Nostr Attestation

**Why third**: The most architecturally complete solution — dual-layer trust.

- DNS + Nostr = censorship-resistant, dual-verified TLS binding
- Demonstrates the full vision: DNSSEC + Nostr signatures + SLIP-10 derivation
- Builds on Option B, adding only a Nostr attestation event
- Nostr-native apps get a verifiable TLS binding without DNS queries
- DNS resolvers get a DNSSEC-protected SPKI hash

**What it proves**: The full "three-layer trust architecture" — mathematical derivation + protocol attestation + standard DNSSEC.

---

## PoC Design <a name="poc-design"></a>

### Recommended PoC: Start with Option B, then add Option I

**Phase 1: SLIP-10 + TXT (Option B) — ~1 day**

#### User Flow
```
1. User visits nodns.shop
2. Enters nsec (or generates ephemeral keypair)
3. Clicks "Generate TLS Certificate"
4. Frontend:
   a. Decodes nsec → 32 bytes
   b. Derives P-256 key via SLIP-10
   c. Creates self-signed X.509 cert (SAN = npub1xxx.nodns.shop)
   d. Computes SHA-256 of SPKI DER
   e. Shows cert details and SPKI hash
5. User clicks "Publish TLS Binding"
6. Frontend publishes Nostr event with TXT record:
   ["record", "TXT", "_tls", "3600", "nodns-tlsa=3-1-1 <sha256-hex>"]
7. TXT record appears at _tls.npub1xxx.nodns.shop within seconds
8. User can download the cert + private key (PEM) for their server
```

#### Technical Steps

**Frontend (new file: `src/lib/tls.ts`)**:
```typescript
// Dependencies: @noble/hashes, @noble/curves, @peculiar/x509

import { hmac } from '@noble/hashes/hmac';
import { sha512 } from '@noble/hashes/sha512';
import { p256 } from '@noble/curves/p256';
import { X509CertificateGenerator } from '@peculiar/x509';

// Step 1: SLIP-10 P-256 master key derivation
function slip10DeriveP256(seed: Uint8Array): { privateKey: Uint8Array; chainCode: Uint8Array } {
  const I = hmac(sha512, 'Nist256p1 seed', seed);
  const IL = I.slice(0, 32);  // private key candidate
  const IR = I.slice(32, 64); // chain code
  // P-256 key validation + retry if needed (~2^-32 chance)
  return { privateKey: IL, chainCode: IR };
}

// Step 2: Get P-256 public key
const derivedKey = slip10DeriveP256(nsecBytes);
const pubKey = p256.getPublicKey(derivedKey.privateKey);

// Step 3: Create self-signed cert
// Use @peculiar/x509 or WebCrypto + manual ASN.1

// Step 4: Compute SPKI hash
const spkiDer = /* extract from cert or compute */;
const spkiHash = sha256(spkiDer);

// Step 5: Publish TXT record via existing nostr.ts
```

**Bot changes**: None.

**Verification**:
```bash
# Query the TXT record
dig _tls.npub1xxx.nodns.shop TXT +dnssec +short
# "nodns-tlsa=3-1-1 abc123..."

# Verify SPKI hash matches derived key
# (custom script: npub → SLIP-10 → P-256 public key → SHA-256(SPKI) → compare)
```

**Phase 2: Add Nostr Attestation (Option I) — ~2 hours**

Add a second Nostr event alongside the TXT record:
```json
{
  "kind": 11111,
  "tags": [
    ["record", "TXT", "_tls", "3600", "nodns-tlsa=3-1-1 <hash>"],
    ["tlsa", "npub1xxx.nodns.shop", "3", "1", "1", "<hash>"],
    ["tlsa-derivation", "slip10", "Nist256p1 seed", "m"]
  ],
  "content": "TLS key binding"
}
```

This costs almost nothing to add and gives the dual-layer property.

---

### Alternative PoC: Option E (ACME DNS-01) — ~2-3 days

If browser-trusted TLS is the priority:

```
1. Bot: add ACME client (Rust crate: `acme2` or `rustls-acme`)
2. User: clicks "Get Browser-Trusted Certificate"
3. Bot: 
   a. Creates ACME order for npub1xxx.nodns.shop
   b. Gets DNS-01 challenge
   c. Publishes _acme-challenge.npub1xxx.nodns.shop TXT
   d. Tells LE to verify
   e. Receives cert
   f. Returns cert + key to user (encrypted, or user provides CSR)
4. User: installs cert on their server
```

**Key concern**: npub subdomains can be 59+ chars. Let's Encrypt accepts these, but the full FQDN (`npub1ykal2pa3dl.nodns.shop`) is ~70 chars — well within DNS limits (253).

---

## Key Technical Details

### DANE/TLSA Quick Reference (RFC 6698)

**TLSA RDATA fields**:
| Field | Size | Values |
|---|---|---|
| Certificate Usage | 1 byte | 0=PKIX-CA, 1=PKIX-EE, 2=DANE-CA, **3=DANE-EE** |
| Selector | 1 byte | **0=Full cert**, **1=SPKI** |
| Matching Type | 1 byte | 0=Exact, **1=SHA-256**, 2=SHA-512 |
| Cert Assoc Data | variable | Hash or full data |

**DANE-EE (3.1.1)** — recommended for self-signed certs:
- No PKIX validation needed
- No certificate chain verification
- No expiration check (RFC 7671: "DANE-EE is specifically designed for self-signed certificates")
- Just: does the SPKI SHA-256 match?

**TLSA naming convention**:
```
_<port>._<protocol>.<fqdn>
_443._tcp.npub1xxx.nodns.shop.   TLSA  3 1 1  <sha256-hex>
```

**DNSSEC requirement**: TLSA records are only trustworthy when DNSSEC-validated. NoDNS already has DNSSEC → ✅ this works.

**Browser support**: None. Firefox (Bug 1201841 — closed WONTFIX), Chrome (no support), Safari (no support). Only Postfix, OpenSSL, Exim, and custom software.

### SLIP-10 Derivation (P-256)

**Formula**:
```
I = HMAC-SHA512(key="Nist256p1 seed", data=nsec_bytes)
IL = I[0:32]  → P-256 private key candidate
IR = I[32:64] → chain code

Validate: parse256(IL) != 0 AND parse256(IL) < n (P-256 order)
If invalid (~2^-32): re-derive with incremented data
```

**Test vector** (from SLIP-0010):
```
Seed: 000102030405060708090a0b0c0d0e0f
P-256 private: 612091aaa12e22dd2abef664f8a01a82cae99ad7441b7ef8110424915c268bc2
```

**JS libraries needed**:
- `@noble/hashes` — HMAC-SHA512
- `@noble/curves` — P-256 key operations
- `@peculiar/x509` — X.509 certificate generation (browser-compatible)
- Or `pkijs` — alternative X.509 library (mature, WebCrypto-native)

### Browser Cert Generation Stack

**Recommended**: `@peculiar/x509` (modern, active, ~30KB) + WebCrypto

```typescript
import { X509CertificateGenerator } from '@peculiar/x509';
import { crypto } from '@peculiar/webcrypto'; // or native WebCrypto

const alg = { name: 'ECDSA', namedCurve: 'P-256' };
const keys = await crypto.subtle.generateKey(alg, true, ['sign', 'verify']);

const cert = await X509CertificateGenerator.createSelfSigned(
  { name: 'CN=npub1xxx.nodns.shop' },
  keys.publicKey,
  keys.privateKey,
  alg,
  new Date(),
  new Date(Date.now() + 365 * 24 * 60 * 60 * 1000),
  ['digitalSignature', 'keyEncipherment'],
  ['serverAuth'],
  { altNames: [`npub1xxx.nodns.shop`] }
);
```

**Certificate fields for TLS**:
- **SAN (Subject Alternative Name)**: `dNSName: npub1xxx.nodns.shop` — **required** for modern TLS
- **CN**: Legacy fallback only; modern clients ignore it
- **keyUsage**: `digitalSignature`
- **extendedKeyUsage**: `serverAuth` (OID 1.3.6.1.5.5.7.3.1)
- **basicConstraints**: `CA:FALSE`
- **Validity**: 30-365 days reasonable for self-signed

---

## Sources <a name="sources"></a>

### Standards
- **RFC 6698** — TLSA / DANE: https://www.rfc-editor.org/rfc/rfc6698
- **RFC 7671** — DANE operational guidance: https://www.rfc-editor.org/rfc/rfc7671
- **RFC 4255** — SSHFP (SSH fingerprint in DNS): https://www.rfc-editor.org/rfc/rfc4255
- **RFC 7929** — OPENPGPKEY: https://www.rfc-editor.org/rfc/rfc7929
- **RFC 8162** — SMIMEA: https://www.rfc-editor.org/rfc/rfc8162
- **RFC 4398** — CERT record types: https://www.rfc-editor.org/rfc/rfc4398
- **RFC 8555** — ACME (Let's Encrypt): https://www.rfc-editor.org/rfc/rfc8555
- **SLIP-0010** — Universal HD key derivation: https://github.com/satoshilabs/slips/blob/master/slip-0010.md

### Libraries
- `@peculiar/x509`: https://github.com/PeculiarVentures/x509
- `pkijs`: https://github.com/PeculiarVentures/PKI.js
- `node-forge` (legacy): https://github.com/digitalbazaar/forge
- `@noble/hashes`: https://github.com/paulmillr/noble-hashes
- `@noble/curves`: https://github.com/paulmillr/noble-curves
- `micro-key-producer`: https://github.com/paulmillr/micro-key-producer

### Implementations
- Handshake DANE (HIP-0017): https://github.com/handshake-org/HIPs/blob/master/HIP-0017.md
- ENS DNS Registrar: https://github.com/ensdomains/ens-contracts
- Cloudflare Origin CA: https://developers.cloudflare.com/ssl/origin-configuration/origin-ca/
- mkcert: https://github.com/FiloSottile/mkcert
- OpenSSL DANE APIs: `SSL_CTX_dane_enable`

### Existing NoDNS docs
- `docs/13-nostr-dnssec-derivation.md` — SLIP-10 research (DNSSEC key derivation)
- `docs/15-nsec-to-dnssec-analysis.md` — nsec→DNSSEC tradeoff analysis
- `nodns-bot-rs/src/dnssec_derivation.rs` — SLIP-10 P-256 implementation (Rust)
