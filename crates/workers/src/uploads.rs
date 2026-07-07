//! Periodic orphan-upload sweep: deletes stored files no attachment or avatar
//! references that are older than the grace window. Grace protects in-flight uploads.

use std::sync::Arc;

use time::{Duration, OffsetDateTime};

use application::MaintenanceService;

pub async fn run(
    maintenance: Arc<MaintenanceService>,
    grace: Duration,
    interval: std::time::Duration,
) {
    let mut ticker = tokio::time::interval(interval);
    loop {
        ticker.tick().await;
        let now = OffsetDateTime::now_utc();
        match maintenance.sweep_orphan_uploads(grace, now).await {
            Ok(swept) => tracing::info!(swept, "orphan upload sweep complete"),
            Err(e) => tracing::error!(error = %e, "orphan upload sweep failed"),
        }
    }
}
