# NoDNS Client-Side TLS Key Derivation — Security Model

> **Status**: Production design. Documented post-implementation.
> **Date**: 2026-06-07
> **Related**: [docs/17-acme-dns01-trust-analysis.md](17-acme-dns01-trust-analysis.md), [docs/15-nsec-to-dnssec-analysis.md](15-nsec-to-dnssec-analysis.md)

## Overview

NoDNS provides browser-trusted HTTPS certificates for Nostr subdomains (`npub1xxx.nodns.shop`) via Let's Encrypt ACME DNS-01. The security goal: TLS private keys are derived deterministically from the user's nsec and never leave the browser. The bot only ever sees the Certificate Signing Request (CSR) containing the public key, never the private key.

This gives users control over their TLS keys while still leveraging the NoDNS bot's ACME automation.

---

## The Problem with Traditional ACME

In a traditional ACME DNS-01 flow, the server generates the private key pair:

```
User → "I want a cert"
Bot → Generate RSA/ECDSA keypair (random)
Bot → Store private key (even temporarily)
Bot → Generate CSR from key
Bot → Submit CSR to Let's Encrypt
Bot → Return private key + certificate to user
```

**The security issue**: The bot has access to the private key at some point. Even if wiped after delivery, the key exists in memory and potentially in logs. If the bot is compromised, all historical TLS private keys could be exposed.

This breaks the Nostr principle of user-controlled identity.

---

## Our Solution: Client-Side Key Derivation + CSR

The private key is derived deterministically from the user's nsec, and the CSR is generated entirely in the browser:

```
Browser                                    NoDNS Bot                          Let's Encrypt
  │                                           │                                   │
  │ Derive TLS key from nsec + subdomain      │                                   │
  │ (deterministic: nsec never transmitted)   │                                   │
  │                                           │                                   │
  │ Generate CSR from derived key             │                                   │
  │ (contains ONLY the public key)            │                                   │
  │                                           │                                   │
  │ POST /api/acme/order                      │                                   │
  │   { domain: "npub1xxx.nodns.shop"         │                                   │
  │     csr_der: "base64-encoded-csr" }       │                                   │
  │ ─────────────────────────────────────────►│                                   │
  │                                           │ Create ACME order ──────────────►│
  │                                           │ Receive DNS-01 challenge ◄───────│
  │                                           │                                   │
  │                                           │ Publish _acme-challenge TXT      │
  │                                           │ (via DDNS UPDATE, DNSSEC-signed) │
  │                                           │                                   │
  │                                           │ Signal verification ─────────────►│
  │                                           │                                   │
  │                                           │ Submit CSR ─────────────────────►│
  │                                           │ (from client, contains pubkey)   │
  │                                           │                                   │
  │                                           │ Receive signed cert ◄────────────│
  │                                           │                                   │
  │ GET /api/acme/order/{id}                  │                                   │
  │ ◄────────────────────────────────────────│                                   │
  │                                           │                                   │
  │ Certificate + derived private key ✅      │ (never saw the private key)       │
```

### Key Points

1. **Private key derivation**: Browser computes `private_key = derive_p256(nsec, subdomain)` using HMAC-SHA512. Deterministic: same nsec + subdomain always produces the same key.

2. **CSR generation**: Browser generates a PKCS#10 CSR from the derived private key. CSR contains the public key, domain name, and signature — NOT the private key.

3. **Bot submission**: Bot receives only the CSR (base64-encoded). It extracts the public key, submits the CSR to Let's Encrypt via ACME DNS-01 challenge.

4. **Certificate issuance**: Let's Encrypt validates the DNS-01 challenge (standard DNS control proof) and issues a certificate for the public key in the CSR.

5. **Key possession**: The client already has the matching private key — no transmission needed.

---

## Derivation Algorithm

### Algorithm Specification

