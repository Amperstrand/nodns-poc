//! Automatic DNS backend failover: primary (Knot DNS) → fallback (Cloudflare).
//!
//! Wraps two [`DnsConnector`] backends. The primary is preferred. After
//! [`FAILURE_THRESHOLD`] consecutive primary failures the circuit opens and
//! operations are routed to the fallback. Every [`RECOVERY_PROBE_INTERVAL`]
//! the primary is probed (half-open); a successful probe closes the circuit
//! and routes traffic back to the primary.
//!
//! This mirrors the [`crate::circuit_breaker`] pattern but is specialised for
//! routing to an alternate backend rather than rejecting calls.
//!
//! The connector is **opt-in**: it is only constructed at startup when a zone
//! is configured with `backend = "ddns"` *and* Cloudflare credentials are
//! also present. Zones without Cloudflare credentials keep the historical
//! single-backend behaviour unchanged.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::connector::DnsConnector;

/// Consecutive primary failures required before switching to the fallback.
const FAILURE_THRESHOLD: u32 = 3;

/// How long the primary circuit stays open before a half-open probe is allowed.
const RECOVERY_PROBE_INTERVAL: Duration = Duration::from_secs(5 * 60);

#[derive(Debug)]
struct FailoverState {
    consecutive_failures: u32,
    /// When `Some(t)` and `t` is in the future, the fallback is forced.
    /// When `Some(t)` and `t` has passed, a half-open probe to the primary is allowed.
    /// When `None`, the primary is healthy (closed).
    open_until: Option<Instant>,
}

impl FailoverState {
    const fn new() -> Self {
        Self {
            consecutive_failures: 0,
            open_until: None,
        }
    }

    /// True if a call should be attempted against the primary backend.
    /// (i.e. circuit is closed, or the half-open cooldown has elapsed.)
    fn primary_should_be_tried(&self) -> bool {
        match self.open_until {
            None => true,
            Some(deadline) => deadline <= Instant::now(),
        }
    }

    fn is_currently_failing_over(&self) -> bool {
        matches!(self.open_until, Some(deadline) if deadline > Instant::now())
    }

    /// Record a primary success: reset everything and close the circuit.
    fn record_primary_success(&mut self) {
        let was_failing_over = self.is_currently_failing_over();
        self.consecutive_failures = 0;
        self.open_until = None;
        if was_failing_over {
            info!("DNS failover: primary backend recovered — routing back to primary");
        }
    }

    /// Record a primary failure. Returns `true` if the fallback should now be
    /// used for this operation (i.e. the circuit just opened or remains open).
    fn record_primary_failure(&mut self) -> bool {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        let now = Instant::now();

        let already_open = self.is_currently_failing_over();

        if self.consecutive_failures >= FAILURE_THRESHOLD {
            // (Re)open the circuit. Always refresh the cooldown so a half-open
            // probe that fails restarts the timer.
            let just_opened = !already_open;
            self.open_until = Some(now + RECOVERY_PROBE_INTERVAL);
            if just_opened {
                warn!(
                    failures = self.consecutive_failures,
                    threshold = FAILURE_THRESHOLD,
                    cooldown_secs = RECOVERY_PROBE_INTERVAL.as_secs(),
                    "DNS failover: primary backend failing — switching to fallback"
                );
            }
            true
        } else {
            warn!(
                failures = self.consecutive_failures,
                threshold = FAILURE_THRESHOLD,
                "DNS failover: primary backend error (below threshold, not switching yet)"
            );
            false
        }
    }
}

/// A `DnsConnector` that tries `primary` first and falls back to `fallback`
/// after the primary's circuit opens.
pub struct FailoverConnector {
    primary: Arc<dyn DnsConnector>,
    fallback: Arc<dyn DnsConnector>,
    state: Arc<Mutex<FailoverState>>,
}

impl FailoverConnector {
    /// Build a failover wrapper. `primary` is preferred (typically Knot DDNS);
    /// `fallback` is used when the primary circuit is open (typically Cloudflare).
    pub fn new(primary: Arc<dyn DnsConnector>, fallback: Arc<dyn DnsConnector>) -> Self {
        Self {
            primary,
            fallback,
            state: Arc::new(Mutex::new(FailoverState::new())),
        }
    }
}

/// Inline the failover routing for one connector method.
///
/// A declarative macro is used (rather than a generic helper) because Rust's
/// HRTB cannot express a closure that returns a future borrowing *both* the
/// connector and the method arguments. Inlining direct method calls avoids the
/// lifetime conflict entirely while keeping the per-method bodies identical.
macro_rules! route_op {
    ($self:expr, $label:expr, $method:ident($($arg:expr),*)) => {{
        let try_primary = {
            let state = $self.state.lock().await;
            state.primary_should_be_tried()
        };
        if try_primary {
            match $self.primary.$method($($arg),*).await {
                Ok(()) => {
                    $self.state.lock().await.record_primary_success();
                    Ok(())
                }
                Err(primary_err) => {
                    let use_fallback = {
                        let mut state = $self.state.lock().await;
                        state.record_primary_failure()
                    };
                    if use_fallback {
                        warn!(
                            op = $label,
                            error = %primary_err,
                            "DNS failover: primary failed, attempting fallback backend"
                        );
                        $self.fallback.$method($($arg),*).await
                    } else {
                        Err(primary_err)
                    }
                }
            }
        } else {
            $self.fallback.$method($($arg),*).await
        }
    }};
}

