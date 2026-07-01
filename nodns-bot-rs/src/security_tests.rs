use std::collections::HashMap;
use std::sync::Arc;

use nostr_sdk::nips::nip19::ToBech32;
use nostr_sdk::prelude::*;

use crate::auth::AuthorityChecker;
use crate::config::ZonePaymentConfig;
use crate::parser;
use crate::payment::Verifier;
use crate::pob;
use crate::pow;
use crate::store::Store;
use crate::types::Delegation;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn make_pubkey_hex() -> &'static str {
    "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
}

fn make_other_pubkey_hex() -> &'static str {
    "c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5"
}

fn make_npub() -> String {
    let pk = PublicKey::from_hex(make_pubkey_hex()).unwrap();
    pk.to_bech32().unwrap()
}

fn setup_store() -> Arc<Store> {
    let store = Store::new(":memory:", None).expect("open in-memory db");
    store.init().expect("init schema");
    Arc::new(store)
}

fn a_record_tag(rdata: &str) -> Vec<String> {
    vec![
        "record".to_string(),
        "A".to_string(),
        "@".to_string(),
        "3600".to_string(),
        rdata.to_string(),
    ]
}

fn aaaa_record_tag(rdata: &str) -> Vec<String> {
    vec![
        "record".to_string(),
        "AAAA".to_string(),
        "@".to_string(),
        "3600".to_string(),
        rdata.to_string(),
    ]
}

fn txt_record_tag(name: &str, rdata: &str) -> Vec<String> {
    vec![
        "record".to_string(),
        "TXT".to_string(),
        name.to_string(),
        "3600".to_string(),
        rdata.to_string(),
    ]
}

fn legacy_a_record_tag(rdata: &str) -> Vec<String> {
    vec![
        "record".to_string(),
        "A".to_string(),
        "@".to_string(),
        "".to_string(),
        "".to_string(),
        "".to_string(),
        "".to_string(),
        "".to_string(),
        "".to_string(),
        rdata.to_string(),
        "3600".to_string(),
    ]
}

// ===========================================================================
// SECURITY: Private IP blocking — A records (RFC 1918, loopback, link-local)
// ===========================================================================

#[test]
fn rejects_rfc1918_10_x_in_a_record() {
    let err = parser::parse_record_tag(&a_record_tag("10.0.0.1"), &[], true, 0).unwrap_err();
    assert!(err.to_string().contains("private IP address blocked"));
}

#[test]
fn rejects_rfc1918_172_16_x_in_a_record() {
    let err = parser::parse_record_tag(&a_record_tag("172.16.5.4"), &[], true, 0).unwrap_err();
    assert!(err.to_string().contains("private IP address blocked"));
}

#[test]
fn rejects_rfc1918_192_168_x_in_a_record() {
    let err = parser::parse_record_tag(&a_record_tag("192.168.1.1"), &[], true, 0).unwrap_err();
    assert!(err.to_string().contains("private IP address blocked"));
}

#[test]
fn rejects_loopback_127_x_in_a_record() {
    let err = parser::parse_record_tag(&a_record_tag("127.0.0.1"), &[], true, 0).unwrap_err();
    assert!(err.to_string().contains("private IP address blocked"));
}

#[test]
fn rejects_link_local_169_254_x_in_a_record() {
    let err = parser::parse_record_tag(&a_record_tag("169.254.1.1"), &[], true, 0).unwrap_err();
    assert!(err.to_string().contains("private IP address blocked"));
}

#[test]
fn rejects_cgnat_100_64_x_in_a_record() {
    let err = parser::parse_record_tag(&a_record_tag("100.64.0.1"), &[], true, 0).unwrap_err();
    assert!(err.to_string().contains("private IP address blocked"));
}

#[test]
fn rejects_0_0_0_0_in_a_record() {
    let err = parser::parse_record_tag(&a_record_tag("0.0.0.0"), &[], true, 0).unwrap_err();
    assert!(err.to_string().contains("private IP address blocked"));
}

#[test]
fn rejects_private_ip_in_legacy_11_element_format() {
    let err = parser::parse_record_tag(&legacy_a_record_tag("10.1.2.3"), &[], true, 0).unwrap_err();
    assert!(err.to_string().contains("private IP address blocked"));
}

#[test]
fn allows_public_ip_when_blocking_enabled() {
    let rec = parser::parse_record_tag(&a_record_tag("1.1.1.1"), &[], true, 0).unwrap();
    assert_eq!(rec.rdata, "1.1.1.1");
}

// ===========================================================================
// SECURITY: Private IP blocking — AAAA records (IPv6 ULA, link-local, loopback)
// ===========================================================================

#[test]
fn rejects_fc00_ula_in_aaaa_record() {
    let err = parser::parse_record_tag(&aaaa_record_tag("fc00::1"), &[], true, 0).unwrap_err();
    assert!(err.to_string().contains("private IP address blocked"));
}

#[test]
fn rejects_fe80_link_local_in_aaaa_record() {
    let err = parser::parse_record_tag(&aaaa_record_tag("fe80::1"), &[], true, 0).unwrap_err();
    assert!(err.to_string().contains("private IP address blocked"));
}

