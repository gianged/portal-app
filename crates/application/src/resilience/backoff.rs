use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

/// Process-global seed source so each `Backoff` gets a distinct jitter stream
/// without reading the clock, which keeps construction pure and test-friendly.
static SEED: AtomicU64 = AtomicU64::new(0x9E37_79B9_7F4A_7C15);

/// Decorrelated-jitter exponential backoff. Each [`Backoff::next_delay`] picks a delay
/// in `[base, prev * 3]` capped at `cap`, so concurrent retriers spread out
/// instead of resynchronising on a fixed schedule. Pure: no IO, no clock.
///
/// Shared by the supervisor restart delay, the breaker cooldown, and the drainer
/// ramp so the timing policy is defined once.
#[derive(Debug, Clone)]
pub struct Backoff {
    base: Duration,
    cap: Duration,
    current: Duration,
    rng: u64,
}

impl Backoff {
    /// Builds a backoff growing from `base` toward `cap`.
    #[must_use]
    pub fn new(base: Duration, cap: Duration) -> Self {
        // Advance the global seed so two backoffs built back-to-back diverge.
        let seed = SEED.fetch_add(0x2545_F491_4F6C_DD1D, Ordering::Relaxed);
        Self {
            base,
            cap,
            current: base,
            // Force odd: a zero state would freeze the xorshift stream.
            rng: seed | 1,
        }
    }

    /// Next delay, growing the ceiling toward `cap`. Advances internal state.
    pub fn next_delay(&mut self) -> Duration {
        let ceiling = self.current.saturating_mul(3).min(self.cap).max(self.base);
        let span = ceiling.saturating_sub(self.base);
        let picked = self.base + self.rand_span(span);
        self.current = picked.min(self.cap).max(self.base);
        self.current
    }

    /// Reset the ceiling to `base`, called after a run stays up past the
    /// stable-threshold so a later failure restarts the ramp from the floor.
    pub fn reset(&mut self) {
        self.current = self.base;
    }

    /// Uniform-ish offset in `[0, span)` via xorshift64*; small `span` only.
    fn rand_span(&mut self, span: Duration) -> Duration {
        let nanos = u64::try_from(span.as_nanos()).unwrap_or(u64::MAX);
        if nanos == 0 {
            return Duration::ZERO;
        }
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        let r = x.wrapping_mul(0x2545_F491_4F6C_DD1D);
        Duration::from_nanos(r % nanos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stays_within_base_and_cap() {
        let base = Duration::from_millis(10);
        let cap = Duration::from_millis(500);
        let mut backoff = Backoff::new(base, cap);
        for _ in 0..100 {
            let delay = backoff.next_delay();
            assert!(delay >= base, "delay {delay:?} below base");
            assert!(delay <= cap, "delay {delay:?} above cap");
        }
    }

    #[test]
    fn reset_returns_to_floor_ceiling() {
        let base = Duration::from_millis(10);
        let cap = Duration::from_secs(10);
        let mut backoff = Backoff::new(base, cap);
        for _ in 0..20 {
            backoff.next_delay();
        }
        backoff.reset();
        // After reset the ceiling is base*3, so the next delay can't exceed it.
        let delay = backoff.next_delay();
        assert!(delay <= base.saturating_mul(3));
    }
}
