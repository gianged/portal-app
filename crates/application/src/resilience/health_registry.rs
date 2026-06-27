use std::{collections::HashMap, sync::Arc, time::Duration};

use domain::{
    health::{BackendId, HealthStatus},
    ports::health::HealthCheck,
};
use tokio::time::MissedTickBehavior;

use super::circuit::{CircuitBreaker, CircuitConfig};

/// Owns one [`CircuitBreaker`] per backend and the background prober that drives
/// them. Call sites and worker gates share the same breakers via [`Self::breaker`],
/// so a probe-detected outage immediately fail-fasts the request path too.
pub struct HealthRegistry {
    breakers: HashMap<BackendId, Arc<CircuitBreaker>>,
}

impl HealthRegistry {
    /// One default-configured breaker per listed backend.
    #[must_use]
    pub fn new(backends: &[BackendId]) -> Self {
        let breakers = backends
            .iter()
            .map(|&id| (id, Arc::new(CircuitBreaker::new(CircuitConfig::default()))))
            .collect();
        Self { breakers }
    }

    /// The breaker for `backend`, shared with call sites and worker gates.
    #[must_use]
    pub fn breaker(&self, backend: BackendId) -> Option<Arc<CircuitBreaker>> {
        self.breakers.get(&backend).cloned()
    }

    /// Snapshot of every tracked backend's status, for the readiness endpoint.
    #[must_use]
    pub fn status(&self) -> Vec<(BackendId, HealthStatus)> {
        BackendId::ALL
            .iter()
            .filter_map(|id| self.breakers.get(id).map(|b| (*id, b.status())))
            .collect()
    }

    /// Periodically pings each backend and feeds the result into its breaker:
    /// a failure trips it toward open, a success against a cooled-down breaker
    /// flips it half-open then closed (recovery detection). While a breaker is
    /// still cooling down, `acquire` is false and the ping is skipped, so a
    /// downed backend is not hammered by the prober either. Spawned under
    /// `supervise`, so a panic restarts it.
    pub async fn run_prober(
        self: Arc<Self>,
        checks: Vec<Arc<dyn HealthCheck>>,
        interval: Duration,
    ) {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            for check in &checks {
                let Some(breaker) = self.breaker(check.backend()) else {
                    continue;
                };
                if !breaker.acquire() {
                    continue;
                }
                match check.ping().await {
                    Ok(()) => breaker.record_success(),
                    Err(e) => {
                        tracing::warn!(
                            target: "health",
                            backend = check.backend().as_str(),
                            error = %e,
                            "health probe failed"
                        );
                        breaker.record_failure();
                    }
                }
            }
        }
    }
}
