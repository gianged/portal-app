//! Periodic orphan-upload sweep: deletes stored files that no attachment or
//! avatar references and that are older than the grace window. Runs on a fixed
//! interval; the grace window protects in-flight uploads whose DB row has not
//! committed yet.

use std::sync::Arc;
use std::time::Duration as StdDuration;

use time::{Duration, OffsetDateTime};

use application::MaintenanceService;

pub async fn run(maintenance: Arc<MaintenanceService>, grace: Duration, interval: StdDuration) {
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