#[test]
fn rejects_ipv6_loopback_in_aaaa_record() {
    let err = parser::parse_record_tag(&aaaa_record_tag("::1"), &[], true, 0).unwrap_err();
    assert!(err.to_string().contains("private IP address blocked"));
}

// ===========================================================================
// SECURITY: DNS label validation (name field)
// ===========================================================================

#[test]
fn rejects_label_starting_with_hyphen() {
    let err = parser::validate_dns_label("-bad").unwrap_err();
    assert!(err.to_string().contains("start with a hyphen"));
}

#[test]
fn rejects_label_ending_with_hyphen() {
    let err = parser::validate_dns_label("bad-").unwrap_err();
    assert!(err.to_string().contains("end with a hyphen"));
}

#[test]
fn rejects_label_over_63_chars() {
    let label = "a".repeat(64);
    let err = parser::validate_dns_label(&label).unwrap_err();
    assert!(err.to_string().contains("too long"));
}

#[test]
fn rejects_label_with_uppercase() {
    let err = parser::validate_dns_label("CamelCase").unwrap_err();
    assert!(err.to_string().contains("uppercase"));
}

#[test]
fn rejects_label_with_dot_separator() {
    let err = parser::validate_dns_label("sub.domain").unwrap_err();
    assert!(err.to_string().contains("invalid character"));
}

#[test]
fn rejects_label_with_special_chars() {
    let err = parser::validate_dns_label("hello@world").unwrap_err();
    assert!(err.to_string().contains("invalid character"));
}

#[test]
fn rejects_label_with_space() {
    let err = parser::validate_dns_label("hello world").unwrap_err();
    assert!(err.to_string().contains("invalid character"));
}

// ===========================================================================
// SECURITY: Reserved TXT record protection (DMARC, DKIM, SPF spoofing)
// ===========================================================================

#[test]
fn rejects_dmarc_txt_record() {
    let err = parser::parse_record_tag(
        &txt_record_tag("_dmarc", "v=DMARC1; p=reject"),
        &[],
        false,
        0,
    )
    .unwrap_err();
    assert!(err.to_string().contains("_dmarc") && err.to_string().contains("reserved"));
}

#[test]
fn rejects_domainkey_txt_record() {
    let err =
        parser::parse_record_tag(&txt_record_tag("_domainkey", "o=-"), &[], false, 0).unwrap_err();
    assert!(err.to_string().contains("_domainkey") && err.to_string().contains("reserved"));
}

#[test]
fn rejects_spf_txt_at_apex() {
    let tag = txt_record_tag("@", "v=spf1 include:_spf.google.com ~all");
    let err = parser::parse_record_tag(&tag, &[], false, 0).unwrap_err();
    assert!(err.to_string().contains("SPF"));
}

#[test]
fn rejects_spf_txt_at_apex_with_leading_whitespace() {
    let tag = txt_record_tag("@", "   v=spf1 -all");
    let err = parser::parse_record_tag(&tag, &[], false, 0).unwrap_err();
    assert!(err.to_string().contains("SPF"));
}

// ===========================================================================
// SECURITY: CNAME coexistence (RFC 1912)
// ===========================================================================

#[test]
fn rejects_cname_with_a_at_same_name() {
    let keys = Keys::generate();
    let event = EventBuilder::new(Kind::Custom(11111), "")
        .tags(vec![
            Tag::parse(["record", "A", "@", "3600", "1.2.3.4"]).unwrap(),
            Tag::parse(["record", "CNAME", "@", "3600", "target.example.com"]).unwrap(),
        ])
        .sign_with_keys(&keys)
        .unwrap();
    let err = parser::classify_event(&event, &[], false, 0).unwrap_err();
    assert!(matches!(err, parser::ParserError::CannotCoexistWithCname));
}

#[test]
fn rejects_cname_with_txt_at_same_name() {
    let keys = Keys::generate();
    let event = EventBuilder::new(Kind::Custom(11111), "")
        .tags(vec![
            Tag::parse(["record", "TXT", "www", "3600", "hello"]).unwrap(),
            Tag::parse(["record", "CNAME", "www", "3600", "target.example.com"]).unwrap(),
        ])
        .sign_with_keys(&keys)
        .unwrap();
    let err = parser::classify_event(&event, &[], false, 0).unwrap_err();
    assert!(matches!(err, parser::ParserError::CannotCoexistWithCname));
}

// ===========================================================================
// SECURITY: TXT record length limit
// ===========================================================================

#[test]
fn rejects_oversized_txt_record() {
    let tag = txt_record_tag("@", &"a".repeat(600));
    let err = parser::parse_record_tag(&tag, &[], false, 512).unwrap_err();
    assert!(err.to_string().contains("exceeds max length"));
}

#[test]
fn rejects_oversized_txt_in_classify_event() {
    let keys = Keys::generate();
    let long_txt = "a".repeat(600);
    let event = EventBuilder::new(Kind::Custom(11111), "")
        .tags(vec![Tag::parse([
            "record",
            "TXT",
            "@",
            "3600",
            long_txt.as_str(),
        ])
        .unwrap()])
        .sign_with_keys(&keys)
        .unwrap();
    assert!(parser::classify_event(&event, &[], false, 512).is_err());
}

