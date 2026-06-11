//! Nostr relay subscriber — connects to multiple relays and forwards kind 11111 events.
//!
//! Ported 1:1 from `nodns-bot/internal/nostr/subscriber.go`.
//!
//! Uses nostr-sdk's built-in relay pool (one shared `Client`) instead of
//! managing one client per relay.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use nostr_sdk::{
    Client, Event, Filter, Kind, RelayMessage, RelayPoolNotification, Timestamp,
};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::config::NostrConfig;
use crate::store::Store;
use crate::types::KIND_DNS_RECORD;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum SubscriberError {
    #[error("relay connection failed: {0}")]
    Connection(String),

    #[error("subscription failed: {0}")]
    Subscription(String),
}

// ---------------------------------------------------------------------------
// Subscriber
// ---------------------------------------------------------------------------

/// Manages persistent connections to Nostr relays and forwards verified
/// kind 11111 (DNS record) events through an [`mpsc`] channel.
pub struct Subscriber {
    client: Client,
    relays: Vec<String>,
    store: Arc<Store>,
    handle: Mutex<Option<JoinHandle<()>>>,
    stopped: Arc<AtomicBool>,
}

impl Subscriber {
    /// Create a new subscriber from the Nostr section of the config.
    pub fn new(cfg: &NostrConfig, store: Arc<Store>) -> Self {
        Self {
            client: Client::default(),
            relays: cfg.relays.clone(),
            store,
            handle: Mutex::new(None),
            stopped: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Connect to all configured relays and return a channel of verified events.
    pub fn subscribe(&self) -> Result<mpsc::Receiver<Event>, SubscriberError> {
        let (tx, rx) = mpsc::channel(256);

        let last_seen = match self.store.get_last_seen() {
            Ok(ts) => ts,
            Err(e) => {
                tracing::warn!(error = %e, "failed to get last_seen, starting from 0");
                0
            }
        };

        let mut filter = Filter::new().kinds(vec![Kind::Custom(KIND_DNS_RECORD as u16)]);
        if last_seen > 0 {
            filter = filter.since(Timestamp::from_secs(last_seen as u64));
        }

        tracing::info!(
            relays = ?self.relays,
            since = last_seen,
            "subscribing to relays"
        );

        let client = self.client.clone();
        let relays = self.relays.clone();
        let stopped = self.stopped.clone();

        let handle = tokio::spawn(async move {
            // Add all relays to the shared client.
            for url in &relays {
                if let Err(e) = client.add_relay(url).await {
                    tracing::warn!(relay = %url, error = %e, "failed to add relay");
                }
            }

            client.connect().await;

            tracing::info!("connected to relays, subscribing");

            if let Err(e) = client.subscribe(filter, None).await {
                tracing::error!(error = %e, "subscription failed");
                return;
            }

            // Forward verified events through the channel.
            let _ = client
                .handle_notifications(|notification| {
                    let tx = tx.clone();
                    let stopped = stopped.clone();

                    async move {
                        if stopped.load(Ordering::Relaxed) {
                            return Ok(true);
                        }

                        match notification {
                            RelayPoolNotification::Event { event, .. } => {
                                tracing::debug!(
                                    event_id = %event.id,
                                    pubkey = %event.pubkey,
                                    "received event"
                                );

                                if let Err(e) = event.verify() {
                                    tracing::warn!(
                                        event_id = %event.id,
                                        error = %e,
                                        "signature verification failed, skipping event"
                                    );
                                    return Ok(false);
                                }

                                let event = (*event).clone();
                                if tx.send(event).await.is_err() {
                                    return Ok(true);
                                }
                            }

                            RelayPoolNotification::Message { message, .. } => match &message {
                                RelayMessage::EndOfStoredEvents(_) => {
                                    tracing::debug!("EOSE received");
                                }
                                RelayMessage::Closed { message, .. } => {
                                    tracing::warn!(reason = %message, "subscription closed by relay");
                                    return Ok(true);
                                }
                                _ => {}
                            },

                            RelayPoolNotification::Shutdown => {
                                return Ok(true);
                            }
                        }

                        Ok(false)
                    }
                })
                .await;
        });

        *self.handle.lock().unwrap_or_else(|e| {
            tracing::error!("Handle mutex poisoned, recovering: {}", e);
            e.into_inner()
        }) = Some(handle);

        Ok(rx)
    }

    /// Gracefully shut down all relay connections.
    pub fn stop(&self) {
        tracing::info!("stopping subscriber");
        self.stopped.store(true, Ordering::SeqCst);

        if let Some(handle) = self.handle.lock().unwrap_or_else(|e| {
            tracing::error!("Handle mutex poisoned, recovering: {}", e);
            e.into_inner()
        }).take()
        {
            handle.abort();
        }

        let client = self.client.clone();
        tokio::spawn(async move {
            client.disconnect().await;
        });
    }
}
