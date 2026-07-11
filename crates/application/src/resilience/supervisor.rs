use std::{
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

use tokio::{
    task::{AbortHandle, JoinHandle},
    time,
};

use super::backoff::Backoff;

/// Restart delay floor / ceiling and the run duration that counts as "stable".
const RESTART_BASE: Duration = Duration::from_secs(1);
const RESTART_CAP: Duration = Duration::from_mins(1);
const STABLE_THRESHOLD: Duration = Duration::from_mins(1);

/// Current child registration, shared between the supervisor loop and `abort`.
struct ChildSlot {
    stopped: bool,
    child: Option<AbortHandle>,
}

/// Handle to a supervised loop. Dropping it detaches the supervisor (the loop
/// keeps running); [`SupervisorHandle::abort`] stops respawning and cancels the
/// current run at its next await point.
pub struct SupervisorHandle {
    handle: JoinHandle<()>,
    slot: Arc<Mutex<ChildSlot>>,
}

impl SupervisorHandle {
    /// Stops the supervisor and aborts the currently-running child.
    pub fn abort(&self) {
        let mut slot = lock(&self.slot);
        slot.stopped = true;
        if let Some(child) = slot.child.take() {
            child.abort();
        }
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
    let slot = Arc::new(Mutex::new(ChildSlot {
        stopped: false,
        child: None,
    }));
    let shared = slot.clone();
    let handle = tokio::spawn(async move {
        let mut backoff = Backoff::new(RESTART_BASE, RESTART_CAP);
        loop {
            let started = Instant::now();
            let child = tokio::spawn(factory());
            {
                // Register under the lock so an abort racing the spawn still
                // cancels this child instead of orphaning it.
                let mut slot = lock(&shared);
                if slot.stopped {
                    child.abort();
                } else {
                    slot.child = Some(child.abort_handle());
                }
            }
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
            time::sleep(backoff.next_delay()).await;
        }
    });
    SupervisorHandle { handle, slot }
}

fn lock(slot: &Mutex<ChildSlot>) -> MutexGuard<'_, ChildSlot> {
    slot.lock().unwrap_or_else(PoisonError::into_inner)
}
