mod acme_dns;
mod acme_order;
mod api;
mod client_log;
mod dyndns;
mod health;
mod llms;
mod tls_check;

// Re-exports to preserve handlers::* API used by main.rs
pub use acme_dns::{acmedns_register_handler, acmedns_update_handler};
pub use acme_order::{acme_cert_handler, acme_order_handler};
pub use api::{
    check_handler, records_by_npub_handler, records_by_prefix_handler, records_handler,
    zone_export, zone_pricing_handler, zone_records,
};
pub use client_log::client_log_handler;
pub use dyndns::dyndns_update_handler;
pub use health::health_handler;
pub use llms::{llms_full_txt_handler, llms_txt_handler};
pub use tls_check::tls_check_handler;

#[cfg(test)]
mod test_helpers {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Instant;

    use axum::Router;
    use base64::Engine;

    use crate::config;
    use crate::connector::DnsConnector;
    use crate::dns;
    use crate::nip05;
    use crate::types::Metrics;
    use crate::AppState;

    pub fn make_zone_config() -> config::ZoneConfig {
        config::ZoneConfig {
            knot_address: "127.0.0.1:1".to_string(),
            zone: "nodns.shop".to_string(),
            tsig_key_name: "test-key.".to_string(),
            tsig_key_secret: "dGVzdA==".to_string(),
            tsig_algorithm: "hmac-sha256".to_string(),
            default_ttl: 3600,
            ..Default::default()
        }
    }

    pub fn create_test_state() -> Arc<AppState> {
        let store = Arc::new(crate::store::Store::new(":memory:", None).unwrap());
        store.init().unwrap();

        let nip05_state = Arc::new(nip05::Nip05State {
            store: store.clone(),
            registrar_pubkeys: HashMap::new(),
            relays: vec![],
            zones: vec!["nodns.shop".to_string()],
        });

        Arc::new(AppState {
            store,
            nip05: nip05_state,
            acme: None,
            acme_environment: "staging".to_string(),
            metrics: Metrics::default(),
            start_time: Instant::now(),
            dns_zones: vec![make_zone_config()],
            updaters: Arc::new(HashMap::new()),
            nostr_client: nostr_sdk::Client::default(),
            relay_urls: vec![],
            db_path: std::path::PathBuf::from(":memory:"),
        })
    }

    pub fn create_test_state_with_updater() -> Arc<AppState> {
        let store = Arc::new(crate::store::Store::new(":memory:", None).unwrap());
        store.init().unwrap();

        let zc = make_zone_config();
        let updater = dns::Updater::new(&zc).unwrap();
        let mut updaters: HashMap<String, Arc<dyn DnsConnector>> = HashMap::new();
        updaters.insert("nodns.shop".to_string(), Arc::new(updater));

        let nip05_state = Arc::new(nip05::Nip05State {
            store: store.clone(),
            registrar_pubkeys: HashMap::new(),
            relays: vec![],
            zones: vec!["nodns.shop".to_string()],
        });

        Arc::new(AppState {
            store,
            nip05: nip05_state,
            acme: None,
            acme_environment: "staging".to_string(),
            metrics: Metrics::default(),
            start_time: Instant::now(),
            dns_zones: vec![zc],
            updaters: Arc::new(updaters),
            nostr_client: nostr_sdk::Client::default(),
            relay_urls: vec![],
            db_path: std::path::PathBuf::from(":memory:"),
        })
    }

    pub fn build_router(state: Arc<AppState>) -> Router {
        Router::new()
            .route(
                "/nic/update",
                axum::routing::get(super::dyndns::dyndns_update_handler)
                    .post(super::dyndns::dyndns_update_handler),
            )
            .route(
                "/register",
                axum::routing::post(super::acme_dns::acmedns_register_handler),
            )
            .route(
                "/update",
                axum::routing::post(super::acme_dns::acmedns_update_handler),
            )
            .with_state(state)
    }

    pub fn make_auth_header(npub: &str, nsec: &str) -> String {
        let credentials = format!("{npub}:{nsec}");
        let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
        format!("Basic {encoded}")
    }

    pub fn make_connect_info() -> axum::extract::ConnectInfo<std::net::SocketAddr> {
        axum::extract::ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 12345)))
    }

    pub async fn response_body(response: axum::response::Response) -> String {
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    pub async fn response_json(response: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    pub async fn register_acmedns(router: &Router) -> (String, String, String) {
        use tower::ServiceExt;

        let request = http::Request::builder()
            .method("POST")
            .uri("/register")
            .body(axum::body::Body::empty())
            .unwrap();
        let response = router.clone().oneshot(request).await.unwrap();
        let json = response_json(response).await;
        let subdomain = json["subdomain"].as_str().unwrap().to_string();
        let username = json["username"].as_str().unwrap().to_string();
        let password = json["password"].as_str().unwrap().to_string();
        (subdomain, username, password)
    }
}
