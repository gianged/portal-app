use std::{
    sync::{Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

use domain::{
    error::{RenderError, RepositoryError, StorageError},
    health::HealthStatus,
};

use crate::error::{Error, Result};
use super::backoff::Backoff;

/// Tunables for one breaker.
#[derive(Debug, Clone, Copy)]
pub struct CircuitConfig {
    /// Consecutive failures (while closed) that trip the breaker open.
    pub failure_threshold: u32,
    /// Consecutive half-open successes that close it again.
    pub success_threshold: u32,
    /// Concurrent probe calls admitted while half-open.
    pub half_open_max: u32,
    /// Floor for the open cooldown; grows toward `max_cooldown` on re-open.
    pub base_cooldown: Duration,
    /// Ceiling for the open cooldown.
    pub max_cooldown: Duration,
}

impl Default for CircuitConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 3,
            success_threshold: 2,
            half_open_max: 2,
            base_cooldown: Duration::from_secs(1),
            max_cooldown: Duration::from_secs(30),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum State {
    Closed { failures: u32 },
    Open { until: Instant },
    HalfOpen { in_flight: u32, successes: u32 },
}

struct Inner {
    state: State,
    cooldown: Backoff,
}

/// Three-state circuit breaker: `Closed -> Open -> HalfOpen -> Closed`. While
/// `Open`, [`CircuitBreaker::acquire`] fail-fasts (the "stop hammering"); after
/// the backoff-grown cooldown it admits a small half-open probe quota, and
/// `success_threshold` consecutive probe successes close it again. Any failure
/// re-opens with a longer cooldown.
///
/// No IO of its own. Shared via `Arc` so the registry, prober, and readiness
/// endpoint all observe the same state.
pub struct CircuitBreaker {
    cfg: CircuitConfig,
    inner: Mutex<Inner>,
}

impl CircuitBreaker {
    #[must_use]
    pub fn new(cfg: CircuitConfig) -> Self {
        Self {
            inner: Mutex::new(Inner {
                state: State::Closed { failures: 0 },
                cooldown: Backoff::new(cfg.base_cooldown, cfg.max_cooldown),
            }),
            cfg,
        }
    }

    /// Try to admit a call. `true` when closed, or when half-open with a free
    /// probe slot, or when an open cooldown has elapsed (which transitions the
    /// breaker to half-open and consumes the first probe slot). `false` while
    /// open and still cooling down.
    pub fn acquire(&self) -> bool {
        let mut inner = self.lock();
        match inner.state {
            State::Closed { .. } => true,
            State::Open { until } => {
                if Instant::now() >= until {
                    inner.state = State::HalfOpen {
                        in_flight: 1,
                        successes: 0,
                    };
                    true
                } else {
                    false
                }
            }
            State::HalfOpen {
                in_flight,
                successes,
            } => {
                if in_flight < self.cfg.half_open_max {
                    inner.state = State::HalfOpen {
                        in_flight: in_flight + 1,
                        successes,
                    };
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Record a successful call. Resets the failure count while closed; advances
    /// the half-open success count and closes once the threshold is reached.
    pub fn record_success(&self) {
        let mut inner = self.lock();
        match inner.state {
            State::Closed { .. } => inner.state = State::Closed { failures: 0 },
            State::HalfOpen {
                in_flight,
                successes,
            } => {
                let successes = successes + 1;
                if successes >= self.cfg.success_threshold {
                    inner.cooldown.reset();
                    inner.state = State::Closed { failures: 0 };
                } else {
                    inner.state = State::HalfOpen {
                        in_flight: in_flight.saturating_sub(1),
                        successes,
                    };
                }
            }
            // A success against an open breaker is ignored: recovery is driven
            // through `acquire` flipping it to half-open first.
            State::Open { .. } => {}
        }
    }

    /// Record a failed call. Opens the breaker once the failure threshold is hit
    /// while closed, or immediately re-opens (longer cooldown) while half-open.
    pub fn record_failure(&self) {
        let mut inner = self.lock();
        match inner.state {
            State::Closed { failures } => {
                let failures = failures + 1;
                if failures >= self.cfg.failure_threshold {
                    let cooldown = inner.cooldown.next_delay();
                    inner.state = State::Open {
                        until: Instant::now() + cooldown,
                    };
                } else {
                    inner.state = State::Closed { failures };
                }
            }
            State::HalfOpen { .. } => {
                let cooldown = inner.cooldown.next_delay();
                inner.state = State::Open {
                    until: Instant::now() + cooldown,
                };
            }
            State::Open { .. } => {}
        }
    }

    /// Current health for readiness reporting: closed is `Up`, half-open (or an
    /// elapsed cooldown awaiting its first probe) is `Degraded`, still-cooling
    /// open is `Down`.
    #[must_use]
    pub fn status(&self) -> HealthStatus {
        let inner = self.lock();
        match inner.state {
            State::Closed { .. } => HealthStatus::Up,
            State::HalfOpen { .. } => HealthStatus::Degraded,
            State::Open { until } => {
                if Instant::now() >= until {
                    HealthStatus::Degraded
                } else {
                    HealthStatus::Down
                }
            }
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // Poisoning only happens if a holder panicked mid-update; the state is
        // still readable and self-correcting, so recover rather than propagate.
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Runs `op` behind `breaker`: fail-fast with `Backend("circuit_open")` when the
/// breaker rejects, otherwise record the outcome and propagate it. The single
/// wrapper a backend-touching call site uses. Only transport faults trip the
/// breaker; logical errors (validation, not-found, forbidden) count as the
/// backend having answered.
///
/// # Errors
/// Returns `Repository(Backend("circuit_open"))` when the breaker is open and
/// rejects the call; otherwise propagates the error `op` produced.
pub async fn guarded<F, Fut, T>(breaker: &CircuitBreaker, op: F) -> Result<T>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    if !breaker.acquire() {
        return Err(Error::Repository(RepositoryError::Backend(
            "circuit_open".into(),
        )));
    }
    match op().await {
        Ok(value) => {
            breaker.record_success();
            Ok(value)
        }
        Err(err) => {
            if is_transport_fault(&err) {
                breaker.record_failure();
            } else {
                breaker.record_success();
            }
            Err(err)
        }
    }
}

/// Whether an error reflects the backend being unreachable (vs a logical reply).
fn is_transport_fault(err: &Error) -> bool {
    matches!(
        err,
        Error::Repository(RepositoryError::Backend(_))
            | Error::Storage(StorageError::Backend(_))
            | Error::Event(_)
            | Error::Job(_)
            | Error::Render(RenderError::Backend(_))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fast_cfg() -> CircuitConfig {
        CircuitConfig {
            failure_threshold: 2,
            success_threshold: 1,
            half_open_max: 1,
            base_cooldown: Duration::from_millis(20),
            max_cooldown: Duration::from_millis(40),
        }
    }

    #[test]
    fn opens_after_threshold_then_fast_fails() {
        let cb = CircuitBreaker::new(fast_cfg());
        assert!(cb.acquire());
        cb.record_failure();
        assert_eq!(cb.status(), HealthStatus::Up);
        cb.record_failure();
        assert_eq!(cb.status(), HealthStatus::Down);
        assert!(!cb.acquire(), "open breaker must reject");
    }

    #[test]
    fn recovers_through_half_open() {
        let cb = CircuitBreaker::new(fast_cfg());
        cb.record_failure();
        cb.record_failure();
        // Sleep past max_cooldown so the jittered cooldown has certainly elapsed.
        std::thread::sleep(Duration::from_millis(60));
        // Cooldown elapsed: first acquire flips to half-open and is admitted.
        assert!(cb.acquire());
        assert_eq!(cb.status(), HealthStatus::Degraded);
        cb.record_success();
        assert_eq!(cb.status(), HealthStatus::Up);
    }
}
