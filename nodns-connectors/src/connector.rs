use anyhow::Result;

#[async_trait::async_trait]
pub trait DnsConnector: Send + Sync {
    async fn update_record(
        &self,
        fqdn: &str,
        ttl: u32,
        record_type: u16,
        rdata: &str,
    ) -> Result<()>;

    async fn update_txt_multi(&self, fqdn: &str, ttl: u32, segments: &[String]) -> Result<()>;

    async fn delete_record(&self, fqdn: &str, record_type: u16) -> Result<()>;

    async fn append_record(
        &self,
        fqdn: &str,
        ttl: u32,
        record_type: u16,
        rdata: &str,
    ) -> Result<()>;

    async fn test_connection(&self) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum MockCall {
        UpdateRecord {
            fqdn: String,
            ttl: u32,
            record_type: u16,
            rdata: String,
        },
        UpdateTxtMulti {
            fqdn: String,
            ttl: u32,
            segments: Vec<String>,
        },
        DeleteRecord {
            fqdn: String,
            record_type: u16,
        },
        AppendRecord {
            fqdn: String,
            ttl: u32,
            record_type: u16,
            rdata: String,
        },
        TestConnection,
    }

    type CallLog = Arc<Mutex<Vec<MockCall>>>;

    struct MockConnector {
        calls: CallLog,
        failure: Option<&'static str>,
    }

    impl MockConnector {
        fn new() -> (Self, CallLog) {
            Self::build(None)
        }

        fn failing(msg: &'static str) -> (Self, CallLog) {
            Self::build(Some(msg))
        }

        fn build(failure: Option<&'static str>) -> (Self, CallLog) {
            let calls: CallLog = Arc::new(Mutex::new(Vec::new()));
            let connector = Self {
                calls: calls.clone(),
                failure,
            };
            (connector, calls)
        }

        fn maybe_fail(&self) -> Result<()> {
            match self.failure {
                Some(msg) => Err(anyhow::anyhow!("{msg}")),
                None => Ok(()),
            }
        }

        fn record(&self, call: MockCall) {
            self.calls.lock().unwrap().push(call);
        }
    }

    #[async_trait::async_trait]
    impl DnsConnector for MockConnector {
        async fn update_record(
            &self,
            fqdn: &str,
            ttl: u32,
            record_type: u16,
            rdata: &str,
        ) -> Result<()> {
            self.record(MockCall::UpdateRecord {
                fqdn: fqdn.to_string(),
                ttl,
                record_type,
                rdata: rdata.to_string(),
            });
            self.maybe_fail()
        }

        async fn update_txt_multi(&self, fqdn: &str, ttl: u32, segments: &[String]) -> Result<()> {
            self.record(MockCall::UpdateTxtMulti {
                fqdn: fqdn.to_string(),
                ttl,
                segments: segments.to_vec(),
            });
            self.maybe_fail()
        }

        async fn delete_record(&self, fqdn: &str, record_type: u16) -> Result<()> {
            self.record(MockCall::DeleteRecord {
                fqdn: fqdn.to_string(),
                record_type,
            });
            self.maybe_fail()
        }

        async fn append_record(
            &self,
            fqdn: &str,
            ttl: u32,
            record_type: u16,
            rdata: &str,
        ) -> Result<()> {
            self.record(MockCall::AppendRecord {
                fqdn: fqdn.to_string(),
                ttl,
                record_type,
                rdata: rdata.to_string(),
            });
            self.maybe_fail()
        }

        async fn test_connection(&self) -> Result<()> {
            self.record(MockCall::TestConnection);
            self.maybe_fail()
        }
    }

    fn snapshot(log: &CallLog) -> Vec<MockCall> {
        log.lock().unwrap().clone()
    }

    #[tokio::test]
    async fn arc_dyn_dispatches_update_record() {
        let (mock, log) = MockConnector::new();
        let connector: Arc<dyn DnsConnector> = Arc::new(mock);

        connector
            .update_record("foo.example.", 300, 1, "127.0.0.1")
            .await
            .expect("update_record should succeed on Arc<dyn DnsConnector>");

        let calls = snapshot(&log);
        assert_eq!(calls.len(), 1, "exactly one call should be recorded");
        assert_eq!(
            calls[0],
            MockCall::UpdateRecord {
                fqdn: "foo.example.".into(),
                ttl: 300,
                record_type: 1,
                rdata: "127.0.0.1".into(),
            }
        );
    }

