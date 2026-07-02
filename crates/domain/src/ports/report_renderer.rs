use crate::{
    error::RenderError,
    model::{MonthlyReportData, StaffMonthlyReport, YearlyReportData},
};

/// Renders aggregated report data into a PDF document.
///
/// Deliberately a plain (non-`#[async_trait]`) trait: rendering is synchronous,
/// CPU-bound work with no IO, so advertising it as an awaitable future would be
/// misleading. The application service offloads each call onto a blocking thread
/// (`tokio::task::spawn_blocking`) rather than awaiting it directly.
pub trait ReportRenderer: Send + Sync {
    fn render_monthly(&self, data: &MonthlyReportData) -> Result<Vec<u8>, RenderError>;

    fn render_yearly(&self, data: &YearlyReportData) -> Result<Vec<u8>, RenderError>;

    /// One staff member's monthly archive. `subject_name` is resolved by the
    /// caller so the renderer stays IO-free.
    fn render_staff_monthly(
        &self,
        subject_name: &str,
        data: &StaffMonthlyReport,
    ) -> Result<Vec<u8>, RenderError>;
}
