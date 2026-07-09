use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use domain::{
    error::JobError,
    ports::{job_queue::JobQueue, spool::Spool},
};

use super::circuit::{CircuitBreaker, CircuitConfig};

/// One spooled dispatch: enough to replay the enqueue on the workers side.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpooledJob {
    pub queue: String,
    pub payload: Vec<u8>,
    /// W3C traceparent captured at emit time, so the replayed job keeps the
    /// original trace instead of the drainer's.
    #[serde(default)]
    pub traceparent: Option<String>,
}

/// [`JobQueue`] chain with per-hop circuit breakers: workers gRPC first, the
/// direct apalis push second, the durable job spool last. Both first hops land
/// in the same apalis queue, so a fallback changes transport only. An open
/// breaker skips its hop instantly; errors surface only when every hop failed.
pub struct DispatchQueue {
    primary: Arc<dyn JobQueue>,
    fallback: Arc<dyn JobQueue>,
    spool: Arc<dyn Spool>,
    primary_breaker: Arc<CircuitBreaker>,
    fallback_breaker: Arc<CircuitBreaker>,
    /// Captures the caller's traceparent for spooled entries; injected so this
    /// stays IO-free and deterministic in tests.
    traceparent: fn() -> Option<String>,
}

impl DispatchQueue {
    /// `primary_breaker` is shared (the health registry's workers-gRPC breaker)
    /// so probe results and real call results drive the same state.
    #[must_use]
    pub fn new(
        primary: Arc<dyn JobQueue>,
        fallback: Arc<dyn JobQueue>,
        spool: Arc<dyn Spool>,
        primary_breaker: Arc<CircuitBreaker>,
        traceparent: fn() -> Option<String>,
    ) -> Self {
        Self {
            primary,
            fallback,
            spool,
            primary_breaker,
            fallback_breaker: Arc::new(CircuitBreaker::new(CircuitConfig::default())),
            traceparent,
        }
    }

    async fn try_hop(
        queue_impl: &dyn JobQueue,
        breaker: &CircuitBreaker,
        hop: &'static str,
        queue: &str,
        payload: &[u8],
    ) -> bool {
        if !breaker.acquire() {
            return false;
        }
        match queue_impl.enqueue(queue, payload).await {
            Ok(()) => {
                breaker.record_success();
                true
            }
            Err(e) => {
                breaker.record_failure();
                tracing::warn!(hop, queue, error = %e, "job dispatch hop failed");
                false
            }
        }
    }
}

#[async_trait]
impl JobQueue for DispatchQueue {
    async fn enqueue(&self, queue: &str, payload: &[u8]) -> Result<(), JobError> {
        if Self::try_hop(
            self.primary.as_ref(),
            &self.primary_breaker,
            "grpc",
            queue,
            payload,
        )
        .await
        {
            return Ok(());
        }
        if Self::try_hop(
            self.fallback.as_ref(),
            &self.fallback_breaker,
            "apalis",
            queue,
            payload,
        )
        .await
        {
            return Ok(());
        }
        let entry = serde_json::to_vec(&SpooledJob {
            queue: queue.to_owned(),
            payload: payload.to_vec(),
            traceparent: (self.traceparent)(),
        })
        .expect("SpooledJob only contains serde-derivable types");
        self.spool.push(&entry).await.map_err(|e| {
            JobError::Backend(format!("dispatch chain exhausted, spool failed: {e}"))
        })?;
        tracing::warn!(queue, "job spooled: grpc and apalis hops unavailable");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    use domain::{
        error::SpoolError,
        ports::spool::{SpoolEntry, SpoolId},
    };

    use super::*;

    struct FakeQueue {
        fail: bool,
        calls: AtomicUsize,
    }

    #[async_trait]
    impl JobQueue for FakeQueue {
        async fn enqueue(&self, _queue: &str, _payload: &[u8]) -> Result<(), JobError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                Err(JobError::Backend("down".into()))
            } else {
                Ok(())
            }
        }
    }

    struct FakeSpool {
        entries: Mutex<Vec<Vec<u8>>>,
    }

    #[async_trait]
    impl Spool for FakeSpool {
        async fn push(&self, batch: &[u8]) -> Result<(), SpoolError> {
            self.entries.lock().unwrap().push(batch.to_vec());
            Ok(())
        }

        async fn drain(&self, _max: usize) -> Result<Vec<SpoolEntry>, SpoolError> {
            Ok(Vec::new())
        }

        async fn ack(&self, _ids: &[SpoolId]) -> Result<(), SpoolError> {
            Ok(())
        }
    }

    fn queue(fail: bool) -> Arc<FakeQueue> {
        Arc::new(FakeQueue {
            fail,
            calls: AtomicUsize::new(0),
        })
    }

    fn dispatch(
        primary: Arc<FakeQueue>,
        fallback: Arc<FakeQueue>,
        spool: Arc<FakeSpool>,
    ) -> DispatchQueue {
        DispatchQueue::new(
            primary,
            fallback,
            spool,
            Arc::new(CircuitBreaker::new(CircuitConfig::default())),
            || None,
        )
    }

    #[tokio::test]
    async fn primary_success_skips_fallback() {
        let (primary, fallback) = (queue(false), queue(true));
        let spool = Arc::new(FakeSpool {
            entries: Mutex::new(Vec::new()),
        });
        let dq = dispatch(primary.clone(), fallback.clone(), spool);
        dq.enqueue("notifications", b"x").await.unwrap();
        assert_eq!(primary.calls.load(Ordering::SeqCst), 1);
        assert_eq!(fallback.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn primary_failure_falls_back() {
        let (primary, fallback) = (queue(true), queue(false));
        let spool = Arc::new(FakeSpool {
            entries: Mutex::new(Vec::new()),
        });
        let dq = dispatch(primary.clone(), fallback.clone(), spool);
        dq.enqueue("audit", b"x").await.unwrap();
        assert_eq!(fallback.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn open_breaker_short_circuits_primary() {
        let (primary, fallback) = (queue(true), queue(false));
        let spool = Arc::new(FakeSpool {
            entries: Mutex::new(Vec::new()),
        });
        let dq = dispatch(primary.clone(), fallback.clone(), spool);
        // Default threshold is 3 consecutive failures; the 4th emit must not
        // touch the primary at all.
        for _ in 0..3 {
            dq.enqueue("audit", b"x").await.unwrap();
        }
        assert_eq!(primary.calls.load(Ordering::SeqCst), 3);
        dq.enqueue("audit", b"x").await.unwrap();
        assert_eq!(primary.calls.load(Ordering::SeqCst), 3);
        assert_eq!(fallback.calls.load(Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn both_hops_down_spools_the_job() {
        let (primary, fallback) = (queue(true), queue(true));
        let spool = Arc::new(FakeSpool {
            entries: Mutex::new(Vec::new()),
        });
        let dq = dispatch(primary, fallback, spool.clone());
        dq.enqueue("audit", b"payload").await.unwrap();
        let entries = spool.entries.lock().unwrap();
        assert_eq!(entries.len(), 1);
        let job: SpooledJob = serde_json::from_slice(&entries[0]).unwrap();
        assert_eq!(job.queue, "audit");
        assert_eq!(job.payload, b"payload");
    }
}
