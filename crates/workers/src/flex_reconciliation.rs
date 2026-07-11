//! Month-end flex settlement sweep: near the end of each month, warns users whose
//! approved flex hours do not net to the expected monthly total. Idempotent.

use std::sync::Arc;

use time::{Date, OffsetDateTime};

use application::FlexHoursService;

/// Ticks on `interval`; near month-end, emits unreconciled flex warnings.
pub async fn run(flex: Arc<FlexHoursService>, interval: std::time::Duration) {
    let mut ticker = tokio::time::interval(interval);
    loop {
        ticker.tick().await;
        let today = OffsetDateTime::now_utc().date();
        if !near_month_end(today) {
            continue;
        }
        let year = today.year();
        let month = u8::from(today.month());
        match flex.emit_unreconciled(year, month).await {
            Ok(()) => tracing::info!("flex reconciliation sweep complete"),
            Err(e) => tracing::error!(error = %e, "flex reconciliation sweep failed"),
        }
    }
}

/// True within the last three days of the month, so the sweep fires regardless of
/// the exact daily tick alignment.
fn near_month_end(today: Date) -> bool {
    let last = today.month().length(today.year());
    u16::from(today.day()) + 2 >= u16::from(last)
}
