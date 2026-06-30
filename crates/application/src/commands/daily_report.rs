use domain::{ids::RequestId, model::DailyReportEntryKind};
use time::Date;

/// One line of a daily report. `progress` is a best-effort completion hint for a
/// `RequestWork` entry; it bumps the linked request when the owner is its
/// assignee and the request is in progress.
#[derive(Debug, Clone)]
pub struct DailyReportEntryInput {
    pub kind: DailyReportEntryKind,
    pub description: String,
    pub request_id: Option<RequestId>,
    pub hours: Option<f64>,
    pub progress: Option<u8>,
}

/// Create-or-replace a draft report for `(actor, report_date)`.
#[derive(Debug, Clone)]
pub struct UpsertDailyReportCommand {
    pub report_date: Date,
    pub summary: String,
    pub entries: Vec<DailyReportEntryInput>,
}

/// A leader's decision on a submitted report.
#[derive(Debug, Clone)]
pub struct ReviewDailyReportCommand {
    pub approve: bool,
    pub note: String,
}
