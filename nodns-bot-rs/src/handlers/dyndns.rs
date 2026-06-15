use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Query, State as AxumState};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use base64::Engine;
use nostr_sdk::nips::nip19::ToBech32;
use nostr_sdk::Keys;
use serde::Deserialize;
use tracing::{error, info, warn};

use crate::AppState;

// ---------------------------------------------------------------------------
// DynDNS v2 compatible update handler
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct DynDnsUpdateParams {
    hostname: Option<String>,
    myip: Option<String>,
}

/// Plain-text response helper for `DynDNS` protocol.
fn dyndns_response(status_code: axum::http::StatusCode, body: &str) -> Response {
    (
        status_code,
        [(axum::http::header::CONTENT_TYPE, "text/plain")],
        body.to_string(),
    )
        .into_response()
}

pub async fn dyndns_update_handler(
    AxumState(state): AxumState<Arc<AppState>>,
    headers: HeaderMap,
    connect_info: axum::extract::ConnectInfo<SocketAddr>,
    Query(params): Query<DynDnsUpdateParams>,
) -> Response {
    let Some(auth_header) = headers.get("authorization").and_then(|v| v.to_str().ok()) else {
        return dyndns_response(axum::http::StatusCode::UNAUTHORIZED, "badauth");
    };

    let Some((username, password)) = parse_basic_auth(auth_header) else {
        return dyndns_response(axum::http::StatusCode::UNAUTHORIZED, "badauth");
    };

    if password.is_empty() {
        return dyndns_response(axum::http::StatusCode::UNAUTHORIZED, "badauth");
    }

    let keys = match Keys::parse(&password) {
        Ok(k) => k,
        Err(e) => {
            warn!(error = %e, "dyndns: failed to parse nsec");
            return dyndns_response(axum::http::StatusCode::UNAUTHORIZED, "badauth");
        }
    };

    let derived_npub = match keys.public_key().to_bech32() {
        Ok(n) => n,
        Err(e) => {
            warn!(error = %e, "dyndns: failed to encode derived npub");
            return dyndns_response(axum::http::StatusCode::UNAUTHORIZED, "badauth");
        }
    };
    let derived_pubkey_hex = keys.public_key().to_hex();

    if !username.is_empty() && username != derived_npub {
        return dyndns_response(axum::http::StatusCode::UNAUTHORIZED, "badauth");
    }

    let hostname = match params.hostname {
        Some(ref h) if !h.is_empty() => h.trim().to_lowercase(),
        _ => return dyndns_response(axum::http::StatusCode::BAD_REQUEST, "notfqdn"),
    };

    if !hostname.contains('.') {
        return dyndns_response(axum::http::StatusCode::BAD_REQUEST, "notfqdn");
    }

    let zone = match find_zone_for_hostname(&hostname, &state.dns_zones) {
        Some(z) => z.clone(),
        None => return dyndns_response(axum::http::StatusCode::BAD_REQUEST, "notfqdn"),
    };

    let zone_config = state.dns_zones.iter().find(|z| z.zone == zone);
    let default_ttl = zone_config.map_or(3600, |z| z.default_ttl);

    let zone_suffix = format!(".{zone}");
    let (name, is_npub_name) = if hostname.ends_with(&zone_suffix) {
        let prefix = &hostname[..hostname.len() - zone_suffix.len()];
        if prefix.is_empty() {
            return dyndns_response(axum::http::StatusCode::BAD_REQUEST, "notfqdn");
        }
        if prefix.starts_with("npub1") {
            if prefix != derived_npub {
                return dyndns_response(axum::http::StatusCode::FORBIDDEN, "nohost");
            }
            ("@".to_string(), true)
        } else {
            (prefix.to_string(), false)
        }
    } else {
        return dyndns_response(axum::http::StatusCode::BAD_REQUEST, "notfqdn");
    };

    if !is_npub_name {
        match state.store.get_active_delegation(&name, &zone) {
            Ok(Some(del)) => {
                if del.npub != derived_npub {
                    return dyndns_response(axum::http::StatusCode::FORBIDDEN, "nohost");
                }
            }
            Ok(None) => {
                return dyndns_response(axum::http::StatusCode::FORBIDDEN, "nohost");
            }
            Err(e) => {
                error!(error = %e, "dyndns: failed to check delegation");
                return dyndns_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, "911");
            }
        }
    }

    let ip_str = match &params.myip {
        Some(ip) if !ip.is_empty() => ip.clone(),
        _ => connect_info.0.ip().to_string(),
    };

    let (record_type_str, record_type_u16) = if ip_str.contains(':') {
        ("AAAA", 28)
    } else {
        ("A", 1)
    };

    if record_type_u16 == 1 {
        if ip_str.parse::<std::net::Ipv4Addr>().is_err() {
            return dyndns_response(axum::http::StatusCode::BAD_REQUEST, "notfqdn");
        }
    } else if ip_str.parse::<std::net::Ipv6Addr>().is_err() {
        return dyndns_response(axum::http::StatusCode::BAD_REQUEST, "notfqdn");
    }

    let fqdn = if name == "@" {
        format!("{derived_npub}.{zone}.")
    } else {
        format!("{name}.{zone}.")
    };

    let existing_records = state
        .store
        .get_records_by_pubkey(&derived_pubkey_hex)
        .unwrap_or_default();

    let current_ip = existing_records
        .iter()
        .find(|r| {
            r.record_type == record_type_str && r.name == name && r.zone == zone && !r.deleted
        })
        .map(|r| r.rdata.clone());

    if current_ip.as_deref() == Some(&ip_str) {
        return dyndns_response(axum::http::StatusCode::OK, &format!("nochg {ip_str}"));
    }

    let Some(updater) = state.updaters.get(&zone) else {
        error!(zone = %zone, "dyndns: no updater for zone");
        return dyndns_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, "911");
    };

    if let Err(e) = updater
        .update_record(&fqdn, default_ttl, record_type_u16, &ip_str)
        .await
    {
        error!(error = %e, fqdn = %fqdn, "dyndns: DDNS update failed");
        return dyndns_response(axum::http::StatusCode::INTERNAL_SERVER_ERROR, "911");
    }

    let event_id = format!("dyndns-{}", uuid::Uuid::new_v4());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    if let Err(e) = state.store.save_event(
        &event_id,
        &derived_npub,
        &derived_pubkey_hex,
        &name,
        record_type_str,
        default_ttl,
        &ip_str,
        &zone,
        now,
    ) {
        error!(error = %e, "dyndns: failed to save event to store");
    }

    info!(
        hostname = %hostname,
        ip = %ip_str,
        record_type = record_type_str,
        npub = %derived_npub,
        "dyndns: update successful"
    );

    dyndns_response(axum::http::StatusCode::OK, &format!("good {ip_str}"))
}

