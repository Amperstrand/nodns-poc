use std::process::Command;

fn nodns_bin() -> String {
    "./target/debug/nodns".to_string()
}

fn run_resolve(args: &[&str]) -> String {
    let output = Command::new(nodns_bin())
        .arg("resolve")
        .args(args)
        .output()
        .expect("failed to run nodns resolve");
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn run_resolve_stderr(args: &[&str]) -> String {
    let output = Command::new(nodns_bin())
        .arg("resolve")
        .args(args)
        .output()
        .expect("failed to run nodns resolve");
    String::from_utf8_lossy(&output.stderr).to_string()
}

static KNOWN_NPUB: &str = "npub1hw6amg8p24ne08c9gdq8hhpqx0t0pwanpae9z25crn7m9uy7yarse465gr";
static KNOWN_DOMAIN: &str = "npub1hw6amg8p24ne08c9gdq8hhpqx0t0pwanpae9z25crn7m9uy7yarse465gr.nodns.shop";

#[test]
fn resolve_api_returns_records() {
    let stdout = run_resolve(&[KNOWN_DOMAIN]);
    assert!(stdout.contains(";; API RECORDS"), "expected API header, got:\n{stdout}");
    assert!(stdout.contains("A\t"), "expected A record in output, got:\n{stdout}");
    assert!(stdout.contains("217.77.8.91"), "expected known IP, got:\n{stdout}");
}

#[test]
fn resolve_api_shows_record_count() {
    let stdout = run_resolve(&[KNOWN_DOMAIN]);
    assert!(stdout.contains("record(s)"), "expected record count, got:\n{stdout}");
}

#[test]
fn resolve_dns_only_skips_api() {
    let stdout = run_resolve(&[KNOWN_DOMAIN, "--dns-only"]);
    assert!(!stdout.contains("API RECORDS"), "dns-only should not query API, got:\n{stdout}");
    assert!(stdout.contains("DNS ANSWER"), "expected DNS header, got:\n{stdout}");
}

#[test]
fn resolve_dns_only_no_api_query_header() {
    let stdout = run_resolve(&[KNOWN_DOMAIN, "--dns-only"]);
    assert!(!stdout.contains(";; API RECORDS"), "dns-only should not show API header, got:\n{stdout}");
    assert!(stdout.contains(";; DNS ANSWER"), "expected DNS header, got:\n{stdout}");
}

#[test]
fn resolve_nostr_mode() {
    let stdout = run_resolve(&[KNOWN_DOMAIN, "--nostr"]);
    assert!(stdout.contains("NOSTR EVENTS"), "expected Nostr header, got:\n{stdout}");
    assert!(!stdout.contains("API RECORDS"), "nostr mode should not query API, got:\n{stdout}");
}

#[test]
fn resolve_type_filter() {
    let stdout = run_resolve(&[KNOWN_DOMAIN, "-t", "TXT"]);
    assert!(stdout.contains(";; API RECORDS"), "expected API header, got:\n{stdout}");
    assert!(!stdout.contains("\tA\t"), "A records should be filtered out with -t TXT, got:\n{stdout}");
}

#[test]
fn resolve_unknown_domain_returns_empty_or_fallback() {
    let stdout = run_resolve(&["nonexistent.nodns.shop"]);
    assert!(
        stdout.contains("no records found") || stdout.contains("DNS ANSWER") || stdout.contains("record(s)"),
        "expected empty or fallback response, got:\n{stdout}"
    );
}

#[test]
fn resolve_custom_api_base() {
    let stdout = run_resolve(&[KNOWN_DOMAIN, "--api-base", "https://nodns.shop"]);
    assert!(stdout.contains("nodns.shop"), "expected custom API base in output, got:\n{stdout}");
}

#[test]
fn resolve_help_flag() {
    let output = Command::new(nodns_bin())
        .arg("resolve")
        .arg("--help")
        .output()
        .expect("failed to run nodns resolve --help");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(stdout.contains("--dns-only"), "expected --dns-only in help, got:\n{stdout}");
    assert!(stdout.contains("--nostr"), "expected --nostr in help, got:\n{stdout}");
    assert!(stdout.contains("--api-base"), "expected --api-base in help, got:\n{stdout}");
}