    #[tokio::test]
    async fn arc_dyn_propagates_errors() {
        let (mock, log) = MockConnector::failing("boom");
        let connector: Arc<dyn DnsConnector> = Arc::new(mock);

        let err = connector
            .update_record("err.example.", 60, 1, "1.1.1.1")
            .await
            .expect_err("failure must propagate through the Arc<dyn DnsConnector> boundary");

        assert!(
            err.to_string().contains("boom"),
            "propagated error should carry the mock's message, got: {err}"
        );

        assert_eq!(snapshot(&log).len(), 1);
    }

    #[tokio::test]
    async fn arc_dyn_dispatches_every_method() {
        let (mock, log) = MockConnector::new();
        let connector: Arc<dyn DnsConnector> = Arc::new(mock);

        connector
            .update_record("a.example.", 60, 1, "10.0.0.1")
            .await
            .unwrap();
        connector
            .update_txt_multi("t.example.", 60, &["seg-".into(), "one".into()])
            .await
            .unwrap();
        connector.delete_record("a.example.", 1).await.unwrap();
        connector
            .append_record("b.example.", 60, 16, "hello")
            .await
            .unwrap();
        connector.test_connection().await.unwrap();

        let calls = snapshot(&log);
        assert_eq!(calls.len(), 5, "all five trait methods should dispatch");

        assert!(matches!(
            &calls[0],
            MockCall::UpdateRecord { fqdn, ttl: 60, record_type: 1, rdata }
                if fqdn == "a.example." && rdata == "10.0.0.1"
        ));
        assert!(matches!(
            &calls[1],
            MockCall::UpdateTxtMulti { fqdn, ttl: 60, segments }
                if fqdn == "t.example." && segments == &["seg-".to_string(), "one".to_string()]
        ));
        assert!(matches!(
            &calls[2],
            MockCall::DeleteRecord { fqdn, record_type: 1 } if fqdn == "a.example."
        ));
        assert!(matches!(
            &calls[3],
            MockCall::AppendRecord { fqdn, ttl: 60, record_type: 16, rdata }
                if fqdn == "b.example." && rdata == "hello"
        ));
        assert_eq!(calls[4], MockCall::TestConnection);
    }

    #[tokio::test]
    async fn box_dyn_dispatches_update_record() {
        let (mock, log) = MockConnector::new();
        let connector: Box<dyn DnsConnector> = Box::new(mock);

        connector
            .update_record("box.example.", 300, 1, "127.0.0.1")
            .await
            .expect("update_record should succeed on Box<dyn DnsConnector>");

        let calls = snapshot(&log);
        assert_eq!(calls.len(), 1, "Box<dyn DnsConnector> must record the call");
        assert_eq!(
            calls[0],
            MockCall::UpdateRecord {
                fqdn: "box.example.".into(),
                ttl: 300,
                record_type: 1,
                rdata: "127.0.0.1".into(),
            }
        );
    }

    #[tokio::test]
    async fn box_dyn_propagates_errors() {
        let (mock, _log) = MockConnector::failing("box-boom");
        let connector: Box<dyn DnsConnector> = Box::new(mock);

        let err = connector
            .delete_record("d.example.", 1)
            .await
            .expect_err("failure must propagate through Box<dyn DnsConnector>");

        assert!(
            err.to_string().contains("box-boom"),
            "propagated error should carry the mock's message, got: {err}"
        );
    }

    #[tokio::test]
    async fn arc_dyn_can_feed_failover_connector() {
        let (primary_mock, primary_log) = MockConnector::new();
        let (fallback_mock, fallback_log) = MockConnector::new();
        let primary: Arc<dyn DnsConnector> = Arc::new(primary_mock);
        let fallback: Arc<dyn DnsConnector> = Arc::new(fallback_mock);

        let failover = crate::failover::FailoverConnector::new(primary, fallback);
        failover
            .update_record("fo.example.", 60, 1, "1.2.3.4")
            .await
            .expect("healthy primary should succeed");

        assert_eq!(
            snapshot(&primary_log).len(),
            1,
            "primary should receive the call"
        );
        assert_eq!(
            snapshot(&fallback_log).len(),
            0,
            "fallback should be untouched while primary is healthy"
        );
    }
}
