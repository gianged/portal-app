//! Shared helpers for the apalis job handlers.

use std::{sync::Arc, time::Duration};

use apalis::prelude::{BoxDynError, Error};
use tokio::time;

use application::resilience::CircuitBreaker;
use domain::health::HealthStatus;

/// Sleep step while parked on an open breaker.
const PARK_STEP: Duration = Duration::from_secs(5);
/// Longest a single attempt parks before conceding one retry attempt.
const PARK_MAX: Duration = Duration::from_mins(5);

/// Parks the job while `breaker` is open, so a backend outage pauses the queue
/// instead of burning the storage-side retry budget (5 attempts per job, one
/// per ~30s otherwise). Returns `false` when the breaker is still open after
/// [`PARK_MAX`]; that attempt is then surrendered as a retryable failure.
pub async fn park_while_open(breaker: &CircuitBreaker) -> bool {
    let mut waited = Duration::ZERO;
    while breaker.status() == HealthStatus::Down {
        if waited >= PARK_MAX {
            return false;
        }
        time::sleep(PARK_STEP).await;
        waited += PARK_STEP;
    }
    true
}

/// Transient failure: apalis keeps the job queued and retries it.
pub fn failed<E: std::error::Error + Send + Sync + 'static>(e: E) -> Error {
    Error::Failed(Arc::new(Box::new(e) as BoxDynError))
}

/// Transient failure from a static message.
pub fn retryable(msg: &'static str) -> Error {
    Error::Failed(Arc::new(Box::<dyn std::error::Error + Send + Sync>::from(
        msg,
    )))
}

/// Permanent failure: apalis kills the job immediately, no retries.
pub fn abort<E: std::error::Error + Send + Sync + 'static>(e: E) -> Error {
    Error::Abort(Arc::new(Box::new(e) as BoxDynError))
}
