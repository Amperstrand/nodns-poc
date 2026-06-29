//! Generic per-key circuit breaker.
//!
//! Prevents hammering a service that is known to be down or slow.
//! One breaker is maintained per key (mint URL, provider name, etc.).
//! The state machine is:
//!
//! ```text
//!   Closed --(3 consecutive failures)--> Open
//!   Open   --(5 min cooldown elapsed)--> HalfOpen (one probe allowed)
//!   HalfOpen --(success)--> Closed
//!   HalfOpen --(failure)--> Open (cooldown restarts)
//! ```
//!
//! This mirrors the circuit-breaker pattern used by the nomail service for its
//! mint calls. State is held in a [`std::sync::Mutex`] protected
//! [`HashMap`]; critical sections are tiny (a few field updates) so contention
//! is negligible. A poisoned lock is recovered via
//! [`MutexGuard::into_inner`] rather than panicking the bot.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use tracing::{debug, warn};

/// Number of consecutive failures required to open a closed circuit.
const CB_THRESHOLD: u32 = 3;

/// How long an open circuit must wait before a single half-open probe is allowed.
const CB_COOLDOWN: Duration = Duration::from_secs(300); // 5 minutes

/// Circuit-breaker state for a single mint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CircuitState {
    /// Operating normally; requests are allowed.
    Closed,
    /// Tripped; requests are short-circuited until the cooldown elapses.
    Open,
    /// Cooldown elapsed; a single probe request is allowed through.
    HalfOpen,
}

/// Per-mint bookkeeping stored under the breaker.
#[derive(Clone, Debug)]
struct CircuitEntry {
    failures: u32,
    opened_at: Option<Instant>,
    state: CircuitState,
}

impl Default for CircuitEntry {
    fn default() -> Self {
        Self {
            failures: 0,
            opened_at: None,
            state: CircuitState::Closed,
        }
    }
}

/// A circuit breaker tracking many mints by URL.
///
/// Methods take `&self` (the inner state is behind a `Mutex`) so a single
/// instance can be shared globally via [`LazyLock`].
#[derive(Debug, Default)]
pub struct CircuitBreaker {
    inner: Mutex<HashMap<String, CircuitEntry>>,
}