// ---------------------------------------------------------------------------
// DynDNS helpers
// ---------------------------------------------------------------------------

/// Parse a Basic Authorization header into (username, password).
fn parse_basic_auth(header: &str) -> Option<(String, String)> {
    let encoded = header.strip_prefix("Basic ")?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let decoded_str = String::from_utf8(decoded).ok()?;
    let (user, pass) = decoded_str.split_once(':')?;
    Some((user.to_string(), pass.to_string()))
}

/// Find which configured zone a hostname belongs to.
/// Returns the zone name (e.g. "nodns.shop") if the hostname ends with it.
fn find_zone_for_hostname<'a>(
    hostname: &str,
    zones: &'a [crate::config::ZoneConfig],
) -> Option<&'a String> {
    for zc in zones {
        let suffix = format!(".{}", zc.zone);
        if hostname.ends_with(&suffix) || hostname == zc.zone {
            return Some(&zc.zone);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use crate::handlers::test_helpers::*;

    use axum::body;
    use http::Request;
    use nostr_sdk::Keys;
    use tower::ServiceExt;

    // =======================================================================
    // parse_basic_auth
    // =======================================================================

    #[test]
    fn parse_basic_auth_valid_credentials() {
        let encoded = base64::engine::general_purpose::STANDARD.encode("user:pass");
        let header = format!("Basic {encoded}");
        assert_eq!(
            parse_basic_auth(&header),
            Some(("user".to_string(), "pass".to_string()))
        );
    }

    #[test]
    fn parse_basic_auth_missing_prefix() {
        let encoded = base64::engine::general_purpose::STANDARD.encode("user:pass");
        assert_eq!(parse_basic_auth(&encoded), None);
    }

    #[test]
    fn parse_basic_auth_invalid_base64() {
        assert_eq!(parse_basic_auth("Basic !!!invalid!!!"), None);
    }

    #[test]
    fn parse_basic_auth_no_colon_separator() {
        let encoded = base64::engine::general_purpose::STANDARD.encode("userpass");
        let header = format!("Basic {encoded}");
        assert_eq!(parse_basic_auth(&header), None);
    }

    #[test]
    fn parse_basic_auth_empty_password() {
        let encoded = base64::engine::general_purpose::STANDARD.encode("user:");
        let header = format!("Basic {encoded}");
        assert_eq!(
            parse_basic_auth(&header),
            Some(("user".to_string(), String::new()))
        );
    }

    #[test]
    fn parse_basic_auth_colon_in_password() {
        // split_once(':') splits on the FIRST colon only
        let encoded = base64::engine::general_purpose::STANDARD.encode("user:pass:word");
        let header = format!("Basic {encoded}");
        assert_eq!(
            parse_basic_auth(&header),
            Some(("user".to_string(), "pass:word".to_string()))
        );
    }

    // =======================================================================
    // find_zone_for_hostname
    // =======================================================================

    #[test]
    fn find_zone_hostname_subdomain() {
        let zones = vec![make_zone_config()];
        assert_eq!(
            find_zone_for_hostname("test.nodns.shop", &zones),
            Some(&"nodns.shop".to_string())
        );
    }

    #[test]
    fn find_zone_hostname_exact_zone() {
        let zones = vec![make_zone_config()];
        assert_eq!(
            find_zone_for_hostname("nodns.shop", &zones),
            Some(&"nodns.shop".to_string())
        );
    }

    #[test]
    fn find_zone_hostname_different_zone_returns_none() {
        let zones = vec![make_zone_config()];
        assert_eq!(find_zone_for_hostname("test.example.com", &zones), None);
    }

    #[test]
    fn find_zone_hostname_multiple_zones() {
        let zones = vec![
            make_zone_config(),
            config::ZoneConfig {
                zone: "example.com".to_string(),
                knot_address: "127.0.0.1:2".to_string(),
                tsig_key_name: "k2.".to_string(),
                tsig_key_secret: "dGVzdA==".to_string(),
                ..Default::default()
            },
        ];
        assert_eq!(
            find_zone_for_hostname("foo.example.com", &zones),
            Some(&"example.com".to_string())
        );
        assert_eq!(
            find_zone_for_hostname("bar.nodns.shop", &zones),
            Some(&"nodns.shop".to_string())
        );
    }

    #[test]
    fn find_zone_hostname_case_sensitive() {
        // find_zone_for_hostname does NOT lowercase — the handler does that
        // before calling. Documenting current behavior: case-sensitive match.
        let zones = vec![make_zone_config()];
        assert_eq!(
            find_zone_for_hostname("TEST.NODNS.SHOP", &zones),
            None,
            "hostname is case-sensitive; handler lowercases before calling"
        );
        assert_eq!(
            find_zone_for_hostname("test.nodns.shop", &zones),
            Some(&"nodns.shop".to_string())
        );
    }

    // =======================================================================
    // DynDNS v2 integration tests
    // =======================================================================

    #[tokio::test]
    async fn dyndns_valid_auth_no_updater_returns_911() {
        // With valid npub/nsec pair but no DNS updater for the zone,
        // auth passes but the handler returns 500 "911" (no updater).
        let state = create_test_state();
        let app = build_router(state);

        let keys = Keys::generate();
        let npub = keys.public_key().to_bech32().unwrap();
        let nsec = keys.secret_key().to_bech32().unwrap();
        let hostname = format!("{npub}.nodns.shop");

        let uri = format!("/nic/update?hostname={hostname}&myip=1.2.3.4");

        let mut request = Request::builder()
            .method("GET")
            .uri(&uri)
            .header("authorization", make_auth_header(&npub, &nsec))
            .body(body::Body::empty())
            .unwrap();
        request.extensions_mut().insert(make_connect_info());

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 500);
        let text = response_body(response).await;
        assert_eq!(text, "911");
    }

    #[tokio::test]
    async fn dyndns_wrong_nsec_returns_badauth() {
        let state = create_test_state();
        let app = build_router(state);

        let keys_correct = Keys::generate();
        let keys_wrong = Keys::generate();
        let npub_correct = keys_correct.public_key().to_bech32().unwrap();
        let nsec_wrong = keys_wrong.secret_key().to_bech32().unwrap();

        let uri = format!("/nic/update?hostname={npub_correct}.nodns.shop&myip=1.2.3.4");

        let mut request = Request::builder()
            .method("GET")
            .uri(&uri)
            .header(
                "authorization",
                make_auth_header(&npub_correct, &nsec_wrong),
            )
            .body(body::Body::empty())
            .unwrap();
        request.extensions_mut().insert(make_connect_info());

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 401);
        assert_eq!(response_body(response).await, "badauth");
    }

    #[tokio::test]
    async fn dyndns_empty_password_returns_badauth() {
        let state = create_test_state();
        let app = build_router(state);

        let keys = Keys::generate();
        let npub = keys.public_key().to_bech32().unwrap();

        let auth = base64::engine::general_purpose::STANDARD.encode(format!("{npub}:"));
        let uri = format!("/nic/update?hostname={npub}.nodns.shop&myip=1.2.3.4");

        let mut request = Request::builder()
            .method("GET")
            .uri(&uri)
            .header("authorization", format!("Basic {auth}"))
            .body(body::Body::empty())
            .unwrap();
        request.extensions_mut().insert(make_connect_info());

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 401);
        assert_eq!(response_body(response).await, "badauth");
    }

    #[tokio::test]
    async fn dyndns_missing_auth_returns_badauth() {
        let state = create_test_state();
        let app = build_router(state);

        let mut request = Request::builder()
            .method("GET")
            .uri("/nic/update?hostname=test.nodns.shop&myip=1.2.3.4")
            .body(body::Body::empty())
            .unwrap();
        request.extensions_mut().insert(make_connect_info());

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 401);
        assert_eq!(response_body(response).await, "badauth");
    }

    #[tokio::test]
    async fn dyndns_hostname_without_dot_returns_notfqdn() {
        let state = create_test_state();
        let app = build_router(state);

        let keys = Keys::generate();
        let npub = keys.public_key().to_bech32().unwrap();
        let nsec = keys.secret_key().to_bech32().unwrap();

        let mut request = Request::builder()
            .method("GET")
            .uri("/nic/update?hostname=test&myip=1.2.3.4")
            .header("authorization", make_auth_header(&npub, &nsec))
            .body(body::Body::empty())
            .unwrap();
        request.extensions_mut().insert(make_connect_info());

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 400);
        assert_eq!(response_body(response).await, "notfqdn");
    }

    #[tokio::test]
    async fn dyndns_hostname_not_in_managed_zone_returns_notfqdn() {
        let state = create_test_state();
        let app = build_router(state);

        let keys = Keys::generate();
        let npub = keys.public_key().to_bech32().unwrap();
        let nsec = keys.secret_key().to_bech32().unwrap();

        let mut request = Request::builder()
            .method("GET")
            .uri("/nic/update?hostname=test.example.com&myip=1.2.3.4")
            .header("authorization", make_auth_header(&npub, &nsec))
            .body(body::Body::empty())
            .unwrap();
        request.extensions_mut().insert(make_connect_info());

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 400);
        assert_eq!(response_body(response).await, "notfqdn");
    }

    #[tokio::test]
    async fn dyndns_custom_name_no_delegation_returns_nohost() {
        let state = create_test_state();
        let app = build_router(state);

        let keys = Keys::generate();
        let npub = keys.public_key().to_bech32().unwrap();
        let nsec = keys.secret_key().to_bech32().unwrap();

        let mut request = Request::builder()
            .method("GET")
            .uri("/nic/update?hostname=alice.nodns.shop&myip=1.2.3.4")
            .header("authorization", make_auth_header(&npub, &nsec))
            .body(body::Body::empty())
            .unwrap();
        request.extensions_mut().insert(make_connect_info());

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 403);
        assert_eq!(response_body(response).await, "nohost");
    }

    #[tokio::test]
    async fn dyndns_ipv6_address_accepted_through_auth() {
        // IPv6 address passes IP validation; fails later at updater (no updater)
        let state = create_test_state();
        let app = build_router(state);

        let keys = Keys::generate();
        let npub = keys.public_key().to_bech32().unwrap();
        let nsec = keys.secret_key().to_bech32().unwrap();
        let hostname = format!("{npub}.nodns.shop");

        let uri = format!("/nic/update?hostname={hostname}&myip=2001:db8::1");

        let mut request = Request::builder()
            .method("GET")
            .uri(&uri)
            .header("authorization", make_auth_header(&npub, &nsec))
            .body(body::Body::empty())
            .unwrap();
        request.extensions_mut().insert(make_connect_info());

        let response = app.oneshot(request).await.unwrap();
        // Auth + IP validation pass; fails at "no updater" → 911
        assert_eq!(response.status(), 500);
        assert_eq!(response_body(response).await, "911");
    }

    #[tokio::test]
    async fn dyndns_missing_myip_uses_connect_info_ip() {
        // When myip is absent, handler falls back to ConnectInfo IP (127.0.0.1).
        // Still reaches "no updater" → 911, proving auth passed.
        let state = create_test_state();
        let app = build_router(state);

        let keys = Keys::generate();
        let npub = keys.public_key().to_bech32().unwrap();
        let nsec = keys.secret_key().to_bech32().unwrap();
        let hostname = format!("{npub}.nodns.shop");

        let uri = format!("/nic/update?hostname={hostname}");

        let mut request = Request::builder()
            .method("GET")
            .uri(&uri)
            .header("authorization", make_auth_header(&npub, &nsec))
            .body(body::Body::empty())
            .unwrap();
        request.extensions_mut().insert(make_connect_info());

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 500);
        assert_eq!(response_body(response).await, "911");
    }

    #[tokio::test]
    async fn dyndns_with_updater_dns_push_fails_returns_911() {
        // Updater exists but points at 127.0.0.1:1 (nothing listening).
        // DNS push fails → 500 "911". This proves the full path up to DNS push.
        let state = create_test_state_with_updater();
        let app = build_router(state);

        let keys = Keys::generate();
        let npub = keys.public_key().to_bech32().unwrap();
        let nsec = keys.secret_key().to_bech32().unwrap();
        let hostname = format!("{npub}.nodns.shop");

        let uri = format!("/nic/update?hostname={hostname}&myip=1.2.3.4");

        let mut request = Request::builder()
            .method("GET")
            .uri(&uri)
            .header("authorization", make_auth_header(&npub, &nsec))
            .body(body::Body::empty())
            .unwrap();
        request.extensions_mut().insert(make_connect_info());

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 500);
        assert_eq!(response_body(response).await, "911");
    }
}
