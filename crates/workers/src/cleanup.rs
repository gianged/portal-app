//! Periodic notification-retention sweep: prunes read notifications older than
//! the configured retention window. Runs on a fixed interval alongside the
//! notification consumer; aborted at shutdown and resumed on the next launch
//! (the operation is idempotent).

use std::sync::Arc;
use std::time::Duration as StdDuration;

use time::{Duration, OffsetDateTime};

use application::MaintenanceService;

pub async fn run(maintenance: Arc<MaintenanceService>, retention: Duration, interval: StdDuration) {
    let mut ticker = tokio::time::interval(interval);
    loop {
        ticker.tick().await;
        let now = OffsetDateTime::now_utc();
        match maintenance.prune_read_notifications(retention, now).await {
            Ok(pruned) => tracing::info!(pruned, "notification retention sweep complete"),
            Err(e) => tracing::error!(error = %e, "notification retention sweep failed"),
        }
    }
}