impl CircuitBreaker {
    /// Create an empty breaker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Acquire the lock, recovering from poison instead of panicking.
    ///
    /// Poison only occurs if a thread panicked while holding the lock; our
    /// critical sections are panic-free, so this is purely defensive. On
    /// poison we take the inner guard anyway so the bot keeps serving.
    fn lock(&self) -> MutexGuard<'_, HashMap<String, CircuitEntry>> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Returns `true` if requests to `mint_url` should be attempted.
    ///
    /// As a side effect this transitions an `Open` circuit whose cooldown has
    /// elapsed into `HalfOpen` (thereby allowing the single probe). This makes
    /// the probe atomic with the availability decision so concurrent callers
    /// don't all rush a recovering mint.
    pub fn is_available(&self, mint_url: &str) -> bool {
        let mut map = self.lock();
        let entry = map.entry(mint_url.to_string()).or_default();
        match entry.state {
            CircuitState::Closed => true,
            CircuitState::HalfOpen => true,
            CircuitState::Open => {
                let Some(opened_at) = entry.opened_at else {
                    // Defensive: should always be set when Open, but if not,
                    // treat as eligible for a probe.
                    entry.state = CircuitState::HalfOpen;
                    debug!(mint = %mint_url, "circuit -> half-open (no open time recorded)");
                    return true;
                };
                if opened_at.elapsed() >= CB_COOLDOWN {
                    entry.state = CircuitState::HalfOpen;
                    debug!(mint = %mint_url, "circuit -> half-open (cooldown elapsed, probe allowed)");
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Record a successful call; resets the circuit to `Closed`.
    pub fn record_success(&self, mint_url: &str) {
        let mut map = self.lock();
        if let Some(entry) = map.get_mut(mint_url) {
            if entry.state != CircuitState::Closed {
                debug!(
                    mint = %mint_url,
                    prev_state = ?entry.state,
                    "circuit -> closed (success)"
                );
            }
            entry.state = CircuitState::Closed;
            entry.failures = 0;
            entry.opened_at = None;
        }
    }

    /// Record a failed call. May trip the circuit (`Closed -> Open`) or
    /// re-open it (`HalfOpen -> Open`).
    pub fn record_failure(&self, mint_url: &str) {
        let mut map = self.lock();
        let entry = map.entry(mint_url.to_string()).or_default();
        entry.failures = entry.failures.saturating_add(1);
        match entry.state {
            CircuitState::HalfOpen => {
                // The probe failed — re-open and restart the cooldown.
                entry.state = CircuitState::Open;
                entry.opened_at = Some(Instant::now());
                warn!(mint = %mint_url, "circuit -> open (half-open probe failed)");
            }
            CircuitState::Closed => {
                if entry.failures >= CB_THRESHOLD {
                    entry.state = CircuitState::Open;
                    entry.opened_at = Some(Instant::now());
                    warn!(
                        mint = %mint_url,
                        failures = entry.failures,
                        "circuit -> open (threshold reached)"
                    );
                } else {
                    debug!(
                        mint = %mint_url,
                        failures = entry.failures,
                        threshold = CB_THRESHOLD,
                        "circuit staying closed (below threshold)"
                    );
                }
            }
            CircuitState::Open => {
                // Already open; keep the original `opened_at` so the cooldown
                // isn't silently extended by every failed retry attempt.
            }
        }
    }

    /// Current state for a mint (defaults to `Closed` if never seen).
    /// Primarily for tests and observability.
    #[allow(dead_code)]
    pub fn state(&self, mint_url: &str) -> CircuitState {
        let map = self.lock();
        map.get(mint_url)
            .map(|e| e.state.clone())
            .unwrap_or(CircuitState::Closed)
    }
}

/// Global singleton breaker used by the payment verifier.
pub static MINT_CIRCUITS: std::sync::LazyLock<CircuitBreaker> =
    std::sync::LazyLock::new(CircuitBreaker::new);

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> CircuitBreaker {
        CircuitBreaker::new()
    }

    #[test]
    fn unknown_mint_is_closed_and_available() {
        let cb = fresh();
        assert_eq!(cb.state("https://mint.example"), CircuitState::Closed);
        assert!(cb.is_available("https://mint.example"));
    }

    #[test]
    fn failures_below_threshold_stay_closed() {
        let cb = fresh();
        let m = "https://mint.example";
        cb.record_failure(m);
        cb.record_failure(m);
        assert_eq!(cb.state(m), CircuitState::Closed);
        assert!(cb.is_available(m));
    }

    #[test]
    fn threshold_failures_open_the_circuit() {
        let cb = fresh();
        let m = "https://mint.example";
        cb.record_failure(m);
        cb.record_failure(m);
        cb.record_failure(m);
        assert_eq!(cb.state(m), CircuitState::Open);
        assert!(!cb.is_available(m));
    }

    #[test]
    fn success_resets_to_closed() {
        let cb = fresh();
        let m = "https://mint.example";
        cb.record_failure(m);
        cb.record_failure(m);
        cb.record_success(m);
        assert_eq!(cb.state(m), CircuitState::Closed);
        // Should be fully reset (failures counter cleared).
        cb.record_failure(m);
        assert_eq!(cb.state(m), CircuitState::Closed);
    }

    #[test]
    fn open_circuit_becomes_half_open_after_cooldown() {
        let cb = fresh();
        let m = "https://mint.example";
        for _ in 0..CB_THRESHOLD {
            cb.record_failure(m);
        }
        assert_eq!(cb.state(m), CircuitState::Open);

        // Simulate cooldown elapsing by backdating opened_at.
        {
            let mut map = cb.inner.lock().unwrap();
            let entry = map.get_mut(m).unwrap();
            entry.opened_at = Some(Instant::now() - CB_COOLDOWN - Duration::from_secs(1));
        }

        // First availability check flips it to HalfOpen and allows the probe.
        assert!(cb.is_available(m));
        assert_eq!(cb.state(m), CircuitState::HalfOpen);
    }

    #[test]
    fn half_open_success_closes() {
        let cb = fresh();
        let m = "https://mint.example";
        for _ in 0..CB_THRESHOLD {
            cb.record_failure(m);
        }
        {
            let mut map = cb.inner.lock().unwrap();
            map.get_mut(m).unwrap().opened_at =
                Some(Instant::now() - CB_COOLDOWN - Duration::from_secs(1));
        }
        assert!(cb.is_available(m));
        assert_eq!(cb.state(m), CircuitState::HalfOpen);

        cb.record_success(m);
        assert_eq!(cb.state(m), CircuitState::Closed);
        assert!(cb.is_available(m));
    }

    #[test]
    fn half_open_failure_reopens() {
        let cb = fresh();
        let m = "https://mint.example";
        for _ in 0..CB_THRESHOLD {
            cb.record_failure(m);
        }
        {
            let mut map = cb.inner.lock().unwrap();
            map.get_mut(m).unwrap().opened_at =
                Some(Instant::now() - CB_COOLDOWN - Duration::from_secs(1));
        }
        assert!(cb.is_available(m));
        assert_eq!(cb.state(m), CircuitState::HalfOpen);

        cb.record_failure(m);
        assert_eq!(cb.state(m), CircuitState::Open);
        // Reopened -> a fresh cooldown applies (not immediately available).
        assert!(!cb.is_available(m));
    }

    #[test]
    fn open_circuit_stays_open_within_cooldown() {
        let cb = fresh();
        let m = "https://mint.example";
        for _ in 0..CB_THRESHOLD {
            cb.record_failure(m);
        }
        assert_eq!(cb.state(m), CircuitState::Open);
        // Immediately after tripping, the cooldown has NOT elapsed.
        assert!(!cb.is_available(m));
        assert_eq!(cb.state(m), CircuitState::Open);
    }

    #[test]
    fn mints_are_tracked_independently() {
        let cb = fresh();
        let a = "https://a.example";
        let b = "https://b.example";
        for _ in 0..CB_THRESHOLD {
            cb.record_failure(a);
        }
        assert_eq!(cb.state(a), CircuitState::Open);
        // B is untouched.
        assert_eq!(cb.state(b), CircuitState::Closed);
        assert!(cb.is_available(b));
    }

    #[test]
    fn extra_failures_on_open_do_not_extend_cooldown() {
        let cb = fresh();
        let m = "https://mint.example";
        for _ in 0..CB_THRESHOLD {
            cb.record_failure(m);
        }
        let original_open = {
            let map = cb.inner.lock().unwrap();
            map.get(m).unwrap().opened_at
        };
        // Hammer with more failures.
        cb.record_failure(m);
        cb.record_failure(m);
        let after = {
            let map = cb.inner.lock().unwrap();
            map.get(m).unwrap().opened_at
        };
        assert_eq!(
            original_open, after,
            "opened_at must not be refreshed while Open"
        );
    }
}
