# Nostr → DNSSEC Key Derivation (Research)

> **Status**: IMPLEMENTED and LIVE in production since 2026-06-10. SLIP-10-derived P-256 KSK (tag 15318) actively signing the nodns.shop zone alongside original KSK 12717.

## The Problem

Can we derive the DNSSEC signing key from a Nostr nsec (private key) so that controlling the Nostr identity mathematically proves control over the DNS zone?

## Why nsec Cannot Be Used Directly

Nostr uses **secp256k1** (same curve as Bitcoin). DNSSEC does not support secp256k1 as a signing algorithm. The IANA DNSSEC algorithm registry has no entry for secp256k1 and never will — it would require a new IETF RFC Standards Action.

### Supported DNSSEC Elliptic Curve Algorithms

| Algorithm # | Name | Curve | Status |
|---|---|---|---|
| 13 | ECDSA P-256 + SHA-256 | NIST P-256 | RECOMMENDED |
| 14 | ECDSA P-384 + SHA-384 | NIST P-384 | MAY |
| 15 | Ed25519 | Ed25519 | RECOMMENDED |
| 16 | Ed448 | Ed448 | MAY |

The curves are fundamentally different mathematics:
- **secp256k1**: y² = x³ + 7 (Koblitz curve, Bitcoin/Nostr)
- **P-256**: y² = x³ - 3x + b (NIST curve, our current DNSSEC algorithm)
- **Ed25519**: -x² + y² = 1 + dx²y² (Twisted Edwards curve)

You cannot convert a key from one curve to another. A secp256k1 private key has no mathematical relationship to any P-256 or Ed25519 key.

## Solution: SLIP-10 Derivation

**SLIP-10** (SatoshiLabs, the Trezor team) is a standard for deriving keys for **different curves** from a single seed.

### How It Works

```
Your nsec (32 bytes)
  │
  ├──→ SLIP-10 "Bitcoin seed"    → secp256k1 key  (your Nostr identity)
  │
  ├──→ SLIP-10 "Nist256p1 seed"  → P-256 key      (DNSSEC alg 13)
  │
  └──→ SLIP-10 "ed25519 seed"    → Ed25519 key    (DNSSEC alg 15)
```

The mechanism: HMAC-SHA512 with different master-key labels produces completely independent master keys for each curve, but all deterministically derived from the same 32-byte seed (the nsec).

### Derivation for P-256 (matches current algorithm 13)

1. Take nsec as 32-byte seed
2. Compute: `I = HMAC-SHA512(key="Nist256p1 seed", data=nsec_bytes)`
3. Split: `I_L` (first 32 bytes) = candidate private key, `I_R` (last 32 bytes) = chain code
4. Validate: `parse256(I_L) < n` (P-256 group order `n`) and `!= 0`
5. If valid: `I_L` is the P-256 private key
6. If invalid (extremely rare, ~2⁻³² chance): re-hash with incremented data and retry

### Derivation for Ed25519 (algorithm 15)

1. Take nsec as 32-byte seed
2. Compute: `I = HMAC-SHA512(key="ed25519 seed", data=nsec_bytes)`
3. Split: `I_L` (first 32 bytes) = Ed25519 private key
4. Split: `I_R` (last 32 bytes) = chain code
5. `I_L` is directly usable as an Ed25519 private key (no range check needed)

### Verifiable Link

Anyone who knows the npub (public key) can:
1. Take the 32-byte npub-derived seed (same as nsec, public derivation)
2. Derive the P-256 or Ed25519 public key using SLIP-10
3. Compare with the DNSKEY record in the zone
4. Confirm: "this DNSSEC key was derived from that Nostr identity"

**Note**: The derivation is one-way — knowing the derived P-256 key does NOT reveal the nsec.

## Implementation Path

### Step 1: Rust Derivation Module

Add a module to `nodns-bot-rs` that takes an nsec and derives a DNSSEC key.

```rust
// Pseudocode
fn derive_dnssec_key(nsec: &[u8; 32], curve: Curve) -> PrivateKey {
    let label = match curve {
        Curve::P256 => "Nist256p1 seed",
        Curve::Ed25519 => "ed25519 seed",
    };
    let (il, _ir) = hmac_sha512(label.as_bytes(), nsec);
    // Validate range for P-256 (not needed for Ed25519)
    PrivateKey::from_bytes(&il, curve)
}
```

