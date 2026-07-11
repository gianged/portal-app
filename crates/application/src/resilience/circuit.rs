use std::{
    sync::{Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

use domain::health::HealthStatus;

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

    /// Try to admit a call. `Some` when closed, or when half-open with a free
    /// probe slot, or when an open cooldown has elapsed (which transitions the
    /// breaker to half-open and consumes the first probe slot). `None` while
    /// open and still cooling down. Report the outcome through the returned
    /// [`Permit`]; dropping it unreported returns the probe slot.
    pub fn acquire(&self) -> Option<Permit<'_>> {
        let mut inner = self.lock();
        let half_open_slot = match inner.state {
            State::Closed { .. } => false,
            State::Open { until } => {
                if Instant::now() < until {
                    return None;
                }
                inner.state = State::HalfOpen {
                    in_flight: 1,
                    successes: 0,
                };
                true
            }
            State::HalfOpen {
                in_flight,
                successes,
            } => {
                if in_flight >= self.cfg.half_open_max {
                    return None;
                }
                inner.state = State::HalfOpen {
                    in_flight: in_flight + 1,
                    successes,
                };
                true
            }
        };
        Some(Permit {
            breaker: self,
            half_open_slot,
            settled: false,
        })
    }

    /// Record a successful call. Resets the failure count while closed; advances
    /// the half-open success count and closes once the threshold is reached.
    /// Acquire-gated callers report through their [`Permit`] instead; calling
    /// this directly is for ungated feedback sources like the drainer.
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
    /// Same direct-call caveat as [`Self::record_success`].
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

    /// Return an unreported half-open probe slot so a cancelled call cannot pin
    /// `in_flight` at the quota forever.
    fn release_half_open_slot(&self) {
        let mut inner = self.lock();
        if let State::HalfOpen {
            in_flight,
            successes,
        } = inner.state
        {
            inner.state = State::HalfOpen {
                in_flight: in_flight.saturating_sub(1),
                successes,
            };
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // Poisoning only happens if a holder panicked mid-update; the state is
        // still readable and self-correcting, so recover rather than propagate.
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Admission token from [`CircuitBreaker::acquire`]. Holds the half-open probe
/// slot: report the call's outcome through it, or drop it (e.g. on future
/// cancellation) to release the slot without one.
pub struct Permit<'a> {
    breaker: &'a CircuitBreaker,
    half_open_slot: bool,
    settled: bool,
}

impl Permit<'_> {
    /// Record the admitted call as successful.
    pub fn record_success(mut self) {
        self.settled = true;
        self.breaker.record_success();
    }

    /// Record the admitted call as failed.
    pub fn record_failure(mut self) {
        self.settled = true;
        self.breaker.record_failure();
    }
}

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        if !self.settled && self.half_open_slot {
            self.breaker.release_half_open_slot();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

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
        cb.acquire()
            .expect("closed breaker admits")
            .record_failure();
        assert_eq!(cb.status(), HealthStatus::Up);
        cb.record_failure();
        assert_eq!(cb.status(), HealthStatus::Down);
        assert!(cb.acquire().is_none(), "open breaker must reject");
    }

    #[test]
    fn recovers_through_half_open() {
        let cb = CircuitBreaker::new(fast_cfg());
        cb.record_failure();
        cb.record_failure();
        // Sleep past max_cooldown so the jittered cooldown has certainly elapsed.
        thread::sleep(Duration::from_millis(60));
        // Cooldown elapsed: first acquire flips to half-open and is admitted.
        let permit = cb.acquire().expect("elapsed cooldown admits a probe");
        assert_eq!(cb.status(), HealthStatus::Degraded);
        permit.record_success();
        assert_eq!(cb.status(), HealthStatus::Up);
    }

    #[test]
    fn dropped_permit_releases_half_open_slot() {
        let cb = CircuitBreaker::new(fast_cfg());
        cb.record_failure();
        cb.record_failure();
        thread::sleep(Duration::from_millis(60));
        // half_open_max is 1: an unreported drop must free the only slot.
        drop(cb.acquire().expect("first probe admitted"));
        cb.acquire()
            .expect("slot released by drop")
            .record_success();
        assert_eq!(cb.status(), HealthStatus::Up);
    }
}
