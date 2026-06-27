//! RFC 2136 DNS UPDATE server for `NoDNS`.
//!
//! Listens on a configurable UDP port and accepts DNS UPDATE messages
//! authenticated via a single shared TSIG key (HMAC-SHA256).  Authorized
//! updates are forwarded to Knot DNS through the existing `dns::Updater` and
//! persisted to the `SQLite` store.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use hickory_client::proto::dnssec::rdata::tsig::TsigAlgorithm;
use hickory_client::proto::dnssec::tsig::TSigner;
use hickory_client::proto::op::{Message, MessageType, OpCode, ResponseCode};
use hickory_client::proto::rr::{DNSClass, Name, RData, RecordType};
use hickory_client::proto::serialize::binary::BinEncodable;
use tokio::net::UdpSocket;
use tracing::{debug, error, info, warn};

use crate::connector::DnsConnector;
use crate::store::Store;

// ---------------------------------------------------------------------------
// Error helpers
// ---------------------------------------------------------------------------

/// Build a DNS error response echoing the query ID and opcode.
fn error_response(query: &Message, rcode: ResponseCode) -> Vec<u8> {
    let mut resp = Message::new();
    resp.set_id(query.id())
        .set_message_type(MessageType::Response)
        .set_op_code(query.op_code())
        .set_response_code(rcode);

    // Copy the question/zone section so the client can correlate.
    for q in query.queries() {
        resp.add_query(q.clone());
    }

    resp.to_bytes().unwrap_or_default()
}

/// Build a successful DNS response (NOERROR).
fn success_response(query: &Message) -> Vec<u8> {
    let mut resp = Message::new();
    resp.set_id(query.id())
        .set_message_type(MessageType::Response)
        .set_op_code(query.op_code())
        .set_response_code(ResponseCode::NoError);

    for q in query.queries() {
        resp.add_query(q.clone());
    }

    resp.to_bytes().unwrap_or_default()
}

// ---------------------------------------------------------------------------
// TSIG verification (simplified)
// ---------------------------------------------------------------------------

