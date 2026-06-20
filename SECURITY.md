# Security Regression Test Suite

This document lists every security invariant enforced by the automated test suites.
The Rust tests run on every commit via the pre-commit hook. The Playwright tests
run against the live API in CI.

## Rust Security Tests (`nodns-bot-rs/src/security_tests.rs`)

Run: `cargo test security_tests` (73 tests, <10s offline)

### Private IP Blocking (A/AAAA Records)

DNS A and AAAA records must not point to private/reserved IP ranges.
The `block_private_ip` policy flag enables this when configured.

| Invariant | Test |
|---|---|
| RFC 1918 10.0.0.0/8 blocked in A records | `rejects_rfc1918_10_x_in_a_record` |
| RFC 1918 172.16.0.0/12 blocked in A records | `rejects_rfc1918_172_16_x_in_a_record` |
| RFC 1918 192.168.0.0/16 blocked in A records | `rejects_rfc1918_192_168_x_in_a_record` |
| Loopback 127.0.0.0/8 blocked in A records | `rejects_loopback_127_x_in_a_record` |
| Link-local 169.254.0.0/16 blocked in A records | `rejects_link_local_169_254_x_in_a_record` |
| CGNAT 100.64.0.0/10 blocked in A records | `rejects_cgnat_100_64_x_in_a_record` |
| 0.0.0.0/8 blocked in A records | `rejects_0_0_0_0_in_a_record` |
| Private IPs blocked in legacy 11-element format | `rejects_private_ip_in_legacy_11_element_format` |
| IPv6 ULA fc00::/7 blocked in AAAA records | `rejects_fc00_ula_in_aaaa_record` |
| IPv6 link-local fe80::/10 blocked in AAAA records | `rejects_fe80_link_local_in_aaaa_record` |
| IPv6 loopback ::1 blocked in AAAA records | `rejects_ipv6_loopback_in_aaaa_record` |

### DNS Label Validation

Record name labels must be valid DNS labels (RFC 1035).

| Invariant | Test |
|---|---|
| Cannot start with hyphen | `rejects_label_starting_with_hyphen` |
| Cannot end with hyphen | `rejects_label_ending_with_hyphen` |
| Max 63 characters | `rejects_label_over_63_chars` |
| Must be lowercase | `rejects_label_with_uppercase` |
| Cannot contain dots | `rejects_label_with_dot_separator` |
| Cannot contain special characters | `rejects_label_with_special_chars` |
| Cannot contain spaces | `rejects_label_with_space` |

### Reserved TXT Record Protection

Prevents users from creating TXT records that could spoof email security policies.

| Invariant | Test |
|---|---|
| `_dmarc` TXT records blocked | `rejects_dmarc_txt_record` |
| `_domainkey` TXT records blocked | `rejects_domainkey_txt_record` |
| SPF (`v=spf1`) TXT at apex blocked | `rejects_spf_txt_at_apex` |
| SPF with leading whitespace blocked | `rejects_spf_txt_at_apex_with_leading_whitespace` |

### CNAME Coexistence (RFC 1912)

CNAME records cannot coexist with other record types at the same name.

| Invariant | Test |
|---|---|
| CNAME + A at same name rejected | `rejects_cname_with_a_at_same_name` |
| CNAME + TXT at same name rejected | `rejects_cname_with_txt_at_same_name` |

### TXT Length Limit

TXT records exceeding the configured max length are rejected.

| Invariant | Test |
|---|---|
| TXT over 512 chars rejected via tag parser | `rejects_oversized_txt_record` |
| TXT over limit rejected in full event classification | `rejects_oversized_txt_in_classify_event` |

### Record Type Whitelist

Only whitelisted record types are accepted. Malformed records are rejected.

| Invariant | Test |
|---|---|
| Unsupported types (SOA) rejected | `rejects_unsupported_record_type` |
| Types not in allowed list rejected | `rejects_record_type_not_in_allowed_list` |
| Empty record type rejected | `rejects_empty_record_type` |
| Malformed A record IP rejected | `rejects_malformed_a_record_ip` |
| Empty rdata for A record rejected | `rejects_empty_rdata_for_a_record` |
| Malformed MX (missing fields) rejected | `rejects_malformed_mx_missing_fields` |
| Malformed SRV (missing fields) rejected | `rejects_malformed_srv_missing_fields` |
| Invalid CNAME domain rejected | `rejects_invalid_cname_domain` |

### Delegation Validation

Delegation events must have valid temporal ranges, belong to the correct zone,
and be signed by the authorized registrar.

| Invariant | Test |
|---|---|
| valid_until == valid_from rejected | `rejects_delegation_valid_until_equal_valid_from` |
| valid_until < valid_from rejected | `rejects_delegation_valid_until_before_valid_from` |
| Expired delegations rejected | `rejects_expired_delegation` |
| Future-dated delegations rejected | `rejects_future_dated_delegation` |
| Domain not in zone rejected | `rejects_delegation_domain_not_in_zone` |
| Non-registrar signer rejected | `rejects_delegation_signed_by_non_registrar` |