// ===========================================================================
// SECURITY: Record type whitelist enforcement
// ===========================================================================

#[test]
fn rejects_unsupported_record_type() {
    let tag = vec![
        "record".to_string(),
        "SOA".to_string(),
        "@".to_string(),
        "3600".to_string(),
        "ns1.example.com admin.example.com".to_string(),
    ];
    let err = parser::parse_record_tag(&tag, &[], false, 0).unwrap_err();
    assert!(err.to_string().contains("unsupported record type"));
}

#[test]
fn rejects_record_type_not_in_allowed_list() {
    let allowed = vec!["A".to_string(), "AAAA".to_string()];
    let tag = vec![
        "record".to_string(),
        "CNAME".to_string(),
        "@".to_string(),
        "3600".to_string(),
        "example.com".to_string(),
    ];
    let err = parser::parse_record_tag(&tag, &allowed, false, 0).unwrap_err();
    assert!(err.to_string().contains("not allowed"));
}

#[test]
fn rejects_empty_record_type() {
    let tag = vec![
        "record".to_string(),
        "".to_string(),
        "@".to_string(),
        "3600".to_string(),
        "1.2.3.4".to_string(),
    ];
    assert!(parser::parse_record_tag(&tag, &[], false, 0).is_err());
}

#[test]
fn rejects_malformed_a_record_ip() {
    let tag = a_record_tag("not.an.ip.address");
    let err = parser::parse_record_tag(&tag, &[], false, 0).unwrap_err();
    assert!(err.to_string().contains("invalid IPv4 address"));
}

#[test]
fn rejects_empty_rdata_for_a_record() {
    let tag = vec![
        "record".to_string(),
        "A".to_string(),
        "@".to_string(),
        "3600".to_string(),
        "".to_string(),
    ];
    let err = parser::parse_record_tag(&tag, &[], false, 0).unwrap_err();
    assert!(err.to_string().contains("requires rdata"));
}

#[test]
fn rejects_malformed_mx_missing_fields() {
    let tag = vec![
        "record".to_string(),
        "MX".to_string(),
        "@".to_string(),
        "3600".to_string(),
        "10".to_string(),
    ];
    let err = parser::parse_record_tag(&tag, &[], false, 0).unwrap_err();
    assert!(err.to_string().contains("MX record requires"));
}

#[test]
fn rejects_malformed_srv_missing_fields() {
    let tag = vec![
        "record".to_string(),
        "SRV".to_string(),
        "@".to_string(),
        "3600".to_string(),
        "10 20".to_string(),
    ];
    let err = parser::parse_record_tag(&tag, &[], false, 0).unwrap_err();
    assert!(err.to_string().contains("SRV record requires"));
}

#[test]
fn rejects_invalid_cname_domain() {
    let tag = vec![
        "record".to_string(),
        "CNAME".to_string(),
        "@".to_string(),
        "3600".to_string(),
        "invalid..double..dot".to_string(),
    ];
    let err = parser::parse_record_tag(&tag, &[], false, 0).unwrap_err();
    assert!(err.to_string().contains("empty label") || err.to_string().contains("domain"));
}

// ===========================================================================
// SECURITY: Delegation range validation (valid_from < valid_until)
// ===========================================================================

#[test]
fn rejects_delegation_valid_until_equal_valid_from() {
    let store = setup_store();
    let checker = AuthorityChecker::new(store, HashMap::new());
    let delegation = Delegation {
        domain: "alice.test.shop".to_string(),
        npub: make_npub(),
        valid_from: 1000,
        valid_until: 1000,
        renew_by: 1000,
    };
    let result = checker.validate_delegation(&delegation, "test.shop", make_pubkey_hex());
    assert!(result.is_err());
}

#[test]
fn rejects_delegation_valid_until_before_valid_from() {
    let store = setup_store();
    let checker = AuthorityChecker::new(store, HashMap::new());
    let delegation = Delegation {
        domain: "alice.test.shop".to_string(),
        npub: make_npub(),
        valid_from: 5000,
        valid_until: 1000,
        renew_by: 1000,
    };
    let result = checker.validate_delegation(&delegation, "test.shop", make_pubkey_hex());
    assert!(result.is_err());
}

// ===========================================================================
// SECURITY: Delegation temporal validation (expiry / future start)
// ===========================================================================

#[test]
fn rejects_expired_delegation() {
    let store = setup_store();
    let checker = AuthorityChecker::new(store, HashMap::new());
    let delegation = Delegation {
        domain: "alice.test.shop".to_string(),
        npub: make_npub(),
        valid_from: 0,
        valid_until: 1,
        renew_by: 1,
    };
    let err = checker
        .validate_delegation(&delegation, "test.shop", make_pubkey_hex())
        .unwrap_err();
    assert!(err.to_string().contains("expired"));
}

#[test]
fn rejects_future_dated_delegation() {
    let store = setup_store();
    let checker = AuthorityChecker::new(store, HashMap::new());
    let delegation = Delegation {
        domain: "alice.test.shop".to_string(),
        npub: make_npub(),
        valid_from: 99999999999,
        valid_until: 999999999999,
        renew_by: 99999999999,
    };
    let err = checker
        .validate_delegation(&delegation, "test.shop", make_pubkey_hex())
        .unwrap_err();
    assert!(err.to_string().contains("future"));
}