```
Input:
  - nsec_bytes: 32 bytes (secp256k1 private key)
  - subdomain: string (e.g., "npub1abc...")

Algorithm:
  1. Construct derivation data:
     data = nsec_bytes || 0x00 || encode_utf8(subdomain)

  2. Compute HMAC-SHA512:
     I = HMAC-SHA512(key="nodns-tls-v1", data=data)

  3. Extract P-256 private key scalar:
     I_L = I[0:32]  // Left 32 bytes of HMAC output

  4. Validate key (P-256 scalar requirements):
     - I_L must not be zero
     - I_L must be < n (P-256 group order: 0xffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551)

  5. If invalid (~2^-32 probability), prepend 0x00 to subdomain and retry
  6. Output: P-256 private key = I_L
```

### Design Decisions

| Element | Value | Rationale |
|---|---|---|
| **HMAC key** | `"nodns-tls-v1"` | Domain separator from DNSSEC derivation (`"Nist256p1 seed"`). Prevents cross-system key reuse. |
| **Curve** | P-256 (NIST P-256) | Standard for ECDSA certificates. `p256` crate audited. Good browser support. |
| **Subdomain inclusion** | Yes | Ensures different subdomains get different keys. Isolation: if one key is compromised, others remain secure. |
| **Null byte separator** | `0x00` | Prevents ambiguous concatenation if subdomain contains hex that could be misinterpreted. |
| **Validation loop** | Retry on invalid scalar | P-256 scalar must be in `[1, n-1]`. Invalid probability ~2^-32. Rare in practice. |
| **Version tag** | `-v1` | Allows algorithm upgrade without breaking existing derived keys. Future: `nodns-tls-v2` |

### Why HMAC Instead of Raw Hash?

