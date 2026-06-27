use std::time::{Duration, Instant};

use tokio::task::JoinHandle;

use super::backoff::Backoff;

/// Restart delay floor / ceiling and the run duration that counts as "stable".
const RESTART_BASE: Duration = Duration::from_secs(1);
const RESTART_CAP: Duration = Duration::from_secs(60);
const STABLE_THRESHOLD: Duration = Duration::from_secs(60);

/// Handle to a supervised loop. Dropping it detaches the supervisor (the loop
/// keeps running); [`SupervisorHandle::abort`] stops it at the next boundary.
pub struct SupervisorHandle {
    handle: JoinHandle<()>,
}

impl SupervisorHandle {
    /// Stops the supervisor task. The currently-running child is detached, not
    /// cancelled; supervised loops here are idempotent and safe to abandon.
    pub fn abort(&self) {
        self.handle.abort();
    }
}

/// Spawns `factory`'s future and keeps it alive: if it returns or panics, log the
/// cause on target `supervisor` and respawn after a jittered backoff. A run that
/// lasts past the stable-threshold resets the ramp, so an occasional crash backs
/// off briefly while a crash loop backs off toward the ceiling.
///
/// The supervised future runs in its own task so an unwinding panic is caught by
/// the join handle (and logged once by the panic hook) rather than killing the
/// supervisor.
pub fn supervise<F, Fut>(name: impl Into<String>, mut factory: F) -> SupervisorHandle
where
    F: FnMut() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let name = name.into();
    let handle = tokio::spawn(async move {
        let mut backoff = Backoff::new(RESTART_BASE, RESTART_CAP);
        loop {
            let started = Instant::now();
            let child = tokio::spawn(factory());
            match child.await {
                Ok(()) => {
                    tracing::warn!(target: "supervisor", task = %name, "task exited; restarting");
                }
                Err(e) if e.is_cancelled() => {
                    tracing::info!(target: "supervisor", task = %name, "task cancelled; supervisor stopping");
                    break;
                }
                Err(e) if e.is_panic() => {
                    tracing::error!(target: "supervisor", task = %name, "task panicked; restarting");
                }
                Err(_) => {
                    tracing::error!(target: "supervisor", task = %name, "task join failed; restarting");
                }
            }
            // Stable run -> restart the ramp from the floor; crash loop -> grow it.
            if started.elapsed() >= STABLE_THRESHOLD {
                backoff.reset();
            }
            tokio::time::sleep(backoff.next_delay()).await;
        }
    });
    SupervisorHandle { handle }
}
