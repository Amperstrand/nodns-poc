//! NIP-05 Nostr identity endpoint.
//!
//! Serves `/.well-known/nostr.json?name=<user>` to map usernames to Nostr
//! pubkeys per [NIP-05](https://github.com/nostr-protocol/nips/blob/master/05.md).
//!
//! Lookup order:
//! 1. `_` or empty → registrar key (the bot itself)
//! 2. Exact npub match (e.g. `npub1abc...xyz`)
//! 3. npub prefix (first 8+ hex chars of pubkey)
//! 4. Delegated domain name (e.g. `alice`)

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Query, State as AxumState};
use axum::http::HeaderValue;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use crate::store::Store;

// ---------------------------------------------------------------------------
// Query parameters
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct Nip05Query {
    pub name: Option<String>,
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct Nip05Response {
    pub names: HashMap<String, String>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub relays: HashMap<String, Vec<String>>,
}

// ---------------------------------------------------------------------------
// NIP-05 handler state
// ---------------------------------------------------------------------------

/// Shared state needed by the NIP-05 handler.
pub struct Nip05State {
    pub store: Arc<Store>,
    /// Registrar pubkeys per zone (hex).
    pub registrar_pubkeys: HashMap<String, String>,
    /// Relay URLs from config.
    pub relays: Vec<String>,
    pub zones: Vec<String>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// `GET /.well-known/nostr.json?name=<user>`
pub async fn nip05_handler(
    AxumState(state): AxumState<Arc<crate::AppState>>,
    Query(params): Query<Nip05Query>,
) -> Response {
    let nip05 = &state.nip05;
    let name = params.name.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty());

    // No name provided → return registrar key
    let Some(name) = name else {
        return registrar_response(nip05, "_");
    };

    // `_` → return registrar key
    if name == "_" {
        return registrar_response(nip05, "_");
    }

    // Try exact npub match
    if let Some(response) = lookup_by_npub(nip05, name) {
        return response;
    }

    // Try npub prefix (hex pubkey prefix, 8+ chars)
    if let Some(response) = lookup_by_pubkey_prefix(nip05, name) {
        return response;
    }

    // Try delegated name
    if let Some(response) = lookup_by_delegation(nip05, name) {
        return response;
    }

    // No match → return empty names with CORS headers
    empty_response()
}

// ---------------------------------------------------------------------------
// Lookup helpers
// ---------------------------------------------------------------------------

/// Return the NIP-05 response for the registrar key.
fn registrar_response(state: &Nip05State, name: &str) -> Response {
    let mut names = HashMap::new();
    let mut relays = HashMap::new();

    // Deterministic selection: sort registrar keys lexicographically, pick first
    let hex = state
        .registrar_pubkeys
        .keys()
        .min()
        .and_then(|key| state.registrar_pubkeys.get(key));

    if let Some(hex) = hex {
        names.insert(name.to_string(), hex.clone());
        if !state.relays.is_empty() {
            relays.insert(hex.clone(), state.relays.clone());
        }
    }

    let body = Nip05Response { names, relays };
    cors_json(&body)
}

/// Look up by exact npub (e.g. `npub1abc...xyz`).
fn lookup_by_npub(state: &Nip05State, name: &str) -> Option<Response> {
    let records = state.store.get_records_by_npub_exact(name).ok()?;

    for record in &records {
        if record.npub == name {
            return Some(build_match(&state.relays, &record.pubkey));
        }
    }
    None
}

/// Look up by hex pubkey prefix (first 8+ chars).
fn lookup_by_pubkey_prefix(state: &Nip05State, prefix: &str) -> Option<Response> {
    if prefix.len() < 8 {
        return None;
    }
    if !prefix.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }

    let prefix_lower = prefix.to_lowercase();
    let records = state.store.lookup_by_pubkey_prefix(&prefix_lower).ok()?;

    for record in &records {
        if record.pubkey.to_lowercase().starts_with(&prefix_lower) {
            return Some(build_match(&state.relays, &record.pubkey));
        }
    }
    None
}

/// Look up by delegated domain name (e.g. `alice` → delegation where domain="alice").
fn lookup_by_delegation(state: &Nip05State, name: &str) -> Option<Response> {
    for zone in &state.zones {
        if let Ok(Some(del)) = state.store.get_active_delegation(name, zone) {
            return Some(build_match(&state.relays, &del.pubkey));
        }
    }
    None
}

/// Build a NIP-05 match response for a single user.
fn build_match(relays: &[String], hex_pubkey: &str) -> Response {
    let mut names = HashMap::new();
    let mut relay_map = HashMap::new();

    names.insert("_".to_string(), hex_pubkey.to_string());
    if !relays.is_empty() {
        relay_map.insert(hex_pubkey.to_string(), relays.to_vec());
    }

    let body = Nip05Response { names, relays: relay_map };
    cors_json(&body)
}

/// Return an empty NIP-05 response (no matches).
fn empty_response() -> Response {
    let body = Nip05Response {
        names: HashMap::new(),
        relays: HashMap::new(),
    };
    cors_json(&body)
}

// ---------------------------------------------------------------------------
// CORS response helper
// ---------------------------------------------------------------------------

/// Serialize a NIP-05 response as JSON with CORS headers.
fn cors_json(body: &Nip05Response) -> Response {
    use axum::http::header::{ACCESS_CONTROL_ALLOW_ORIGIN, CONTENT_TYPE};

    let mut resp = axum::Json(body).into_response();
    let headers = resp.headers_mut();
    headers.insert(ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    resp
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_nip05_response_relays_is_empty() {
        let mut names = HashMap::new();
        names.insert("_".to_string(), "abc".to_string());
        let resp = Nip05Response {
            names,
            relays: HashMap::new(),
        };
        assert!(resp.relays.is_empty());
    }

    #[test]
    fn registrar_pubkey_selection_is_deterministic() {
        let mut pubkeys: HashMap<String, String> = HashMap::new();
        pubkeys.insert("zone-b".to_string(), "bbbb".to_string());
        pubkeys.insert("zone-a".to_string(), "aaaa".to_string());
        pubkeys.insert("zone-c".to_string(), "cccc".to_string());

        let selected_key = pubkeys.keys().min().unwrap();
        let selected_hex = pubkeys.get(selected_key).unwrap();

        // Repeating the same selection must always yield the same result
        for _ in 0..10 {
            let key = pubkeys.keys().min().unwrap();
            assert_eq!(key, "zone-a");
            assert_eq!(pubkeys.get(key).unwrap(), selected_hex);
        }
    }
}
