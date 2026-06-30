//! Periodic leave-balance expiry sweep: warns on grants nearing expiry and lapses
//! grants whose expiry has passed. Idempotent, safe to abort.

use std::{sync::Arc, time::Duration as StdDuration};

use time::OffsetDateTime;

use application::LeaveBalanceService;

/// Ticks on `interval`; runs the leave-balance expiry sweep on each wake.
pub async fn run(leave: Arc<LeaveBalanceService>, interval: StdDuration) {
    let mut ticker = tokio::time::interval(interval);
    loop {
        ticker.tick().await;
        let today = OffsetDateTime::now_utc().date();
        match leave.run_expiry(today).await {
            Ok(()) => tracing::info!("leave expiry sweep complete"),
            Err(e) => tracing::error!(error = %e, "leave expiry sweep failed"),
        }
    }
}
