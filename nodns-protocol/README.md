# nodns-protocol

Parse and validate NoDNS `kind 11111` record tags. Standalone, dependency-light, usable by bots, sync daemons, CLIs, and third-party clients.

## Usage

```rust
use nodns_protocol::{parse_records, Record, ValidationPolicy};

let tags = vec![
    vec!["record".into(), "A".into(), "@".into(), "3600".into(), "1.2.3.4".into()],
    vec!["record".into(), "TXT".into(), "@".into(), "3600".into(), "hello".into()],
];

let policy = ValidationPolicy::default();
let records = parse_records(&tags, &policy)?;

assert_eq!(records.len(), 2);
assert_eq!(records[0].rtype, "A");
assert_eq!(records[0].rdata, "1.2.3.4");
```

## Public API

| Item | Purpose |
|---|---|
| `Record { rtype, name, ttl, rdata }` | One DNS record, 5-element zone-file form |
| `ValidationPolicy { allowed_types, block_private_ip, max_txt_length }` | Caller-injected policy with sensible defaults |
| `parse_records(tags, policy)` | Parse all `["record",...]` tags from an event |
| `parse_record(tag, policy)` | Parse a single 5-element tag |
| `validate_record(rec, policy)` | Validate a constructed record |
| `validate_record_set(records)` | Cross-record check (CNAME coexistence) |
| `is_private_ip(ip)` | Check 10 private/reserved ranges |
| `validate_dns_label(name)` | DNS label rules (length, charset, hyphens) |

## Dependencies

`thiserror` + `ipnet` only. No `nostr-sdk`, no `hickory-proto`.

## Validation

- Type whitelist: A, AAAA, CNAME, TXT, MX (configurable)
- Private IP blocking: RFC 1918, loopback, link-local, CGN (`100.64/10`), unspecified (`0/8`)
- TXT length cap (default 512)
- Reserved TXT names: `_dmarc`, `_domainkey`, SPF at apex
- CNAME coexistence (RFC 1912)
- DNS label validation (lowercase alnum + hyphens + underscores, ≤63 chars)

## Tests

65 tests covering all record types, validation rules, edge cases, and security protections.
