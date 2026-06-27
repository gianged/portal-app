use async_trait::async_trait;

use crate::{error::HealthError, health::BackendId};

/// One cheap liveness probe per backend. Declared as a port so the prober lives
/// in `application` while the concrete pings (a `SELECT 1`, a `PING`, an HTTP
/// `/healthz`) live in `infrastructure`. The probe must bound its own wait so a
/// hung backend reports `Down` quickly rather than blocking the prober.
#[async_trait]
pub trait HealthCheck: Send + Sync {
    /// Which backend this probe covers.
    fn backend(&self) -> BackendId;

    /// Succeeds when the backend answers a trivial request in time.
    async fn ping(&self) -> Result<(), HealthError>;
}