### Registrar Authority

Only the configured registrar pubkey can publish registrar keys or validate delegations.

| Invariant | Test |
|---|---|
| Wrong pubkey rejected as registrar | `rejects_registrar_check_with_wrong_pubkey` |
| Unconfigured zone rejected | `rejects_registrar_for_unconfigured_zone` |

### Authority — Name Ownership

Only the npub owner can manage their names. Custom names require active delegation.

| Invariant | Test |
|---|---|
| npub name mismatch rejected | `rejects_npub_name_mismatch` |
| Subdomain of another npub rejected | `rejects_subdomain_of_other_npub_name` |
| Custom name without delegation rejected | `rejects_custom_name_without_delegation` |
| Custom name assigned to other npub rejected | `rejects_custom_name_assigned_to_other_npub` |
| Custom name in grace period rejected | `rejects_custom_name_in_grace_period` |
| Expired delegation rejected | `rejects_custom_name_with_expired_delegation` |
| Domain not in zone rejected | `rejects_authority_for_domain_not_in_zone` |
| Invalid pubkey hex rejected | `rejects_authority_check_with_invalid_pubkey_hex` |

### Payment Validation

Anti-spam payment requirements are enforced for record creation and claims.

| Invariant | Test |
|---|---|
| New records require payment when price > 0 | `rejects_new_record_payment_when_price_nonzero` |
| Updates free when update_price == 0 | `allows_free_updates_when_update_price_zero` |
| Payment disabled when create_price == 0 | `rejects_payment_when_verifier_disabled` |
| npub_names_free doesn't bypass custom name pricing | `rejects_npub_names_free_bypass_for_custom_names` |
| Paid updates enforced when update_price > 0 | `rejects_free_update_when_update_price_nonzero` |
| Zero-price disabled zones skip payment | `rejects_zero_price_bypass_for_paid_zone` |

### Tag Injection Prevention

Malformed or duplicate tags in Nostr events are rejected.

| Invariant | Test |
|---|---|
| Delegation tag too short | `rejects_delegation_tag_too_short` |
| Empty delegation domain | `rejects_delegation_tag_empty_domain` |
| Empty delegation npub | `rejects_delegation_tag_empty_npub` |
| Non-numeric delegation timestamp | `rejects_delegation_tag_non_numeric_timestamp` |
| Registrar tag too short | `rejects_registrar_tag_too_short` |
| Empty registrar zone | `rejects_registrar_tag_empty_zone` |
| Empty registrar pubkey | `rejects_registrar_tag_empty_pubkey` |
| Claim tag empty name | `rejects_claim_tag_empty_name` |
| Claim tag uppercase name | `rejects_claim_tag_uppercase_name` |
| Claim tag special chars | `rejects_claim_tag_special_chars_in_name` |
| Claim tag non-numeric timestamp | `rejects_claim_tag_non_numeric_valid_until` |
| Duplicate delegation tag | `rejects_duplicate_delegation_tag_in_event` |
| Duplicate registrar tag | `rejects_duplicate_registrar_tag_in_event` |
| Duplicate claim tag | `rejects_duplicate_claim_tag_in_event` |
| Event with no recognized tags | `rejects_event_with_no_recognized_tags` |
| Event with wrong kind | `rejects_event_with_wrong_kind` |

## Playwright Security Tests (`tests/security.spec.ts`)

Run: `npx playwright test --project=security`

### DynDNS Auth Gate
- `/nic/update` without Authorization → 401 badauth
- Malformed base64 credentials → 401
- Bearer token instead of Basic nsec → 401
- Wrong nsec for npub → 401
- Empty password → 401
- Custom name without delegation → 401/403

### Response Security Headers
- `X-Content-Type-Options: nosniff` present
- `X-Frame-Options: DENY` present
- `Referrer-Policy` set
- `Permissions-Policy` restricts camera/mic/geolocation

### Path Traversal
- `../etc/passwd` in name param treated as data, not file access
- No filesystem content (`root:`) reflected in responses

### XSS Prevention
- Script tags in params returned as JSON strings, not HTML
- Content-Type is always `application/json`, never `text/html`

### Error Hygiene
- No stack traces in responses
- No internal paths leaked
- No TSIG keys, secrets, or nsec values exposed
- Pricing endpoint doesn't leak connection details

### HTTP Method Enforcement
- POST to GET-only endpoints rejected (4xx)
- DELETE to GET-only endpoints rejected (4xx)
- PUT to GET-only endpoints rejected (4xx)

### Query Parameter Robustness
- Extremely long params don't crash (no 5xx)
- Empty name handled gracefully
- Null bytes and control chars don't crash

## Pre-commit Hook

The pre-commit hook (`.githooks/pre-commit`) gates commits that touch Rust files:

1. `cargo fmt --check` — formatting
2. `cargo clippy -- -D warnings` — linting
3. `cargo test security_tests` — **security regression gate** (blocks commit on failure)

To install hooks: `git config core.hooksPath .githooks`

To bypass (emergency only): `git commit --no-verify`