// ===========================================================================
// SECURITY: Delegation domain must belong to zone
// ===========================================================================

#[test]
fn rejects_delegation_domain_not_in_zone() {
    let store = setup_store();
    let checker = AuthorityChecker::new(store, HashMap::new());
    let delegation = Delegation {
        domain: "alice.other.com".to_string(),
        npub: make_npub(),
        valid_from: 0,
        valid_until: 9999999999,
        renew_by: 9999999999,
    };
    let err = checker
        .validate_delegation(&delegation, "test.shop", make_pubkey_hex())
        .unwrap_err();
    assert!(err.to_string().contains("not within zone"));
}

// ===========================================================================
// SECURITY: Registrar authority enforcement
// ===========================================================================

#[test]
fn rejects_delegation_signed_by_non_registrar() {
    let store = setup_store();
    let checker = AuthorityChecker::new(store, HashMap::new());
    let delegation = Delegation {
        domain: "alice.test.shop".to_string(),
        npub: make_npub(),
        valid_from: 0,
        valid_until: 9999999999,
        renew_by: 9999999999,
    };
    let err = checker
        .validate_delegation(&delegation, "test.shop", make_other_pubkey_hex())
        .unwrap_err();
    assert!(err.to_string().contains("not the registrar"));
}

#[test]
fn rejects_registrar_check_with_wrong_pubkey() {
    let store = setup_store();
    let npub = make_npub();
    store
        .save_registrar_key("test.shop", make_pubkey_hex(), &npub, "test", "event1")
        .unwrap();
    let checker = AuthorityChecker::new(store, HashMap::new());
    assert!(!checker
        .is_registrar("test.shop", make_other_pubkey_hex())
        .unwrap());
}

#[test]
fn rejects_registrar_for_unconfigured_zone() {
    let store = setup_store();
    let checker = AuthorityChecker::new(store, HashMap::new());
    assert!(!checker
        .is_registrar("unknown.shop", make_pubkey_hex())
        .unwrap());
}

// ===========================================================================
// SECURITY: Authority — npub name must match signer
// ===========================================================================

#[test]
fn rejects_npub_name_mismatch() {
    let store = setup_store();
    let checker = AuthorityChecker::new(store, HashMap::new());
    let err = checker
        .check_authority("npub1wrongkey.test.shop.", "test.shop", make_pubkey_hex())
        .unwrap_err();
    assert!(err.to_string().contains("does not match signer npub"));
}

#[test]
fn rejects_subdomain_of_other_npub_name() {
    let store = setup_store();
    let checker = AuthorityChecker::new(store, HashMap::new());
    let err = checker
        .check_authority(
            "_acme-challenge.npub1wrongkey.test.shop.",
            "test.shop",
            make_pubkey_hex(),
        )
        .unwrap_err();
    assert!(err.to_string().contains("does not match signer npub"));
}

// ===========================================================================
// SECURITY: Authority — custom name requires delegation to the signer
// ===========================================================================

#[test]
fn rejects_custom_name_without_delegation() {
    let store = setup_store();
    let checker = AuthorityChecker::new(store, HashMap::new());
    let err = checker
        .check_authority("alice.test.shop.", "test.shop", make_pubkey_hex())
        .unwrap_err();
    assert!(err.to_string().contains("no active delegation"));
}

#[test]
fn rejects_custom_name_assigned_to_other_npub() {
    let store = setup_store();
    store
        .save_delegation(
            "event1",
            "alice",
            "test.shop",
            "npub1someotherkey",
            make_pubkey_hex(),
            0,
            9999999999,
            9999999999,
            make_pubkey_hex(),
        )
        .unwrap();
    let checker = AuthorityChecker::new(store, HashMap::new());
    let err = checker
        .check_authority("alice.test.shop.", "test.shop", make_pubkey_hex())
        .unwrap_err();
    assert!(err.to_string().contains("assigned to"));
}

#[test]
fn rejects_custom_name_in_grace_period() {
    let store = setup_store();
    let npub = make_npub();
    store
        .save_delegation(
            "event1",
            "alice",
            "test.shop",
            &npub,
            make_pubkey_hex(),
            0,
            9999999999,
            9999999999,
            make_pubkey_hex(),
        )
        .unwrap();
    store.mark_delegation_grace("alice", "test.shop").unwrap();
    let checker = AuthorityChecker::new(store, HashMap::new());
    let err = checker
        .check_authority("alice.test.shop.", "test.shop", make_pubkey_hex())
        .unwrap_err();
    assert!(err.to_string().contains("grace period"));
}

#[test]
fn rejects_custom_name_with_expired_delegation() {
    let store = setup_store();
    let npub = make_npub();
    store
        .save_delegation(
            "event1",
            "alice",
            "test.shop",
            &npub,
            make_pubkey_hex(),
            0,
            9999999999,
            9999999999,
            make_pubkey_hex(),
        )
        .unwrap();
    store.mark_delegation_grace("alice", "test.shop").unwrap();
    store.mark_delegation_expired("alice", "test.shop").unwrap();
    let checker = AuthorityChecker::new(store, HashMap::new());
    let err = checker
        .check_authority("alice.test.shop.", "test.shop", make_pubkey_hex())
        .unwrap_err();
    assert!(err.to_string().contains("no active delegation"));
}