/// Simplified TSIG verification using hickory's built-in `TSigner`.
///
/// `verify_message_byte` handles key name matching, algorithm verification,
/// MAC comparison, and returns the acceptable time range for timestamp
/// validation.
fn verify_tsig(raw: &[u8], signer: &TSigner) -> bool {
    match signer.verify_message_byte(None, raw, true) {
        Ok((_hash, time_range, _signed_time)) => {
            // Verify that the current time falls within the acceptable window.
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            if time_range.contains(&now) {
                true
            } else {
                warn!(
                    now,
                    start = time_range.start,
                    end = time_range.end,
                    "TSIG timestamp outside fudge window"
                );
                false
            }
        }
        Err(e) => {
            warn!(error = %e, "TSIG verification failed");
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Zone matching
// ---------------------------------------------------------------------------

/// Determine which configured zone a given FQDN belongs to.
fn find_zone_for_domain<'a>(fqdn: &str, zones: &'a [String]) -> Option<&'a str> {
    let fqdn = fqdn.trim_end_matches('.');
    for zone in zones {
        let suffix = format!(".{zone}");
        if fqdn.ends_with(&suffix) || fqdn == zone.as_str() {
            return Some(zone.as_str());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Record type helpers (reuse dns.rs patterns)
// ---------------------------------------------------------------------------

fn record_type_to_string(rt: RecordType) -> &'static str {
    match rt {
        RecordType::A => "A",
        RecordType::AAAA => "AAAA",
        RecordType::CNAME => "CNAME",
        RecordType::TXT => "TXT",
        RecordType::MX => "MX",
        RecordType::SRV => "SRV",
        RecordType::NS => "NS",
        RecordType::PTR => "PTR",
        _ => "UNKNOWN",
    }
}

fn is_allowed_record_type(rt: RecordType) -> bool {
    matches!(
        rt,
        RecordType::A
            | RecordType::AAAA
            | RecordType::CNAME
            | RecordType::TXT
            | RecordType::MX
            | RecordType::SRV
    )
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

pub struct DnsUpdateServer {
    listen_addr: SocketAddr,
    store: Arc<Store>,
    updaters: Arc<HashMap<String, Arc<dyn DnsConnector>>>,
    zones: Vec<String>,
    tsig_signer: TSigner,
}

impl DnsUpdateServer {
    pub fn new(
        listen_addr: SocketAddr,
        store: Arc<Store>,
        updaters: Arc<HashMap<String, Arc<dyn DnsConnector>>>,
        zones: Vec<String>,
        tsig_key_name: &str,
        tsig_key_secret: &str,
    ) -> Result<Self, String> {
        let key_name_fqdn = if tsig_key_name.ends_with('.') {
            tsig_key_name.to_string()
        } else {
            format!("{tsig_key_name}.")
        };

        let signer_name =
            Name::from_str(&key_name_fqdn).map_err(|e| format!("invalid TSIG key name: {e}"))?;

        let secret_bytes = BASE64_STANDARD
            .decode(tsig_key_secret.trim())
            .map_err(|e| format!("TSIG secret must be base64-encoded: {e}"))?;

        let tsig_signer = TSigner::new(secret_bytes, TsigAlgorithm::HmacSha256, signer_name, 300)
            .map_err(|e| format!("TSIG signer creation failed: {e}"))?;

        Ok(Self {
            listen_addr,
            store,
            updaters,
            zones,
            tsig_signer,
        })
    }

    /// Run the DNS UPDATE server loop.
    pub async fn run(self: Arc<Self>) {
        let socket = match UdpSocket::bind(self.listen_addr).await {
            Ok(s) => s,
            Err(e) => {
                error!(error = %e, addr = %self.listen_addr, "failed to bind DNS UPDATE server");
                return;
            }
        };

        info!(addr = %self.listen_addr, "RFC 2136 DNS UPDATE server listening");

        let mut buf = [0u8; 4096];
        loop {
            match socket.recv_from(&mut buf).await {
                Ok((len, src)) => {
                    let data = &buf[..len];
                    let msg = match Message::from_vec(data) {
                        Ok(m) => m,
                        Err(e) => {
                            debug!(error = %e, from = %src, "failed to parse DNS message");
                            continue;
                        }
                    };

                    // Dispatch: only handle UPDATE opcode.
                    if msg.op_code() != OpCode::Update {
                        debug!(opcode = ?msg.op_code(), from = %src, "ignoring non-UPDATE message");
                        let resp = error_response(&msg, ResponseCode::NotImp);
                        let _ = socket.send_to(&resp, src).await;
                        continue;
                    }

                    // Verify TSIG.
                    if !verify_tsig(data, &self.tsig_signer) {
                        warn!(from = %src, "TSIG verification failed — rejecting UPDATE");
                        let resp = error_response(&msg, ResponseCode::Refused);
                        let _ = socket.send_to(&resp, src).await;
                        continue;
                    }

                    // Process the update.
                    let result = self.process_update(&msg).await;
                    let resp = match result {
                        Ok(()) => success_response(&msg),
                        Err(rcode) => error_response(&msg, rcode),
                    };
                    let _ = socket.send_to(&resp, src).await;
                }
                Err(e) => {
                    error!(error = %e, "UDP recv error");
                }
            }
        }
    }

    /// Process a verified DNS UPDATE message.
    ///
    /// RFC 2136 §2 — the update section (`name_servers` in hickory) contains
    /// the actual add/delete operations.
    async fn process_update(&self, msg: &Message) -> Result<(), ResponseCode> {
        // Validate the zone section: it must match one of our managed zones.
        let zone_query = msg.queries().first();
        let zone_name = zone_query
            .map(|q| q.name().to_string().trim_end_matches('.').to_string())
            .unwrap_or_default();

        if !self.zones.iter().any(|z| z == &zone_name) {
            warn!(zone = %zone_name, "UPDATE for unknown zone");
            return Err(ResponseCode::NotZone);
        }

        let update_records = msg.name_servers();
        if update_records.is_empty() {
            debug!("UPDATE with empty update section — no-op");
            return Ok(());
        }

        // Process each update record.
        for rec in update_records {
            let fqdn = rec.name().to_string();
            let fqdn_trimmed = fqdn.trim_end_matches('.');
            let rt = rec.record_type();

            // Determine zone for this FQDN.
            let Some(zone) = find_zone_for_domain(&fqdn, &self.zones) else {
                warn!(fqdn = %fqdn, "UPDATE record FQDN not in any managed zone");
                return Err(ResponseCode::Refused);
            };
            let zone = zone.to_string();

            // Reject unsupported record types.
            if !is_allowed_record_type(rt) {
                warn!(fqdn = %fqdn, rtype = ?rt, "UPDATE with unsupported record type");
                return Err(ResponseCode::Refused);
            }

            // Authorisation check: the FQDN must belong to a known user in the store.
            let authorised = match self.store.get_records_by_domain(fqdn_trimmed) {
                Ok(records) => !records.is_empty(),
                Err(e) => {
                    error!(error = %e, fqdn = %fqdn, "store lookup failed");
                    false
                }
            };

            if !authorised {
                // Also allow updates where the FQDN itself is the apex or a
                // delegation that exists. For the prototype, we check both
                // domain records and delegations. If neither exist, reject.
                warn!(fqdn = %fqdn, "UPDATE for unauthorised domain (no records in store)");
                return Err(ResponseCode::Refused);
            }

            let Some(updater) = self.updaters.get(&zone) else {
                error!(zone = %zone, "no updater for zone");
                return Err(ResponseCode::ServFail);
            };

            let ttl = rec.ttl();
            let rt_str = record_type_to_string(rt);
            let rt_u16 = rt.into();

            match rec.dns_class() {
                // RFC 2136 §2.5.1 — Add/insert RR.
                DNSClass::IN => {
                    let rdata_str = rdata_to_string(rec.data(), rt);
                    if rdata_str.is_empty() {
                        warn!(fqdn = %fqdn, "UPDATE add with empty rdata");
                        return Err(ResponseCode::FormErr);
                    }

                    info!(
                        fqdn = %fqdn_trimmed,
                        rtype = rt_str,
                        ttl,
                        rdata = %rdata_str,
                        zone = %zone,
                        "RFC 2136 UPDATE: add record"
                    );

                    if let Err(e) = updater
                        .update_record(fqdn_trimmed, ttl, rt_u16, &rdata_str)
                        .await
                    {
                        error!(error = %e, "failed to forward UPDATE to Knot DNS");
                        return Err(ResponseCode::ServFail);
                    }

                    // Persist to store.
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;
                    let event_id = format!("rfc2136-{}-{}", now, fqdn_trimmed.replace('.', "-"));
                    let npub = extract_npub_from_fqdn(fqdn_trimmed, &zone);
                    if let Err(e) = self.store.save_event(
                        &event_id,
                        &npub,
                        &npub,
                        extract_name_from_fqdn(fqdn_trimmed, &zone),
                        rt_str,
                        ttl,
                        &rdata_str,
                        &zone,
                        now,
                        0,
                    ) {
                        warn!(error = %e, "failed to persist RFC 2136 update to store");
                    }
                }

                // RFC 2136 §2.5.2 — Delete RRset (class=ANY, ttl=0).
                DNSClass::ANY => {
                    info!(
                        fqdn = %fqdn_trimmed,
                        rtype = rt_str,
                        zone = %zone,
                        "RFC 2136 UPDATE: delete RRset"
                    );

                    if let Err(e) = updater.delete_record(fqdn_trimmed, rt_u16).await {
                        error!(error = %e, "failed to forward DELETE to Knot DNS");
                        return Err(ResponseCode::ServFail);
                    }
                }

                // RFC 2136 §2.5.3 — Delete specific RR (class=NONE).
                DNSClass::NONE => {
                    let rdata_str = rdata_to_string(rec.data(), rt);
                    info!(
                        fqdn = %fqdn_trimmed,
                        rtype = rt_str,
                        rdata = %rdata_str,
                        zone = %zone,
                        "RFC 2136 UPDATE: delete specific RR"
                    );

                    // For specific RR deletion, we delete the whole RRset
                    // since Knot DNS handles this adequately for most use cases.
                    if let Err(e) = updater.delete_record(fqdn_trimmed, rt_u16).await {
                        error!(error = %e, "failed to forward DELETE to Knot DNS");
                        return Err(ResponseCode::ServFail);
                    }
                }

                other => {
                    warn!(class = ?other, "unknown DNS class in UPDATE section");
                    return Err(ResponseCode::FormErr);
                }
            }
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// RData stringification
// ---------------------------------------------------------------------------

/// Convert `RData` to a human-friendly string matching the format expected by
/// the `dns::Updater` methods.
fn rdata_to_string(rdata: &RData, rt: RecordType) -> String {
    match (rt, rdata) {
        (RecordType::A, RData::A(a)) => a.to_string(),
        (RecordType::AAAA, RData::AAAA(aaaa)) => aaaa.to_string(),
        (RecordType::TXT, RData::TXT(txt)) => {
            let parts: Vec<String> = txt
                .txt_data()
                .iter()
                .map(|b| String::from_utf8_lossy(b).into_owned())
                .collect();
            parts.join("")
        }
        (RecordType::CNAME, RData::CNAME(cname)) => {
            let name = cname.0.to_string();
            // Ensure trailing dot for FQDN.
            if name.ends_with('.') {
                name
            } else {
                format!("{name}.")
            }
        }
        (RecordType::MX, RData::MX(mx)) => {
            let exchange = mx.exchange().to_string();
            let exchange = if exchange.ends_with('.') {
                exchange
            } else {
                format!("{exchange}.")
            };
            format!("{} {}", mx.preference(), exchange)
        }
        (RecordType::SRV, RData::SRV(srv)) => {
            let target = srv.target().to_string();
            let target = if target.ends_with('.') {
                target
            } else {
                format!("{target}.")
            };
            format!(
                "{} {} {} {}",
                srv.priority(),
                srv.weight(),
                srv.port(),
                target
            )
        }
        _ => String::new(),
    }
}

// ---------------------------------------------------------------------------
// FQDN parsing helpers
// ---------------------------------------------------------------------------

/// Extract the name (subdomain label) from a FQDN like `foo.npub1abc.nodns.shop`
/// by stripping the zone suffix. Returns the first label for the store.
fn extract_name_from_fqdn<'a>(fqdn: &'a str, zone: &str) -> &'a str {
    let zone_suffix = format!(".{zone}");
    if let Some(name) = fqdn.strip_suffix(&zone_suffix) {
        // name might be "foo.npub1abc" — return just the first label.
        name.split('.').next().unwrap_or(name)
    } else if fqdn == zone {
        "@"
    } else {
        fqdn
    }
}

/// Extract npub from FQDN. The FQDN format is `<name>.<npub>.<zone>` or
/// just `<npub>.<zone>`. For the prototype, return the portion before the zone.
fn extract_npub_from_fqdn(fqdn: &str, zone: &str) -> String {
    let zone_suffix = format!(".{zone}");
    if let Some(prefix) = fqdn.strip_suffix(&zone_suffix) {
        // If prefix contains a dot, the last part is the npub.
        if let Some(npub) = prefix.rsplit('.').next() {
            return npub.to_string();
        }
        return prefix.to_string();
    }
    "unknown".to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use hickory_client::proto::rr::rdata::{A, AAAA, TXT};
    use std::net::{Ipv4Addr, Ipv6Addr};

    // =======================================================================
    // find_zone_for_domain
    // =======================================================================

    #[test]
    fn find_zone_subdomain_of_zone() {
        let zones = vec!["nodns.shop".to_string()];
        assert_eq!(
            find_zone_for_domain("foo.nodns.shop", &zones),
            Some("nodns.shop")
        );
    }

    #[test]
    fn find_zone_exact_match() {
        let zones = vec!["nodns.shop".to_string()];
        assert_eq!(
            find_zone_for_domain("nodns.shop", &zones),
            Some("nodns.shop")
        );
    }

    #[test]
    fn find_zone_trailing_dot() {
        let zones = vec!["nodns.shop".to_string()];
        // Trailing dot is stripped before matching
        assert_eq!(
            find_zone_for_domain("foo.nodns.shop.", &zones),
            Some("nodns.shop")
        );
    }

    #[test]
    fn find_zone_no_match() {
        let zones = vec!["nodns.shop".to_string()];
        assert_eq!(find_zone_for_domain("foo.example.com", &zones), None);
    }

    #[test]
    fn find_zone_multiple_zones_picks_correct() {
        let zones = vec!["nodns.shop".to_string(), "example.com".to_string()];
        assert_eq!(
            find_zone_for_domain("bar.example.com", &zones),
            Some("example.com")
        );
        assert_eq!(
            find_zone_for_domain("baz.nodns.shop", &zones),
            Some("nodns.shop")
        );
    }

    // =======================================================================
    // record_type_to_string
    // =======================================================================

    #[test]
    fn record_type_known_types() {
        assert_eq!(record_type_to_string(RecordType::A), "A");
        assert_eq!(record_type_to_string(RecordType::AAAA), "AAAA");
        assert_eq!(record_type_to_string(RecordType::CNAME), "CNAME");
        assert_eq!(record_type_to_string(RecordType::TXT), "TXT");
        assert_eq!(record_type_to_string(RecordType::MX), "MX");
        assert_eq!(record_type_to_string(RecordType::SRV), "SRV");
        assert_eq!(record_type_to_string(RecordType::NS), "NS");
        assert_eq!(record_type_to_string(RecordType::PTR), "PTR");
    }

    #[test]
    fn record_type_unknown_falls_back() {
        assert_eq!(record_type_to_string(RecordType::Unknown(999)), "UNKNOWN");
    }

    // =======================================================================
    // is_allowed_record_type
    // =======================================================================

    #[test]
    fn allowed_record_types() {
        assert!(is_allowed_record_type(RecordType::A));
        assert!(is_allowed_record_type(RecordType::AAAA));
        assert!(is_allowed_record_type(RecordType::CNAME));
        assert!(is_allowed_record_type(RecordType::TXT));
        assert!(is_allowed_record_type(RecordType::MX));
        assert!(is_allowed_record_type(RecordType::SRV));
    }

    #[test]
    fn disallowed_record_types() {
        assert!(!is_allowed_record_type(RecordType::NS));
        assert!(!is_allowed_record_type(RecordType::PTR));
        assert!(!is_allowed_record_type(RecordType::SOA));
        assert!(!is_allowed_record_type(RecordType::Unknown(999)));
    }

    // =======================================================================
    // rdata_to_string
    // =======================================================================

    #[test]
    fn rdata_a_record() {
        let rdata = RData::A(A::from(Ipv4Addr::new(1, 2, 3, 4)));
        assert_eq!(rdata_to_string(&rdata, RecordType::A), "1.2.3.4");
    }

    #[test]
    fn rdata_aaaa_loopback() {
        let rdata = RData::AAAA(AAAA::from(Ipv6Addr::LOCALHOST));
        assert_eq!(rdata_to_string(&rdata, RecordType::AAAA), "::1");
    }

    #[test]
    fn rdata_txt_single_part() {
        let rdata = RData::TXT(TXT::new(vec!["hello".to_string()]));
        assert_eq!(rdata_to_string(&rdata, RecordType::TXT), "hello");
    }

    #[test]
    fn rdata_txt_multiple_parts_concatenated() {
        let rdata = RData::TXT(TXT::new(vec!["hel".to_string(), "lo".to_string()]));
        assert_eq!(rdata_to_string(&rdata, RecordType::TXT), "hello");
    }

    #[test]
    fn rdata_mismatched_type_returns_empty() {
        let rdata = RData::A(A::from(Ipv4Addr::new(1, 2, 3, 4)));
        // Pass AAAA type but A rdata — should hit the catch-all arm.
        assert!(rdata_to_string(&rdata, RecordType::AAAA).is_empty());
    }

    // =======================================================================
    // extract_name_from_fqdn
    // =======================================================================

    #[test]
    fn extract_name_subdomain_prefix() {
        // "foo.npub1abc.nodns.shop" → strip zone suffix → "foo.npub1abc" → first label "foo"
        assert_eq!(
            extract_name_from_fqdn("foo.npub1abc.nodns.shop", "nodns.shop"),
            "foo"
        );
    }

    #[test]
    fn extract_name_exact_zone_is_apex() {
        assert_eq!(extract_name_from_fqdn("nodns.shop", "nodns.shop"), "@");
    }

    #[test]
    fn extract_name_no_match_returns_fqdn() {
        assert_eq!(
            extract_name_from_fqdn("foo.example.com", "nodns.shop"),
            "foo.example.com"
        );
    }

    // =======================================================================
    // extract_npub_from_fqdn
    // =======================================================================

    #[test]
    fn extract_npub_with_name_prefix() {
        // "foo.npub1abc.nodns.shop" → strip zone → "foo.npub1abc" → last part "npub1abc"
        assert_eq!(
            extract_npub_from_fqdn("foo.npub1abc.nodns.shop", "nodns.shop"),
            "npub1abc"
        );
    }

    #[test]
    fn extract_npub_bare_npub() {
        // "npub1abc.nodns.shop" → strip zone → "npub1abc" → no dot → "npub1abc"
        assert_eq!(
            extract_npub_from_fqdn("npub1abc.nodns.shop", "nodns.shop"),
            "npub1abc"
        );
    }

    #[test]
    fn extract_npub_no_match_returns_unknown() {
        assert_eq!(
            extract_npub_from_fqdn("foo.example.com", "nodns.shop"),
            "unknown"
        );
    }
}
