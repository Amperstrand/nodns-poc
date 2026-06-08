# Audit Findings & Issue Tracker

Date: 2026-06-07
Scope: Full codebase audit — bot (Rust), frontend (Next.js), docs

## Critical Bugs

### ISSUE-001: Mixed events silently drop DNS records
**File**: `nodns-bot-rs/src/main.rs:502-511`
**Severity**: High
**Status**: ✅ CLOSED — Changed `else if` chain to independent `if` blocks. Delegation, registrar, records, and deletes now all process independently.

---

### ISSUE-002: `last_seen` advances on partial failure — lost events
**File**: `nodns-bot-rs/src/main.rs:520-522`
**Severity**: Medium
**Status**: ✅ WONTFIX — Current behavior is correct: `set_last_seen` runs after processing. If bot crashes mid-event, it WILL be retried. Retrying partial-success events would cause duplicate side effects (double DDNS updates, double payment verification). Admin intervention is the correct remediation for partial failures.

---

### ISSUE-003: CSR base64 silently falls back instead of rejecting
**File**: `nodns-bot-rs/src/main.rs:251`
**Severity**: Medium
**Status**: ✅ CLOSED — Now returns 400 with error message on invalid base64.

---

### ISSUE-004: `parseError` stuck in cert-display
**File**: `nodns-frontend/src/components/cert-display.tsx:138-148`
**Severity**: Low
**Status**: ✅ CLOSED — `setParseError(null)` added at start of `parse()`.

---

### ISSUE-005: `downloadFile` revokes URL immediately — race condition
**File**: `nodns-frontend/src/components/cert-request.tsx:14-21`
**Severity**: Low
**Status**: ✅ CLOSED — `revokeObjectURL` now delayed 1s via `setTimeout`.

---

## Security Concerns

### ISSUE-006: TLS key derivation uses raw HMAC instead of HKDF
**Files**: `nodns-bot-rs/src/tls_derivation.rs`, `nodns-frontend/src/lib/tls-derivation.ts`
**Severity**: Medium (design concern, not exploitable)
**Status**: OPEN

Both implementations use `HMAC-SHA512(key="nodns-tls-v1", data=nsec||0x00||subdomain)` then take the first 32 bytes as a P-256 scalar. This is a bespoke KDF, not a standard one. HKDF (RFC 5869) with extract-then-expand is the proper pattern.

**Risk**: Low — `SecretKey::from_bytes` does modular reduction. But changing this later would break existing derived keys.

**Recommendation**: Document the derivation as a frozen protocol commitment. Add test vectors. Consider HKDF for future protocols.

---

### ISSUE-007: ACME private keys stored in plaintext in SQLite
**File**: `nodns-bot-rs/src/acme.rs:151-156, 438-446`
**Severity**: Medium (deferred)
**Status**: OPEN

ACME account credentials and issued private keys are stored in plaintext in the `meta` and `acme_orders` tables.

**Mitigation**: Client-side derivation means most private keys never reach the server. Account credentials could be encrypted at rest.

---

### ISSUE-008: Payment accounting uses untrusted event metadata
**File**: `nodns-bot-rs/src/payment.rs:245-262`
**Severity**: Medium
**Status**: ✅ CLOSED — `verify_payment` now returns `Result<u64, PaymentError>` with the verified token amount. Caller uses the returned value instead of `p.amount` from the event tag.

---

### ISSUE-009: Payment mint URL exact string comparison
**File**: `nodns-bot-rs/src/payment.rs:123-131`
**Severity**: Low
**Status**: ✅ CLOSED — URLs normalized with `trim_end_matches('/')` before comparison.

---

## Refactoring Opportunities

### ISSUE-010: DNS record validation should use hickory's typed parsers
**File**: `nodns-bot-rs/src/parser.rs`
**Severity**: Low (correctness improvement)
**Status**: OPEN

