//! Experimental Nostr-over-DNS event caching.
//!
//! Caches selected Nostr events as DNS TXT records so they can be retrieved
//! via a standard DNS query (e.g. `dig note1abc...nodns.shop TXT +short`).
//!
//! All operations are non-fatal — failures are logged as warnings and do not
//! affect the main event processing pipeline.

use nostr_sdk::nips::nip19::ToBech32;
use nostr_sdk::Event;
use tracing::warn;

use nodns_connectors::connector::DnsConnector;
const TXT_SEGMENT_MAX: usize = 255;

pub struct DnsEventCache;

impl DnsEventCache {
    pub async fn cache_event(
        backend: &dyn DnsConnector,
        event: &Event,
        zone: &str,
    ) -> Result<(), String> {
        let note_id = event
            .id
            .to_bech32()
            .map_err(|e| format!("failed to encode event id as note: {e}"))?;

        let fqdn = format!("{}.{}.", note_id, zone);
        let json = compact_event_json(event);
        let segments = split_txt_segments(&json);

        backend
            .update_txt_multi(&fqdn, 3600, &segments)
            .await
            .map_err(|e| format!("DNS cache write failed for {fqdn}: {e}"))
    }

    pub async fn cache_profile(
        backend: &dyn DnsConnector,
        event: &Event,
        zone: &str,
    ) -> Result<(), String> {
        let npub = event
            .pubkey
            .to_bech32()
            .map_err(|e| format!("failed to encode pubkey as npub: {e}"))?;

        let fqdn = format!("_profile.{}.{}.", npub, zone);
        let json = compact_event_json(event);
        let segments = split_txt_segments(&json);

        backend
            .update_txt_multi(&fqdn, 3600, &segments)
            .await
            .map_err(|e| format!("DNS profile cache write failed for {fqdn}: {e}"))
    }

    pub async fn try_cache(backend: &dyn DnsConnector, event: &Event, zone: &str) {
        let kind = u64::from(event.kind.as_u16());
        let result = match kind {
            0 => Self::cache_profile(backend, event, zone).await,
            _ => Self::cache_event(backend, event, zone).await,
        };

        if let Err(e) = result {
            warn!(event_id = %event.id.to_hex(), kind, error = %e, "DNS event cache failed (non-fatal)");
        }
    }
}

fn split_txt_segments(content: &str) -> Vec<String> {
    content
        .as_bytes()
        .chunks(TXT_SEGMENT_MAX)
        .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
        .collect()
}

fn compact_event_json(event: &Event) -> String {
    let pubkey_hex = event.pubkey.to_hex();
    let sig_hex = hex::encode(event.sig.serialize());
    let id_hex = event.id.to_hex();

    let tags_array: Vec<serde_json::Value> = event
        .tags
        .iter()
        .map(|tag| {
            let values: Vec<serde_json::Value> = tag
                .as_slice()
                .iter()
                .map(|s| serde_json::Value::String(s.to_string()))
                .collect();
            serde_json::Value::Array(values)
        })
        .collect();

    let compact = serde_json::json!({
        "k": u64::from(event.kind.as_u16()),
        "p": pubkey_hex,
        "s": sig_hex,
        "i": id_hex,
        "t": event.created_at.as_secs(),
        "c": event.content,
        "tags": tags_array,
    });

    serde_json::to_string(&compact).unwrap_or_else(|_| String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_txt_segments_short_string() {
        let segments = split_txt_segments("hello");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0], "hello");
    }

    #[test]
    fn split_txt_segments_exact_boundary() {
        let input = "a".repeat(255);
        let segments = split_txt_segments(&input);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].len(), 255);
    }

    #[test]
    fn split_txt_segments_over_boundary() {
        let input = "a".repeat(256);
        let segments = split_txt_segments(&input);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].len(), 255);
        assert_eq!(segments[1].len(), 1);
    }

    #[test]
    fn split_txt_segments_large_content() {
        let input = "x".repeat(1000);
        let segments = split_txt_segments(&input);
        assert_eq!(segments.len(), 4);
        assert_eq!(segments[0].len(), 255);
        assert_eq!(segments[3].len(), 235);
    }

    #[test]
    fn split_txt_segments_empty() {
        let segments = split_txt_segments("");
        assert!(segments.is_empty());
    }
}