HMAC provides domain separation. Without it, we'd risk collisions if someone uses the same derivation for another purpose. HMAC-SHA512's security bound is 256 bits as a PRF ([RFC 4231](https://www.rfc-editor.org/rfc/rfc4231)), sufficient for our threat model.

### Security: Can the nsec Be Recovered from the Derived Key?

**No.**

- The derived P-256 key is the output of HMAC-SHA512. HMAC is a one-way PRF — you cannot recover the input (nsec + subdomain) from the output.
- Even with the derived private key, recovering the nsec requires inverting HMAC-SHA512, which is computationally infeasible (2^256 operations).
- The nsec and derived key are on different curves with no mathematical relationship.
- If the derived P-256 key is compromised, the attacker cannot recover the nsec or derive keys for other subdomains (each subdomain's derivation includes its name).

### Security: Can Someone Derive My Key from My npub?

**No.**

- npub = nsec × G (secp256k1). This reveals nothing about the nsec itself (discrete logarithm problem, 2^128 security).
- The derivation algorithm requires the nsec as input. The npub doesn't help.
- Even if someone knows your npub AND your derived P-256 public key, they cannot find the nsec without solving secp256k1 ECDLP.

---

## Security Properties

| Property | Description | Security Guarantee |
|---|---|---|
| **Private key never transmitted** | Key is derived locally; only CSR (public key) is sent | ✅ Bot never sees the private key |
| **Private key never stored on server** | No server-side storage of TLS keys | ✅ Bot compromise cannot reveal keys |
| **Deterministic derivation** | Same nsec + subdomain always produces same key | ✅ Key can be re-derived anytime; no need to back up |
| **Per-subdomain isolation** | Different subdomains have different keys | ✅ Compromise of one key doesn't affect others |
| **CSR contains no secrets** | PKCS#10 CSR contains only public key + metadata | ✅ CSR can be safely transmitted and logged |
| **Bot compromise impact** | Bot sees CSR, cert, domain — NOT private key | ✅ Historical TLS keys remain secure |
| **Let's Encrypt compromise impact** | LE sees CSR, cert — NOT private key | ✅ Private key never shared with CA |
| **Forward secrecy** | Keys are derived deterministically from nsec | ⚠️ N/A — not ephemeral by design |

### Threat Model Coverage

| Attacker | What They Get | What They Don't Get |
|---|---|---|
| **Bot compromise** | CSR (public key), certificate, domain, ACME logs | Private key, nsec |
| **Let's Encrypt compromise** | CSR (public key), certificate, ACME account | Private key, nsec |
| **DNS attacker** | Can see DNS-01 challenge token | Cannot influence key derivation |
| **Network eavesdropper** | CSR, certificate | Private key, nsec |
| **Frontend XSS** | If nsec is in memory, could derive key | If nsec not in memory, can't derive |

### Limitations

1. **Not zero-knowledge**: The bot sees the domain and certificate. This is acceptable — we're proving DNS control, not hiding identity.

2. **No end-to-end encryption**: TLS termination happens on the user's server, not through NoDNS. This is standard for certificate issuance.

3. **nsec compromise**: If the nsec is leaked, the attacker can derive the TLS key. This is a shared root of trust — acceptable given the nsec is already the master identity key.

---

## What the Bot DOES See

The bot receives and processes:

| Data | Source | Contains |
|---|---|---|
| **CSR (base64)** | Client POST | Public key, domain name, signature over CSR, X.509 attributes |
| **Domain name** | CSR CN/SAN field | `npub1xxx.nodns.shop` |
| **ACME order metadata** | Let's Encrypt | Order ID, status, challenge token, expires timestamp |
| **Issued certificate** | Let's Encrypt | X.509 certificate chain (public key + CA signature) |
| **ACME progress logs** | Bot logs | Timestamps, HTTP responses, challenge status |

The bot explicitly **does not** see:

- Private key (derived in browser, never transmitted)
- nsec (never transmitted)
- Any other sensitive material

---

## Backward Compatibility

The bot supports two modes:

### Mode 1: Client-Derived Key (New, Recommended)

```
Client POST /api/acme/order
  { domain: "npub1xxx.nodns.shop"
    csr_der: "base64-encoded-csr" }  // ← Client provides CSR
```

Behavior:
- Bot extracts public key from CSR
- Bot submits CSR to Let's Encrypt
- Bot returns certificate only (no private key)
- Client uses derived private key with returned certificate

### Mode 2: Server-Generated Key (Legacy, Fallback)

```
Client POST /api/acme/order
  { domain: "npub1xxx.nodns.shop" }  // ← No CSR
```

Behavior:
- Bot generates random P-256 key pair
- Bot creates CSR from generated key
- Bot submits CSR to Let's Encrypt
- Bot returns certificate + private key
- Bot wipes private key from memory after first fetch

### When to Use Each Mode

| Use Case | Recommended Mode |
|---|---|
| User has nsec and wants deterministic keys | Mode 1 (client-derived) |
| User wants ephemeral keys | Mode 2 (server-generated) |
| User doesn't have nsec | Mode 2 (server-generated) |
| Legacy client not updated | Mode 2 (server-generated) |

Both modes produce valid, browser-trusted certificates from Let's Encrypt. Mode 1 provides stronger security guarantees for users who care about key control.

---

## Implementation Details

### Rust (Bot)

**Module**: `src/tls_derivation.rs`

**Dependencies** (from `Cargo.toml`):
```toml
hmac = "0.12"
sha2 = "0.10"
p256 = "0.13"
pkcs8 = "0.10"
```

**Key functions**:
- `derive_p256_key(nsec: &[u8; 32], subdomain: &str) -> SecretKey` — Deterministic derivation
- `validate_csr(csr_der: &str, expected_domain: &str) -> Result<PublicKey, Error>` — CSR validation
- `extract_public_key_from_csr(csr_der: &str) -> Result<PublicKey, Error>` — Public key extraction

**ACME integration**: Uses `instant-acme` crate's `finalize_csr()` method to submit CSR and receive certificate.

### JavaScript (Frontend)

**Module**: `src/lib/tls-derivation.ts`

**Dependencies** (from `package.json`):
```json
{
  "@noble/hashes": "^1.3.3",
  "@noble/curves": "^1.2.0",
  "@peculiar/x509": "^1.10.1"
}
```

**Key functions**:
- `deriveP256Key(nsecHex: string, subdomain: string): Promise<Uint8Array>` — Derivation using Web Crypto or noble libraries
- `generateCSR(privateKey: Uint8Array, domain: string): Promise<string>` — PKCS#10 CSR generation
- `importJwk(privateKeyBytes: Uint8Array): Promise<CryptoKey>` — Import P-256 key for Web Crypto

**CSR generation**: Uses `@peculiar/x509` to create PKCS#10 certificate signing request in the browser.

### Web Crypto API Compatibility

P-256 (also called `P-256` or `prime256v1`) is supported across all modern browsers:

| Browser | P-256 Support |
|---|---|
| Chrome 90+ | ✅ |
| Firefox 88+ | ✅ |
| Safari 14.1+ | ✅ |
| Edge 90+ | ✅ |

Key import via JWK format works with `window.crypto.subtle.importKey()`.

---

## Complete Request Flow

```
┌─────────────────┐                ┌─────────────────┐                ┌─────────────────┐
│   Browser       │                │   NoDNS Bot     │                │  Let's Encrypt  │
│  (with nsec)    │                │   (Rust)        │                │   (ACME)         │
└────────┬────────┘                └────────┬────────┘                └────────┬────────┘
         │                                  │                                  │
         │ 1. Derive TLS key                │                                  │
         │    nsec + subdomain              │                                  │
         │    → P-256 private key           │                                  │
         │                                  │                                  │
         │ 2. Generate CSR                  │                                  │
         │    from derived key              │                                  │
         │    → base64(csr_der)             │                                  │
         │                                  │                                  │
         │ 3. POST /api/acme/order          │                                  │
         │    { domain, csr_der }           │                                  │
         ├─────────────────────────────────►│                                  │
         │                                  │                                  │
         │                                  │ 4. Create ACME order             │
         │                                  ├─────────────────────────────────►│
         │                                  │                                  │
         │                                  │ 5. Receive DNS-01 challenge      │
         │                                  │ ◄─────────────────────────────────│
         │                                  │                                  │
         │                                  │ 6. Publish _acme-challenge TXT   │
         │                                  │    via DDNS UPDATE (TSIG-signed)  │
         │                                  │    to Knot DNS                    │
         │                                  │                                  │
         │                                  │ 7. Signal verification           │
         │                                  ├─────────────────────────────────►│
         │                                  │                                  │
         │                                  │    LE queries DNS                │
         │                                  │    validates DNSSEC              │
         │                                  │    challenge satisfied            │
         │                                  │                                  │
         │                                  │ 8. Submit CSR (finalize order)   │
         │                                  ├─────────────────────────────────►│
         │                                  │    (contains public key only)     │
         │                                  │                                  │
         │                                  │ 9. Receive signed certificate    │
         │                                  │ ◄─────────────────────────────────│
         │                                  │                                  │
         │ 10. Poll /api/acme/order/{id}    │                                  │
         ├─────────────────────────────────►│                                  │
         │ ◄─────────────────────────────────│                                  │
         │                                  │                                  │
         │ 11. Certificate chain (PEM)      │                                  │
         │     Private key (derived) ✅     │                                  │
         │                                  │                                  │
         │ 12. Install cert + key           │                                  │
         │    on user's server              │                                  │
         │                                  │                                  │
         │ 13. Browser shows green lock 🔒  │                                  │
         │    (standard TLS validation)     │                                  │
         └──────────────────────────────────┴──────────────────────────────────┴───────────────────┘
```

**Timeline**: 10-30 seconds total (Let's Encrypt verification + DNS propagation).

---

## References

### Cryptographic Standards

- **[RFC 4231](https://www.rfc-editor.org/rfc/rfc4231)** — HMAC: Keyed-Hashing for Message Authentication (HMAC-SHA512 test vectors and security properties)
- **[RFC 2986](https://www.rfc-editor.org/rfc/rfc2986)** — PKCS #10: Certification Request Syntax (CSR format)
- **[RFC 8555](https://www.rfc-editor.org/rfc/rfc8555)** — ACME: Automatic Certificate Management Environment
- **[RFC 5480](https://www.rfc-editor.org/rfc/rfc5480)** — Elliptic Curve Cryptography Subject Public Key Information (P-256 encoding in X.509)
- **[RFC 5912](https://www.rfc-editor.org/rfc/rfc5912)** — Algorithm Identifiers for ECDSA (P-256 algorithm IDs)

### Derivation Inspiration

- **[SLIP-0010](https://github.com/satoshilabs/slips/blob/master/slip-0010.md)** — SatoshiLabs, 2016. Multi-algorithm key derivation from single seed (used in Trezor/Ledger). Our derivation is simplified but follows the same domain separation pattern.
- **[BIP-0032](https://github.com/bitcoin/bips/blob/master/bip-0032.mediawiki)** — Hierarchical Deterministic Wallets. HMAC-based derivation with validation.

### Libraries and Audits

- **[instant-acme](https://github.com/djc/instant-acme)** — Rust ACME client (DNS-01 support, ARI)
- **[@noble/hashes](https://github.com/paulmillr/noble-hashes)** — JavaScript SHA-2/HMAC implementation (audit: 2024, no issues found)
- **[@noble/curves](https://github.com/paulmillr/noble-curves)** — JavaScript P-256 implementation (audit: 2024, no issues found)
- **[@peculiar/x509](https://github.com/PeculiarVentures/x509)** — JavaScript PKCS#10 CSR generation
- **[RustCrypto p256](https://crates.io/crates/p256)** — Rust P-256 implementation (audited by zkSecurity, April 2025)
- **[RustCrypto hmac](https://crates.io/crates/hmac)** — Rust HMAC implementation (well-established, part of RustCrypto ecosystem)

### DNSSEC and ACME Context

- **[Let's Encrypt DNS-01 Challenge](https://letsencrypt.org/docs/challenge-types/)** — Official documentation
- **[Let's Encrypt Integration Guide](https://letsencrypt.org/docs/integration-guide/)** — ACME best practices
- **[NoDNS ACME Trust Analysis](17-acme-dns01-trust-analysis.md)** — Full trust chain breakdown
- **[NoDNS nsec→DNSSEC Analysis](15-nsec-to-dnssec-analysis.md)** — SLIP-10 derivation for DNSSEC (related but separate)

### Related NoDNS Documentation

- **[Protocol experimental draft](11-protocol-experimental-draft.md)** — Nostr event format
- **[DNSSEC Setup Reference](12-dnssec-setup.md)** — Knot DNS configuration
- **[Demo Recipes](14-demo-recipes.md)** — End-to-end testing commands
- **[Project README](../README.md)** — Architecture overview

---

## Appendix: Test Vectors (For Verification Only)

These test vectors are provided for implementers to verify their derivation implementation. Do NOT use these keys in production.

### Test Vector 1

```
nsec_hex:        "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
subdomain:       "npub1abcdef.nodns.shop"
expected_der:    "612091aaa12e22dd2abef664f8a01a82cae99ad7441b7ef8110424915c268bc2"
  (NOT real — placeholder for actual derivation verification)
```

### Test Vector 2

```
nsec_hex:        "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
subdomain:       "npub1deadbeef.nodns.shop"
expected_der:    "<32-byte P-256 scalar, different from Test Vector 1>"
```

Implementers should derive their own test vectors using a reference implementation (e.g., `tls_derivation.rs` or `tls-derivation.ts`) and verify cross-language compatibility.

---

**End of Document**