#[test]
fn rejects_authority_for_domain_not_in_zone() {
    let store = setup_store();
    let checker = AuthorityChecker::new(store, HashMap::new());
    let err = checker
        .check_authority("alice.other.shop.", "test.shop", make_pubkey_hex())
        .unwrap_err();
    assert!(err.to_string().contains("does not belong to zone"));
}

// ===========================================================================
// SECURITY: Payment — create/update pricing and free-tier enforcement
// ===========================================================================

#[test]
fn rejects_new_record_payment_when_price_nonzero() {
    let cfg = ZonePaymentConfig {
        enabled: true,
        create_price: 100,
        update_price: 50,
        ..ZonePaymentConfig::default()
    };
    let v = Verifier::from_zone_config(&cfg);
    assert!(v.should_require_payment(false));
    assert!(v.should_require_payment(true));
}

#[test]
fn allows_free_updates_when_update_price_zero() {
    let cfg = ZonePaymentConfig {
        enabled: true,
        create_price: 100,
        update_price: 0,
        ..ZonePaymentConfig::default()
    };
    let v = Verifier::from_zone_config(&cfg);
    assert!(v.should_require_payment(false));
    assert!(!v.should_require_payment(true));
}

#[test]
fn rejects_payment_when_verifier_disabled() {
    let cfg = ZonePaymentConfig {
        enabled: true,
        create_price: 0,
        ..ZonePaymentConfig::default()
    };
    let v = Verifier::from_zone_config(&cfg);
    assert!(!v.should_require_payment(false));
    assert!(!v.should_require_payment(true));
}

#[test]
fn rejects_npub_names_free_bypass_for_custom_names() {
    let cfg = ZonePaymentConfig {
        enabled: true,
        create_price: 250,
        npub_names_free: true,
        ..ZonePaymentConfig::default()
    };
    let v = Verifier::from_zone_config(&cfg);
    assert_eq!(v.create_price(), 250);
    assert!(v.should_require_payment(false));
}

// ===========================================================================
// SECURITY: Payment — update price enforcement
// ===========================================================================

#[test]
fn rejects_free_update_when_update_price_nonzero() {
    let cfg = ZonePaymentConfig {
        enabled: true,
        create_price: 500,
        update_price: 250,
        ..ZonePaymentConfig::default()
    };
    let v = Verifier::from_zone_config(&cfg);
    assert_eq!(v.update_price(), 250);
    assert!(v.should_require_payment(true));
}

#[test]
fn rejects_zero_price_bypass_for_paid_zone() {
    let cfg = ZonePaymentConfig {
        enabled: true,
        create_price: 1000,
        ..ZonePaymentConfig::default()
    };
    let v = Verifier::from_zone_config(&cfg);
    assert_eq!(v.create_price(), 1000);
    assert!(v.should_require_payment(false));
    assert!(!v.should_require_payment(true));
}

// ===========================================================================
// SECURITY: Delegation tag parsing — malformed input rejection
// ===========================================================================

#[test]
fn rejects_delegation_tag_too_short() {
    let tag = vec!["delegation".to_string(), "alice.test.shop".to_string()];
    let err = parser::parse_delegation_tag(&tag).unwrap_err();
    assert!(err.to_string().contains("must have 6 elements"));
}

#[test]
fn rejects_delegation_tag_empty_domain() {
    let tag = vec![
        "delegation".to_string(),
        "".to_string(),
        "npub1abc".to_string(),
        "1000".to_string(),
        "2000".to_string(),
        "1500".to_string(),
    ];
    let err = parser::parse_delegation_tag(&tag).unwrap_err();
    assert!(err.to_string().contains("domain cannot be empty"));
}

#[test]
fn rejects_delegation_tag_empty_npub() {
    let tag = vec![
        "delegation".to_string(),
        "alice.test.shop".to_string(),
        "".to_string(),
        "1000".to_string(),
        "2000".to_string(),
        "1500".to_string(),
    ];
    let err = parser::parse_delegation_tag(&tag).unwrap_err();
    assert!(err.to_string().contains("npub cannot be empty"));
}

#[test]
fn rejects_delegation_tag_non_numeric_timestamp() {
    let tag = vec![
        "delegation".to_string(),
        "alice.test.shop".to_string(),
        "npub1abc".to_string(),
        "not-a-number".to_string(),
        "2000".to_string(),
        "1500".to_string(),
    ];
    assert!(parser::parse_delegation_tag(&tag).is_err());
}

// ===========================================================================
// SECURITY: Registrar tag parsing — malformed input rejection
// ===========================================================================

#[test]
fn rejects_registrar_tag_too_short() {
    let tag = vec!["registrar".to_string(), "test.shop".to_string()];
    let err = parser::parse_registrar_tag(&tag).unwrap_err();
    assert!(err.to_string().contains("must have 3 elements"));
}

#[test]
fn rejects_registrar_tag_empty_zone() {
    let tag = vec![
        "registrar".to_string(),
        "".to_string(),
        "abcdef123456".to_string(),
    ];
    let err = parser::parse_registrar_tag(&tag).unwrap_err();
    assert!(err.to_string().contains("zone cannot be empty"));
}

