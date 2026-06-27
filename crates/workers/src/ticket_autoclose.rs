//! Periodic ticket auto-close sweep: closes resolved tickets whose reopen window
//! has lapsed, emitting audit + notification events. Idempotent, safe to abort.

use std::sync::Arc;
use std::time::Duration as StdDuration;

use time::{Duration, OffsetDateTime};

use application::MaintenanceService;

pub async fn run(maintenance: Arc<MaintenanceService>, window: Duration, interval: StdDuration) {
    let mut ticker = tokio::time::interval(interval);
    loop {
        ticker.tick().await;
        let now = OffsetDateTime::now_utc();
        match maintenance.auto_close_resolved_tickets(window, now).await {
            Ok(closed) => tracing::info!(closed, "ticket auto-close sweep complete"),
            Err(e) => tracing::error!(error = %e, "ticket auto-close sweep failed"),
        }
    }
}
