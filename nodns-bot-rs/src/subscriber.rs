//! Nostr relay subscriber — connects to multiple relays and forwards kind 11111 events.
//!
//! Ported 1:1 from `nodns-bot/internal/nostr/subscriber.go`.
//!
//! Each relay gets its own connection maintenance task with exponential backoff
//! reconnection (min 1 s → max 60 s, doubling on each failure).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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
    relays: Vec<String>,
    zone: String,
    store: Arc<Store>,
    reconnect_min: Duration,
    reconnect_max: Duration,
    stopped: Arc<AtomicBool>,
    tasks: Mutex<Vec<JoinHandle<()>>>,
}

impl Subscriber {
    /// Create a new subscriber from the Nostr section of the config.
    pub fn new(cfg: &NostrConfig, store: Arc<Store>) -> Self {
        let reconnect_min = cfg
            .reconnect_min
            .map(|d| d.0)
            .unwrap_or(Duration::from_secs(1));
        let reconnect_max = cfg
            .reconnect_max
            .map(|d| d.0)
            .unwrap_or(Duration::from_secs(60));

        Self {
            relays: cfg.relays.clone(),
            zone: cfg.zone.clone(),
            store,
            reconnect_min,
            reconnect_max,
            stopped: Arc::new(AtomicBool::new(false)),
            tasks: Mutex::new(Vec::new()),
        }
    }

    /// Connect to all configured relays and return a channel of verified events.
    ///
    /// Spawns one long-running task per relay.  Each task maintains its own
    /// connection with exponential backoff reconnection.
    pub fn subscribe(&self) -> Result<mpsc::Receiver<Event>, SubscriberError> {
        let (tx, rx) = mpsc::channel(256);

        // Determine `since` filter from last_seen timestamp.
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
            zone = %self.zone,
            "subscribing to relays"
        );

        // Spawn a connection-maintenance task per relay.
        for relay_url in &self.relays {
            let tx = tx.clone();
            let filter = filter.clone();
            let stopped = self.stopped.clone();
            let url = relay_url.clone();
            let reconnect_min = self.reconnect_min;
            let reconnect_max = self.reconnect_max;

            let handle = tokio::spawn(async move {
                maintain_connection(&url, filter, tx, stopped, reconnect_min, reconnect_max).await;
            });

            self.tasks.lock().unwrap_or_else(|e| {
                tracing::error!("Task list mutex poisoned, recovering: {}", e);
                e.into_inner()
            }).push(handle);
        }

        // Drop our sender so the receiver sees "closed" once every task finishes.
        drop(tx);

        Ok(rx)
    }

    /// Gracefully shut down all relay connections.
    ///
    /// Sets the stop flag and aborts all spawned tasks.
    pub fn stop(&self) {
        tracing::info!("stopping subscriber");
        self.stopped.store(true, Ordering::SeqCst);
        let tasks = std::mem::take(&mut *self.tasks.lock().unwrap_or_else(|e| {
            tracing::error!("Task list mutex poisoned, recovering: {}", e);
            e.into_inner()
        }));
        for handle in tasks {
            handle.abort();
        }
    }
}

// ---------------------------------------------------------------------------
// Connection maintenance (per-relay task)
// ---------------------------------------------------------------------------

/// Keep a persistent connection to a single relay with exponential backoff.
///
/// On each iteration it calls [`run_subscription`].  If the subscription
/// ends with an error the task sleeps for `backoff` and then retries,
/// doubling `backoff` each time up to `reconnect_max`.
async fn maintain_connection(
    relay_url: &str,
    filter: Filter,
    tx: mpsc::Sender<Event>,
    stopped: Arc<AtomicBool>,
    reconnect_min: Duration,
    reconnect_max: Duration,
) {
    let mut backoff = reconnect_min;

    loop {
        // Cooperative cancellation check.
        if stopped.load(Ordering::Relaxed) {
            return;
        }

        let result = run_subscription(relay_url, &filter, &tx, &stopped).await;

        // Check again after the subscription exits.
        if stopped.load(Ordering::Relaxed) {
            return;
        }

        if let Err(e) = result {
            tracing::warn!(
                relay = %relay_url,
                error = %e,
                backoff_secs = backoff.as_secs(),
                "relay subscription ended, reconnecting"
            );
        }

        // Sleep before retrying.  `stop()` aborts the task so the sleep is
        // interrupted even if no notifications arrive.
        tokio::time::sleep(backoff).await;

        if stopped.load(Ordering::Relaxed) {
            return;
        }

        if backoff < reconnect_max {
            backoff *= 2;
        }
    }
}

// ---------------------------------------------------------------------------
// Single subscription run
// ---------------------------------------------------------------------------

/// Connect to one relay, subscribe with `filter`, and forward verified events
/// until the connection drops, the relay closes the subscription, or the
/// caller requests shutdown.
async fn run_subscription(
    relay_url: &str,
    filter: &Filter,
    tx: &mpsc::Sender<Event>,
    stopped: &Arc<AtomicBool>,
) -> Result<(), SubscriberError> {
    // Create a read-only client (no signer needed for subscriptions).
    let client = Client::default();

    client
        .add_relay(relay_url)
        .await
        .map_err(|e| SubscriberError::Connection(format!("{relay_url}: {e}")))?;

    // `connect` is infallible — it spawns internal connection tasks.
    client.connect().await;

    tracing::info!(relay = %relay_url, "connected to relay");

    client
        .subscribe(filter.clone(), None)
        .await
        .map_err(|e| SubscriberError::Subscription(format!("{relay_url}: {e}")))?;

    // Variables captured by the notification handler closure.
    let stopped = stopped.clone();
    let tx = tx.clone();
    let relay_url_owned = relay_url.to_string();

    client
        .handle_notifications(|notification| {
            let stopped = stopped.clone();
            let tx = tx.clone();
            let relay_url = relay_url_owned.clone();

            async move {
                // Fast-path: exit if the subscriber has been stopped.
                if stopped.load(Ordering::Relaxed) {
                    return Ok(true);
                }

                match notification {
                    RelayPoolNotification::Event { event, .. } => {
                        tracing::debug!(
                            relay = %relay_url,
                            event_id = %event.id,
                            pubkey = %event.pubkey,
                            "received event"
                        );

                        // Verify event ID and signature.
                        if let Err(e) = event.verify() {
                            tracing::warn!(
                                event_id = %event.id,
                                error = %e,
                                "signature verification failed, skipping event"
                            );
                            return Ok(false);
                        }

                        // Forward the verified event through the channel.
                        let event = (*event).clone();
                        if tx.send(event).await.is_err() {
                            // Receiver dropped — caller is shutting down.
                            return Ok(true);
                        }
                    }

                    RelayPoolNotification::Message { message, .. } => match &message {
                        RelayMessage::EndOfStoredEvents(_) => {
                            tracing::debug!(relay = %relay_url, "EOSE received");
                        }
                        RelayMessage::Closed { message, .. } => {
                            tracing::warn!(
                                relay = %relay_url,
                                reason = %message,
                                "subscription closed by relay"
                            );
                            // Exit the notification loop so the outer
                            // `maintain_connection` can reconnect.
                            return Ok(true);
                        }
                        _ => {}
                    },

                    RelayPoolNotification::Shutdown => {
                        return Ok(true);
                    }
                }

                Ok(false) // continue handling notifications
            }
        })
        .await
        .map_err(|e| SubscriberError::Subscription(format!("{relay_url}: {e}")))?;

    Ok(())
}