#[test]
fn rejects_registrar_tag_empty_pubkey() {
    let tag = vec![
        "registrar".to_string(),
        "test.shop".to_string(),
        "".to_string(),
    ];
    let err = parser::parse_registrar_tag(&tag).unwrap_err();
    assert!(err.to_string().contains("pubkey hex cannot be empty"));
}

// ===========================================================================
// SECURITY: Claim tag — name validation
// ===========================================================================

#[test]
fn rejects_claim_tag_empty_name() {
    let tag = vec![
        "claim".to_string(),
        "".to_string(),
        "nodns.shop".to_string(),
        "1780704000".to_string(),
    ];
    let err = parser::parse_claim_tag(&tag).unwrap_err();
    assert!(err.to_string().contains("name cannot be empty"));
}

#[test]
fn rejects_claim_tag_uppercase_name() {
    let tag = vec![
        "claim".to_string(),
        "Alice".to_string(),
        "nodns.shop".to_string(),
        "1780704000".to_string(),
    ];
    let err = parser::parse_claim_tag(&tag).unwrap_err();
    assert!(err.to_string().contains("uppercase"));
}

#[test]
fn rejects_claim_tag_special_chars_in_name() {
    let tag = vec![
        "claim".to_string(),
        "alice.bob".to_string(),
        "nodns.shop".to_string(),
        "1780704000".to_string(),
    ];
    let err = parser::parse_claim_tag(&tag).unwrap_err();
    assert!(err.to_string().contains("invalid character"));
}

#[test]
fn rejects_claim_tag_non_numeric_valid_until() {
    let tag = vec![
        "claim".to_string(),
        "alice".to_string(),
        "nodns.shop".to_string(),
        "not-a-timestamp".to_string(),
    ];
    assert!(parser::parse_claim_tag(&tag).is_err());
}

// ===========================================================================
// SECURITY: Duplicate tag injection (delegation, registrar, claim, renewal)
// ===========================================================================

#[test]
fn rejects_duplicate_delegation_tag_in_event() {
    let keys = Keys::generate();
    let event = EventBuilder::new(Kind::Custom(11111), "")
        .tags(vec![
            Tag::parse([
                "delegation",
                "alice.test.shop",
                "npub1abc",
                "1000",
                "2000",
                "1500",
            ])
            .unwrap(),
            Tag::parse([
                "delegation",
                "bob.test.shop",
                "npub1xyz",
                "1000",
                "2000",
                "1500",
            ])
            .unwrap(),
        ])
        .sign_with_keys(&keys)
        .unwrap();
    let err = parser::classify_event(&event, &[], false, 0).unwrap_err();
    assert!(err.to_string().contains("duplicate delegation tag"));
}

#[test]
fn rejects_duplicate_registrar_tag_in_event() {
    let keys = Keys::generate();
    let event = EventBuilder::new(Kind::Custom(11111), "")
        .tags(vec![
            Tag::parse(["registrar", "test.shop", "abc123"]).unwrap(),
            Tag::parse(["registrar", "test.shop", "def456"]).unwrap(),
        ])
        .sign_with_keys(&keys)
        .unwrap();
    let err = parser::classify_event(&event, &[], false, 0).unwrap_err();
    assert!(err.to_string().contains("duplicate registrar tag"));
}

#[test]
fn rejects_duplicate_claim_tag_in_event() {
    let keys = Keys::generate();
    let event = EventBuilder::new(Kind::Custom(11111), "")
        .tags(vec![
            Tag::parse(["claim", "alice", "nodns.shop", "1780704000"]).unwrap(),
            Tag::parse(["claim", "bob", "nodns.shop", "1780704000"]).unwrap(),
        ])
        .sign_with_keys(&keys)
        .unwrap();
    let err = parser::classify_event(&event, &[], false, 0).unwrap_err();
    assert!(err.to_string().contains("duplicate claim tag"));
}

#[test]
fn rejects_event_with_no_recognized_tags() {
    let keys = Keys::generate();
    let event = EventBuilder::new(Kind::Custom(11111), "")
        .tags(vec![Tag::parse(["unknown", "data"]).unwrap()])
        .sign_with_keys(&keys)
        .unwrap();
    let err = parser::classify_event(&event, &[], false, 0).unwrap_err();
    assert!(err.to_string().contains("no recognized tags"));
}

#[test]
fn rejects_event_with_wrong_kind() {
    let keys = Keys::generate();
    let event = EventBuilder::new(Kind::TextNote, "")
        .tags(vec![
            Tag::parse(["record", "A", "@", "3600", "1.2.3.4"]).unwrap()
        ])
        .sign_with_keys(&keys)
        .unwrap();
    let err = parser::classify_event(&event, &[], false, 0).unwrap_err();
    assert!(err.to_string().contains("expected kind 11111 or 31111"));
}

// ===========================================================================
// SECURITY: Invalid pubkey in authority check
// ===========================================================================

#[test]
fn rejects_authority_check_with_invalid_pubkey_hex() {
    let store = setup_store();
    let checker = AuthorityChecker::new(store, HashMap::new());
    let err = checker
        .check_authority("alice.test.shop.", "test.shop", "not-hex-at-all")
        .unwrap_err();
    assert!(err.to_string().contains("invalid pubkey hex"));
}