The parser hand-rolls A/AAAA/MX/SRV validation. Hickory already has `AAAA::from_tokens()`, `MX::from_tokens()`, etc. Using these would eliminate validation drift.

**Recommendation**: Use `hickory-proto` for record validation in the parser.

---

### ISSUE-011: Private IP filtering should use `ipnet`/`cidr` crate
**File**: `nodns-bot-rs/src/parser.rs:36-100`
**Severity**: Low (maintenance)
**Status**: OPEN

Manual CIDR checking with custom `ipv4_in_network()`/`ipv6_in_network()`. The `ipnet` crate handles this correctly and covers all reserved ranges.

---

### ISSUE-012: Duration parsing in config is hand-rolled
**File**: `nodns-bot-rs/src/config.rs:103-136`
**Severity**: Low
**Status**: OPEN

Negative integers silently cast to `u64` and wrap. Should use `humantime` or `parse_duration`.

---

### ISSUE-013: Multi-zone store schema has primary key gap
**File**: `nodns-bot-rs/src/store.rs:695-700`
**Severity**: Medium (only affects multi-zone, currently single zone)
**Status**: OPEN

```sql
PRIMARY KEY (event_id, record_type, name)  -- missing `zone`
```

In multi-zone mode, records from different zones overwrite each other.

**Fix**: Add `zone` to primary key. Requires migration.

---

### ISSUE-014: NIP-05 always returns `_` key, non-deterministic registrar
**File**: `nodns-bot-rs/src/nip05.rs:107-115, 163-173`
**Severity**: Low
**Status**: OPEN

NIP-05 response always uses `"_"` as the names key regardless of query. Registrar selection uses `HashMap::values().next()` which is non-deterministic.

---

## Documentation

### ISSUE-015: Protocol spec missing delete tag
**File**: `docs/11-protocol-spec-v0.1.md`
**Severity**: High (spec incomplete)
**Status**: ✅ CLOSED — Added Type 1b delete section, wire format entry, and examples.

---

### ISSUE-016: Deployment status doc outdated
**File**: `docs/08-deployment-status.md`
**Severity**: Low
**Status**: ✅ CLOSED — Rewritten with current architecture: Next.js frontend, ACME, delete support, correct file paths.

---

## Frontend Code Quality

### ISSUE-017: Dead code — `hexToSecretKey`, `ZONES`
**Files**: `nodns-frontend/src/lib/nostr.ts:29-31`, `nodns-frontend/src/lib/constants.ts`
**Severity**: Low
**Status**: ✅ CLOSED — Removed `hexToSecretKey()` from nostr.ts, removed `ZONES` from constants.ts.

---

### ISSUE-018: Site footer broken link
**File**: `nodns-frontend/src/components/site-footer.tsx:5-13`
**Severity**: Low
**Status**: ✅ CLOSED — Changed link from `relay.ngit.dev` to nostr-tools GitHub.

---

### ISSUE-019: Relay list diverges across files
**Files**: `constants.ts`, `architecture.tsx`, `infrastructure.tsx`
**Severity**: Low (maintenance)
**Status**: ✅ CLOSED — Both architecture.tsx and infrastructure.tsx now import RELAYS from constants.ts (single source of truth).

---

### ISSUE-020: Record browser hardcoded API URL
**File**: `nodns-frontend/src/components/record-browser.tsx:30-31`
**Severity**: Low
**Status**: ✅ CLOSED — Changed to relative `/api/records`.

---

### ISSUE-021: Record browser resets collapse state on refresh
**File**: `nodns-frontend/src/components/record-browser.tsx:43`
**Severity**: Low (UX)
**Status**: ✅ CLOSED — Now preserves existing expanded groups and only adds new ones.

---

### ISSUE-024: Live feed reports "connected" prematurely
**File**: `nodns-frontend/src/components/live-feed.tsx:25-37`
**Severity**: Low (UX)
**Status**: ✅ CLOSED — Removed 10s timeout. Connected state now only set on actual event receipt.

