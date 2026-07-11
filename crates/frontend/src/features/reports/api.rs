//! Report HTTP wrappers (Director/HR only, server-enforced); `generate_*` store a PDF and return a signed download URL.

use shared::dto::ids::UserId;
use shared::dto::report::{
    MonthlyReportDto, ReportSummaryDto, StaffMonthlyReportDto, YearlyReportDto,
};

use crate::api::client;
use crate::api::error::FrontendError;

/// Company-wide monthly report for the given period.
pub async fn monthly(year: i32, month: u8) -> Result<MonthlyReportDto, FrontendError> {
    let (y, m) = (year.to_string(), month.to_string());
    let q = client::query(&[("year", &y), ("month", &m)]);
    client::get_json(&format!("/reports/monthly{q}")).await
}

/// Company-wide yearly report with month-by-month growth series.
pub async fn yearly(year: i32) -> Result<YearlyReportDto, FrontendError> {
    let y = year.to_string();
    let q = client::query(&[("year", &y)]);
    client::get_json(&format!("/reports/yearly{q}")).await
}

/// One user's monthly report (hours, attendance, leave, requests).
pub async fn staff_monthly(
    user_id: UserId,
    year: i32,
    month: u8,
) -> Result<StaffMonthlyReportDto, FrontendError> {
    let (y, m) = (year.to_string(), month.to_string());
    let q = client::query(&[("year", &y), ("month", &m)]);
    client::get_json(&format!("/reports/staff/{}/monthly{q}", user_id.0)).await
}

/// All previously generated report PDFs, newest first.
pub async fn archive_list() -> Result<Vec<ReportSummaryDto>, FrontendError> {
    client::get_json("/reports").await
}

/// Generate and store the monthly PDF, returning its archive entry.
pub async fn generate_monthly(year: i32, month: u8) -> Result<ReportSummaryDto, FrontendError> {
    let (y, m) = (year.to_string(), month.to_string());
    let q = client::query(&[("year", &y), ("month", &m)]);
    client::post_empty(&format!("/reports/monthly/generate{q}")).await
}

/// Generate and store the yearly PDF, returning its archive entry.
pub async fn generate_yearly(year: i32) -> Result<ReportSummaryDto, FrontendError> {
    let y = year.to_string();
    let q = client::query(&[("year", &y)]);
    client::post_empty(&format!("/reports/yearly/generate{q}")).await
}