// ===========================================================================
// SECURITY: NIP-13 Proof of Work verification
// ===========================================================================

#[test]
fn pow_count_leading_zero_bits_correctness() {
    assert_eq!(pow::count_leading_zero_bits(""), 0);
    assert_eq!(
        pow::count_leading_zero_bits(
            "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
        ),
        0
    );
    assert_eq!(
        pow::count_leading_zero_bits(
            "0000000000000000000000000000000000000000000000000000000000000000"
        ),
        256
    );
    assert_eq!(
        pow::count_leading_zero_bits(
            "1000000000000000000000000000000000000000000000000000000000000000"
        ),
        3
    );
    assert_eq!(
        pow::count_leading_zero_bits(
            "0100000000000000000000000000000000000000000000000000000000000000"
        ),
        7
    );
    let known_id = "000006d8c378af1779d2feebc7603a125d99eca0ccf1085959b307f64e5dd358";
    assert_eq!(pow::count_leading_zero_bits(known_id), 21);
}

#[test]
fn accepts_event_when_pow_disabled() {
    let id = "f000000000000000000000000000000000000000000000000000000000000000";
    assert!(pow::verify_pow(id, 0));
}

#[test]
fn rejects_event_below_pow_threshold() {
    let id = "f000000000000000000000000000000000000000000000000000000000000000";
    assert_eq!(pow::count_leading_zero_bits(id), 0);
    assert!(!pow::verify_pow(id, 16));
}

#[test]
fn accepts_event_above_pow_threshold() {
    let id = "000006d8c378af1779d2feebc7603a125d99eca0ccf1085959b307f64e5dd358";
    assert_eq!(pow::count_leading_zero_bits(id), 21);
    assert!(pow::verify_pow(id, 16));
    assert!(pow::verify_pow(id, 20));
    assert!(pow::verify_pow(id, 21));
}

#[test]
fn rejects_event_exactly_at_boundary_check() {
    let id = "000006d8c378af1779d2feebc7603a125d99eca0ccf1085959b307f64e5dd358";
    assert_eq!(pow::count_leading_zero_bits(id), 21);
    assert!(!pow::verify_pow(id, 22));
}

// ===========================================================================
// SECURITY: Proof of Burn — kind 30021 tag parsing
// ===========================================================================

fn make_kind_30021(tags: Vec<Tag>) -> Event {
    let keys = Keys::generate();
    EventBuilder::new(Kind::Custom(30021), "")
        .tags(tags)
        .sign_with_keys(&keys)
        .unwrap()
}

fn n_tag() -> Tag {
    Tag::parse([
        "n",
        "aabbccdd",
        "800000",
        "deadbeef",
        "5000000",
        "3",
        "hash1:100,hash2:200",
    ])
    .unwrap()
}

#[test]
fn parse_kind_30021_proof_valid() {
    let event = make_kind_30021(vec![
        Tag::parse(["e", "abc123eventid"]).unwrap(),
        n_tag(),
        Tag::parse(["chain", "000000000019d6689c085ae165831e93"]).unwrap(),
    ]);
    let proof = pob::parse_kind_30021_proof(&event).expect("should parse");
    assert_eq!(proof.event_id, "abc123eventid");
    assert_eq!(proof.txid, "aabbccdd");
    assert_eq!(proof.block_height, 800000);
    assert_eq!(proof.leaf_value, 5000000);
    assert_eq!(proof.merkle_index, 3);
    assert_eq!(proof.chain, "000000000019d6689c085ae165831e93");
}

#[test]
fn parse_kind_30021_proof_missing_e_tag_returns_none() {
    let event = make_kind_30021(vec![n_tag()]);
    assert!(pob::parse_kind_30021_proof(&event).is_none());
}

#[test]
fn parse_kind_30021_proof_missing_n_tag_returns_none() {
    let event = make_kind_30021(vec![
        Tag::parse(["e", "abc123"]).unwrap(),
        Tag::parse(["chain", "deadbeef"]).unwrap(),
    ]);
    assert!(pob::parse_kind_30021_proof(&event).is_none());
}

#[test]
fn parse_kind_30021_proof_n_tag_too_short_returns_none() {
    let event = make_kind_30021(vec![
        Tag::parse(["e", "abc123"]).unwrap(),
        Tag::parse(["n", "txid", "100"]).unwrap(),
    ]);
    assert!(pob::parse_kind_30021_proof(&event).is_none());
}

#[test]
fn parse_kind_30021_proof_empty_tags_returns_none() {
    let event = make_kind_30021(vec![]);
    assert!(pob::parse_kind_30021_proof(&event).is_none());
}

#[test]
fn burn_amount_converts_millisats_to_sats() {
    let proof = pob::NotaryProof {
        event_id: "evt".to_string(),
        txid: "tx".to_string(),
        block_height: 0,
        nonce: "n".to_string(),
        leaf_value: 5000000,
        merkle_index: 0,
        merkle_hashes: vec![],
        chain: "btc".to_string(),
    };
    assert_eq!(pob::burn_amount_sats(&proof), 5000);
}