#[async_trait::async_trait]
impl DnsConnector for FailoverConnector {
    async fn update_record(
        &self,
        fqdn: &str,
        ttl: u32,
        record_type: u16,
        rdata: &str,
    ) -> Result<()> {
        route_op!(
            self,
            "update_record",
            update_record(fqdn, ttl, record_type, rdata)
        )
    }

    async fn update_txt_multi(&self, fqdn: &str, ttl: u32, segments: &[String]) -> Result<()> {
        route_op!(
            self,
            "update_txt_multi",
            update_txt_multi(fqdn, ttl, segments)
        )
    }

    async fn delete_record(&self, fqdn: &str, record_type: u16) -> Result<()> {
        route_op!(self, "delete_record", delete_record(fqdn, record_type))
    }

    async fn append_record(
        &self,
        fqdn: &str,
        ttl: u32,
        record_type: u16,
        rdata: &str,
    ) -> Result<()> {
        route_op!(
            self,
            "append_record",
            append_record(fqdn, ttl, record_type, rdata)
        )
    }

    async fn test_connection(&self) -> Result<()> {
        // Connectivity check targets the primary backend (the preferred path).
        // test_connection is informational; a failure here is logged by the caller
        // and does not itself trip the failover circuit.
        self.primary.test_connection().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A programmable connector that fails the first N calls then succeeds.
    struct MockConnector {
        label: &'static str,
        fail_first_n: AtomicU32,
        call_count: AtomicU32,
    }

    impl MockConnector {
        fn failing(label: &'static str, fail_first_n: u32) -> Arc<Self> {
            Arc::new(Self {
                label,
                fail_first_n: AtomicU32::new(fail_first_n),
                call_count: AtomicU32::new(0),
            })
        }

        fn always_ok(label: &'static str) -> Arc<Self> {
            Self::failing(label, 0)
        }

        fn calls(&self) -> u32 {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl DnsConnector for MockConnector {
        async fn update_record(
            &self,
            _fqdn: &str,
            _ttl: u32,
            _record_type: u16,
            _rdata: &str,
        ) -> Result<()> {
            let n = self.call_count.fetch_add(1, Ordering::SeqCst) + 1;
            let limit = self.fail_first_n.load(Ordering::SeqCst);
            if n <= limit {
                Err(anyhow::anyhow!("{} mock failure #{n}", self.label))
            } else {
                Ok(())
            }
        }

        async fn update_txt_multi(
            &self,
            _fqdn: &str,
            _ttl: u32,
            _segments: &[String],
        ) -> Result<()> {
            Ok(())
        }

        async fn delete_record(&self, _fqdn: &str, _record_type: u16) -> Result<()> {
            Ok(())
        }

        async fn append_record(
            &self,
            _fqdn: &str,
            _ttl: u32,
            _record_type: u16,
            _rdata: &str,
        ) -> Result<()> {
            Ok(())
        }

        async fn test_connection(&self) -> Result<()> {
            Ok(())
        }
    }

    fn connector() -> (FailoverConnector, Arc<MockConnector>, Arc<MockConnector>) {
        let primary = MockConnector::failing("primary", 0);
        let fallback = MockConnector::always_ok("fallback");
        let fc = FailoverConnector::new(primary.clone(), fallback.clone());
        (fc, primary, fallback)
    }

    #[tokio::test]
    async fn primary_success_uses_primary_only() {
        let (fc, primary, fallback) = connector();
        fc.update_record("foo.example.", 60, 1, "1.2.3.4")
            .await
            .unwrap();
        assert_eq!(primary.calls(), 1);
        assert_eq!(fallback.calls(), 0, "fallback should not be touched");
    }

    #[tokio::test]
    async fn below_threshold_failures_do_not_switch() {
        // Primary fails twice: below the threshold of 3.
        let primary = MockConnector::failing("primary", 5);
        let fallback = MockConnector::always_ok("fallback");
        let fc = FailoverConnector::new(primary.clone(), fallback.clone());

        let r1 = fc.update_record("a.example.", 60, 1, "1.1.1.1").await;
        let r2 = fc.update_record("b.example.", 60, 1, "1.1.1.2").await;
        assert!(r1.is_err());
        assert!(r2.is_err());
        // Both calls hit the primary, none hit the fallback.
        assert_eq!(primary.calls(), 2);
        assert_eq!(fallback.calls(), 0);
    }

    #[tokio::test]
    async fn third_failure_triggers_failover_to_fallback() {
        // Primary fails 4 times: crosses the threshold on call #3.
        let primary = MockConnector::failing("primary", 4);
        let fallback = MockConnector::always_ok("fallback");
        let fc = FailoverConnector::new(primary.clone(), fallback.clone());

        // Calls 1 & 2: primary fails, below threshold — error returned.
        assert!(fc
            .update_record("a.example.", 60, 1, "1.1.1.1")
            .await
            .is_err());
        assert!(fc
            .update_record("b.example.", 60, 1, "1.1.1.2")
            .await
            .is_err());

        // Call 3: primary fails (3rd consecutive), circuit opens, fallback used.
        let r3 = fc.update_record("c.example.", 60, 1, "1.1.1.3").await;
        assert!(r3.is_ok(), "third failure should failover to fallback");

        // Call 4: circuit open within cooldown — fallback used directly, primary untouched.
        let r4 = fc.update_record("d.example.", 60, 1, "1.1.1.4").await;
        assert!(r4.is_ok());

        assert_eq!(primary.calls(), 3, "primary tried exactly 3 times");
        assert_eq!(fallback.calls(), 2, "fallback used for calls 3 and 4");
    }

    #[tokio::test]
    async fn fallback_is_used_while_circuit_open() {
        let primary = MockConnector::failing("primary", 100);
        let fallback = MockConnector::always_ok("fallback");
        let fc = FailoverConnector::new(primary.clone(), fallback.clone());

        // Trip the circuit.
        for i in 1..=3 {
            let _ = fc
                .update_record(&format!("p{i}.example."), 60, 1, "1.1.1.1")
                .await;
        }
        // Now open — subsequent calls go straight to fallback.
        for i in 0..5 {
            fc.update_record(&format!("q{i}.example."), 60, 1, "1.1.1.1")
                .await
                .unwrap();
        }
        // Primary was attempted exactly 3 times (the failures that opened the circuit).
        assert_eq!(primary.calls(), 3);
        // Fallback handled call 3 + the 5 follow-ups = 6.
        assert_eq!(fallback.calls(), 6);
    }

    #[tokio::test]
    async fn half_open_probe_success_closes_circuit() {
        let primary = MockConnector::always_ok("primary");
        let fallback = MockConnector::always_ok("fallback");
        let fc = FailoverConnector::new(primary.clone(), fallback.clone());
        {
            let mut state = fc.state.lock().await;
            state.open_until = Some(Instant::now() - Duration::from_secs(1));
            state.consecutive_failures = FAILURE_THRESHOLD;
        }
        fc.update_record("probe.example.", 60, 1, "1.1.1.9")
            .await
            .unwrap();
        let state = fc.state.lock().await;
        assert!(state.open_until.is_none());
        assert_eq!(state.consecutive_failures, 0);
        assert!(!state.is_currently_failing_over());
        assert_eq!(primary.calls(), 1);
        assert_eq!(
            fallback.calls(),
            0,
            "fallback should not be used after probe success"
        );
    }

    #[tokio::test]
    async fn half_open_probe_failure_reopens_circuit() {
        let primary = MockConnector::failing("primary", 100);
        let fallback = MockConnector::always_ok("fallback");
        let fc = FailoverConnector::new(primary.clone(), fallback.clone());
        {
            let mut state = fc.state.lock().await;
            state.open_until = Some(Instant::now() - Duration::from_secs(1));
            state.consecutive_failures = FAILURE_THRESHOLD;
        }
        // Probe fails (primary still broken) → should route to fallback AND reopen.
        fc.update_record("probe.example.", 60, 1, "1.1.1.9")
            .await
            .unwrap();
        let state = fc.state.lock().await;
        assert!(
            state.is_currently_failing_over(),
            "circuit should be open again after a failed probe"
        );
    }

    #[test]
    fn failover_state_starts_closed() {
        let s = FailoverState::new();
        assert!(s.primary_should_be_tried());
        assert!(!s.is_currently_failing_over());
    }

    #[test]
    fn record_failure_below_threshold_returns_false() {
        let mut s = FailoverState::new();
        assert!(!s.record_primary_failure());
        assert!(!s.record_primary_failure());
        assert!(s.open_until.is_none(), "should not open below threshold");
    }

    #[test]
    fn record_failure_at_threshold_opens_circuit() {
        let mut s = FailoverState::new();
        s.record_primary_failure();
        s.record_primary_failure();
        let opened = s.record_primary_failure();
        assert!(opened);
        assert!(s.is_currently_failing_over());
    }

    #[test]
    fn record_success_resets_state() {
        let mut s = FailoverState::new();
        for _ in 0..FAILURE_THRESHOLD {
            s.record_primary_failure();
        }
        assert!(s.is_currently_failing_over());
        s.record_primary_success();
        assert!(!s.is_currently_failing_over());
        assert_eq!(s.consecutive_failures, 0);
    }
}
