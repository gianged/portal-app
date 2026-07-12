use std::{
    fmt::Display,
    time::{Duration, Instant},
};

use tokio::time;

use super::backoff::Backoff;

/// Retry delay floor / ceiling while waiting for a backend to come up.
const RETRY_BASE: Duration = Duration::from_secs(1);
const RETRY_CAP: Duration = Duration::from_secs(10);

/// Retries `op` with jittered backoff until it succeeds or no retry fits before
/// `deadline`, then returns the last error. Each failure is logged on target
/// `startup`, so a binary waiting for infra stays alive and visible instead of
/// failing fast.
pub async fn until_deadline<T, E, F, Fut>(name: &str, deadline: Instant, mut op: F) -> Result<T, E>
where
    E: Display,
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let mut backoff = Backoff::new(RETRY_BASE, RETRY_CAP);
    loop {
        let err = match op().await {
            Ok(value) => return Ok(value),
            Err(err) => err,
        };
        let delay = backoff.next_delay();
        if Instant::now() + delay >= deadline {
            tracing::error!(target: "startup", backend = %name, error = %err, "still unavailable at deadline; giving up");
            return Err(err);
        }
        tracing::warn!(target: "startup", backend = %name, error = %err, retry_in = ?delay, "unavailable; retrying");
        time::sleep(delay).await;
    }
}