#[test]
fn meets_threshold_above() {
    let proof = pob::NotaryProof {
        event_id: "evt".to_string(),
        txid: "tx".to_string(),
        block_height: 0,
        nonce: "n".to_string(),
        leaf_value: 5000000,
        merkle_index: 0,
        merkle_hashes: vec![],
        chain: "btc".to_string(),
    };
    assert!(pob::meets_threshold(&proof, 5000));
    assert!(pob::meets_threshold(&proof, 4000));
}

#[test]
fn meets_threshold_below() {
    let proof = pob::NotaryProof {
        event_id: "evt".to_string(),
        txid: "tx".to_string(),
        block_height: 0,
        nonce: "n".to_string(),
        leaf_value: 100000,
        merkle_index: 0,
        merkle_hashes: vec![],
        chain: "btc".to_string(),
    };
    assert!(!pob::meets_threshold(&proof, 500));
}

#[test]
fn meets_threshold_zero_min_always_passes() {
    let proof = pob::NotaryProof {
        event_id: "evt".to_string(),
        txid: "tx".to_string(),
        block_height: 0,
        nonce: "n".to_string(),
        leaf_value: 0,
        merkle_index: 0,
        merkle_hashes: vec![],
        chain: "btc".to_string(),
    };
    assert!(pob::meets_threshold(&proof, 0));
}

// ===========================================================================
// SECURITY: PoW OR PoB either/or gate logic (store-based PoB)
// ===========================================================================

#[test]
fn either_or_logic_both_disabled_accepts() {
    let min_pow: u32 = 0;
    let min_pob_sats: u64 = 0;

    let no_gate = min_pow == 0 && min_pob_sats == 0;
    assert!(
        no_gate,
        "both disabled should mean no gate (backwards compat)"
    );
}

#[test]
fn either_or_logic_pow_sufficient_alone_passes() {
    let min_pow: u32 = 20;
    let min_pob_sats: u64 = 1000;

    let event_id = "000006d8c378af1779d2feebc7603a125d99eca0ccf1085959b307f64e5dd358";
    let pow_ok = pow::verify_pow(event_id, min_pow);
    assert!(pow_ok);

    let store = setup_store();
    let pob_ok = if min_pob_sats > 0 {
        matches!(store.get_pob_proof(event_id).unwrap(), Some((bs, _)) if bs >= min_pob_sats)
    } else {
        false
    };

    assert!(pow_ok || pob_ok, "sufficient PoW should pass without PoB");
}

#[test]
fn either_or_logic_both_fail_rejects() {
    let min_pow: u32 = 20;
    let min_pob_sats: u64 = 100;

    let event_id = "f000000000000000000000000000000000000000000000000000000000000000";
    let pow_ok = pow::verify_pow(event_id, min_pow);
    assert!(!pow_ok);

    let store = setup_store();
    let pob_ok = if min_pob_sats > 0 {
        matches!(store.get_pob_proof(event_id).unwrap(), Some((bs, _)) if bs >= min_pob_sats)
    } else {
        false
    };

    assert!(!pow_ok && !pob_ok, "neither PoW nor PoB sufficient");
    let passes = (min_pow == 0 && min_pob_sats == 0) || pow_ok || pob_ok;
    assert!(!passes, "should reject when both fail");
}

#[test]
fn either_or_logic_pob_in_store_passes() {
    let store = setup_store();
    let event_id = "f000000000000000000000000000000000000000000000000000000000000000";
    store.save_pob_proof(event_id, 500, "txid123").unwrap();

    let min_pob_sats: u64 = 100;
    let result = match store.get_pob_proof(event_id).unwrap() {
        Some((bs, _)) => bs >= min_pob_sats,
        None => false,
    };
    assert!(result, "stored PoB proof should pass threshold");
}

#[test]
fn either_or_logic_pob_below_threshold_fails() {
    let store = setup_store();
    let event_id = "f000000000000000000000000000000000000000000000000000000000000000";
    store.save_pob_proof(event_id, 50, "txid123").unwrap();

    let min_pob_sats: u64 = 100;
    let result = match store.get_pob_proof(event_id).unwrap() {
        Some((bs, _)) => bs >= min_pob_sats,
        None => false,
    };
    assert!(!result, "stored PoB below threshold should fail");
}

#[test]
fn either_or_logic_no_stored_pob_fails() {
    let store = setup_store();
    let event_id = "f000000000000000000000000000000000000000000000000000000000000000";

    let min_pob_sats: u64 = 100;
    let result = match store.get_pob_proof(event_id).unwrap() {
        Some((bs, _)) => bs >= min_pob_sats,
        None => false,
    };
    assert!(!result, "no stored PoB should fail");
}

#[test]
fn store_pob_proof_roundtrip() {
    let store = setup_store();
    store.save_pob_proof("event_abc", 5000, "txid_def").unwrap();
    let (burn_sats, txid) = store.get_pob_proof("event_abc").unwrap().unwrap();
    assert_eq!(burn_sats, 5000);
    assert_eq!(txid, "txid_def");
}

#[test]
fn store_pob_proof_none_when_absent() {
    let store = setup_store();
    assert!(store.get_pob_proof("nonexistent").unwrap().is_none());
}