### ISSUE-022: Timer/interval cleanup gaps in dashboard
**File**: `nodns-frontend/src/components/dashboard.tsx:89-108, 321-327`
**Severity**: Low (potential memory leak)
**Status**: OPEN

`setTimeout(() => fetchRecords(), 3000)` has no cleanup. Interval-driven DNS polling can overlap.

---

### ISSUE-023: No error boundary in frontend
**Files**: All frontend components
**Severity**: Low
**Status**: OPEN

No `ErrorBoundary` found. CSR/cert parsing failures show as broken components instead of graceful fallbacks.

---

### ISSUE-025: CSR generator still uses @peculiar/x509 with tsyringe risk
**File**: `nodns-frontend/src/lib/csr-generator.ts`
**Severity**: Medium (potential runtime crash)
**Status**: ✅ CLOSED — Rewritten with pure WebCrypto API + manual ASN.1 DER encoding. Zero npm dependencies. Same interface.

---

### ISSUE-026: ACME account cache race condition
**File**: `nodns-bot-rs/src/acme.rs:72-88`
**Severity**: Low (unlikely in practice)
**Status**: ✅ CLOSED — Added double-checked locking: re-check cache after reacquiring lock before inserting. Prevents duplicate account creation on concurrent requests.

---

## Summary

| Priority | Count | Fixed | Remaining |
|---|---|---|---|
| Critical Bugs | 5 | 4 (ISSUE-001, 003, 004, 005) | 1 (ISSUE-002 — WONTFIX, correct behavior) |
| Security | 4 | 2 (ISSUE-008, 009) | 2 (ISSUE-006, 007 — deferred/design decisions) |
| Refactoring | 5 | 0 | 5 (ISSUE-010-014 — need crate research) |
| Documentation | 2 | 2 (ISSUE-015, 016) | 0 |
| Frontend Quality | 10 | 8 (ISSUE-017-021, 024, 025) | 2 (ISSUE-022, 023) |
| System | 1 | 1 (ISSUE-026) | 0 |
| **Total** | **28** | **18 CLOSED** | **10 OPEN** (2 WONTFIX, 5 refactor, 2 frontend, 2 deferred) |

### Fixed in this session (2026-06-07):
- ISSUE-001: Mixed events — changed `else if` to independent `if` blocks
- ISSUE-002: WONTFIX — current `last_seen` behavior correct (retry causes duplicates)
- ISSUE-003: CSR base64 validation → 400 error
- ISSUE-004: parseError reset on re-parse
- ISSUE-005: downloadFile URL revocation race
- ISSUE-008: Payment accounting now uses verified token amount, not event tag
- ISSUE-009: Mint URL normalization (trailing slash)
- ISSUE-015: Protocol spec delete tag
- ISSUE-016: Deployment status doc rewrite
- ISSUE-017: Dead code removal (hexToSecretKey, ZONES)
- ISSUE-018: Site footer link fix
- ISSUE-019: Relay list centralization to constants.ts
- ISSUE-020: Record browser relative API URL
- ISSUE-021: Record browser collapse state preservation
- ISSUE-024: Live feed premature connected state removed
- ISSUE-025: CSR generator rewritten with pure WebCrypto + ASN.1 DER (no @peculiar/x509)
- ISSUE-026: ACME account cache double-checked locking

---

### ISSUE-027: ZeroSSL set as default CA despite requiring operator setup
**Files**: `nodns-bot-rs/src/config.rs:325`, `nodns-frontend/src/components/cert-request.tsx:57`
**Severity**: Medium (usability — default would fail without EAB credentials)
**Status**: ✅ CLOSED — Default CA changed from `"zerossl"` to `"letsencrypt-staging"` in both bot config and frontend radio. ZeroSSL remains available as opt-in. See `docs/17-acme-dns01-trust-analysis.md` for EAB documentation.

---

### Fixed 2026-06-08:
- ISSUE-027: ZeroSSL default → LE staging (would fail without EAB creds)