**Rust crates**:
- `hmac` + `sha2` — HMAC-SHA512 computation
- `p256` — P-256 key operations and PKCS#8 encoding
- `pkcs8` — PEM/PKCS#8 output format for Knot import
- `ed25519-dalek` — Ed25519 key handling (if using algorithm 15)

### Step 2: PEM Export

Convert the derived private key to PKCS#8 PEM format:

```rust
fn export_pem(key: &PrivateKey) -> String {
    // P-256: PKCS#8 DER → base64 PEM
    // Ed25519: PKCS#8 DER → base64 PEM
}
```

### Step 3: Knot Import Script

On the VPS, import the derived key:

```bash
# 1. Bot derives key and writes PEM to temp file
# 2. Import into Knot
keymgr nodns.shop import-pem /tmp/derived-ksk.pem algorithm=ECDSAP256SHA256 ksk=yes

# 3. Retire old auto-generated KSK
keymgr nodns.shop retire 12717

# 4. Reload
knotc reload

# 5. Extract new DS for registrar
keymgr nodns.shop ds
```

### Step 4: Nostr Attestation Event

The bot publishes a kind:11111 Nostr event linking the Nostr identity to the DNSKEY:

```json
{
  "kind": 11111,
  "tags": [
    ["dnskey", "nodns.shop", "12717", "13", "<base64 DNSKEY>"]
  ],
  "content": "",
  "pubkey": "<registrar hex pubkey>"
}
```

This creates an on-chain, verifiable link: "the registrar's Nostr identity authorized this DNSKEY for nodns.shop."

## Three-Layer Trust Architecture

```
Layer 1: Key Derivation (mathematical link)
  nsec → SLIP-10 → P-256 DNSSEC KSK → signs the zone

Layer 2: Nostr Attestation (protocol link)
  Registrar publishes kind:11111 with dnskey tag → verifiable on Nostr

Layer 3: Standard DNSSEC Chain (internet trust)
  Root → .shop → nodns.shop (DS → KSK → ZSK → records)
```

### Combined Trust Model

```
Nostr identity (npub)
  ↓ (Nostr event attests DNSKEY)
DNSKEY for nodns.shop
  ↓ (DS record at parent)
.shop TLD
  ↓ (DS record at root)
DNS Root (ICANN trust anchor)
```

## What This Gives Us

- Your Nostr nsec becomes the "master key" for nodns.shop DNSSEC
- Anyone can verify the Nostr → DNSKEY link by deriving the public key from npub
- Losing the nsec = losing DNSSEC key management ability
- Standard DNSSEC chain remains valid — resolvers don't need to know about Nostr

## What This Does NOT Give Us

- **Resolvers don't see the Nostr link** — they validate standard DNSSEC only
- **DS-at-parent is still manual** — Namecheap must receive the DS record
- **nsec compromise = DNSSEC compromise** — same key material derives both

## Open Questions (Decisions Needed)

### P-256 vs Ed25519?

| Factor | P-256 (alg 13) | Ed25519 (alg 15) |
|---|---|---|
| Matches current setup | Yes — no rollover needed | No — requires algorithm rollover |
| Signature size | Same | Same (both ~88 bytes RRSIG) |
| Performance | Slightly slower verification | Faster signing and verification |
| Industry trend | Widely deployed | Growing adoption |
| Derivation simplicity | Requires range check | No range check needed |

**Recommendation**: Stay with P-256 (alg 13) to avoid a DNSSEC algorithm rollover. The performance difference is negligible for a small zone.

### Derivation Path Depth?

- **Master key only** (no child derivation): simplest, one zone
- **With path** (e.g., `m/1237'/11111'/0'/0`): supports multiple zones, hierarchical key management
  - 1237 = Nostr coin type per SLIP-44
  - 11111 = our DNS record kind

**Recommendation**: Master key only for now. Add path support when multi-zone becomes relevant.

### Who Holds the nsec?

- **Bot config file**: simplest, but nsec on disk
- **NIP-46 remote signing**: nsec stays on user's device, bot requests signatures
- **Hardware wallet**: nsec never leaves device (future)

**Recommendation**: Config file for PoC/demo. NIP-46 for production.

### Key Rotation Policy?

If the nsec is compromised, the derived DNSSEC key is also compromised.

- **No rotation**: derive once, use forever (simplest)
- **Indexed derivation**: increment child index for new keys
- **Time-based rotation**: derive new key on schedule

**Recommendation**: No rotation for PoC. Plan for indexed derivation in production.

## Fundamental Limitations (Cannot Be Solved with Code)

### The DS Record Must Exist at the Parent Zone

No matter how clever the key derivation, the `.shop` TLD needs a DS record pointing to our KSK. This requires Namecheap's cooperation. RFC 8078 (CDS/CDNSKEY) could automate this, but Namecheap likely doesn't support it.

### Resolvers Don't Understand Nostr

Public DNS resolvers validate standard DNSSEC. They don't know about Nostr, SLIP-10, or key derivation. The Nostr link is verifiable by custom software but NOT by the standard DNSSEC validation path. This is by design — DNSSEC has its own trust hierarchy.

These aren't bugs — they're architectural boundaries. The three-layer model respects each system's boundaries while creating links between them.

## Production Deployment (2026-06-10)

### Decisions Made

- **Curve**: P-256 (algorithm 13) — matches existing zone, no rollover needed
- **Derivation depth**: Master key only (no child path) — single zone
- **Key storage**: nsec in bot config file (`/opt/nodns-bot/config.toml` under `[registrar]`)
- **Rotation**: None for PoC

### Implementation

- **Code**: `nodns-bot-rs/src/dnssec_derivation.rs` — ~80 lines of Rust
- **Crates**: `hmac`, `sha2`, `p256`, `pkcs8`
- **At startup**: Bot derives P-256 key from registrar nsec, writes PKCS#8 PEM to `/tmp/nodns-slip10.pem`, imports into Knot via `keymgr import-pem`

### Knot DNS Integration

```bash
# Import derived key (must be root)
keymgr nodns.shop import-pem /tmp/nodns-slip10.pem algorithm=ECDSAP256SHA256 ksk=yes

# CRITICAL: fix ownership or key silently fails to sign
chown knot:knot /var/lib/knot/keys/keys/<keyhash>.pem

knotc reload
```

The `chown` step is essential — `keymgr import-pem` creates files as `root:root`, but the Knot DNS daemon runs as `knot:knot` and cannot read the key. It fails silently (no error logged, key just doesn't sign).

### Dual-KSK State

| Key | Tag | Origin | DS at Registrar |
|---|---|---|---|
| Original KSK | 12717 | Knot auto-generated | ✅ DS submitted at Namecheap |
| SLIP-10 KSK | 15318 | Derived from registrar nsec | ⏳ Pending — user needs to add at Namecheap |

Both KSKs are actively signing the zone. Dual-DS (both at registrar) enables a clean transition path.

### Attestation Event

The bot publishes a kind:11111 Nostr event at startup linking the registrar identity to the DNSKEY:

- **Event ID**: `fd0d8d4399dee87c472c8a5883315cac554bec4c8c5ea77db23f83b2b08ef8cf`
- **Tags**: `["dnskey", "nodns.shop", "15318", "13", "<base64 DNSKEY RDATA>"]`, `["dnskey-derivation", "slip10", "Nist256p1 seed"]`
- **Relays**: relay.damus.io, nos.lol

### Verification

```bash
# Verify both KSKs in DNS
dig @8.8.8.8 nodns.shop DNSKEY +short | wc -l
# Should show 3 lines: KSK 12717, KSK 15318, ZSK 33240

# Verify ad flag (DNSSEC validates end-to-end)
dig @8.8.8.8 nodns.shop SOA | grep ";; flags"
# Should include "ad"

# Verify mathematical link (on development machine)
cd nodns-bot-rs && cargo test test_slip10_matches_dns -- --nocapture
# Should print "MATCH: True"
```

## References

- RFC 4033/4034/4035 — DNSSEC basics
- RFC 6605 — ECDSA (P-256/P-384) for DNSSEC
- RFC 8080 — Ed25519/Ed448 for DNSSEC
- RFC 8078 — CDS/CDNSKEY automated DS bootstrap
- RFC 9276 — NSEC3 best practices (zone walking mitigation)
- SLIP-0010 — Universal key derivation for multiple curves
- NIP-01 — Nostr basic protocol
- NIP-19 — bech32 encoding for nsec/npub
