//! Report HTTP wrappers (Director/HR only; the server enforces a 403). Stats power
//! the dashboard; the `generate_*` calls render + store a PDF and return a signed
//! download URL.

use uuid::Uuid;

use shared::dto::report::{
    MonthlyReportDto, ReportSummaryDto, StaffMonthlyReportDto, YearlyReportDto,
};

use crate::api::client;
use crate::api::error::FrontendError;

pub async fn monthly(year: i32, month: u8) -> Result<MonthlyReportDto, FrontendError> {
    let (y, m) = (year.to_string(), month.to_string());
    let q = client::query(&[("year", &y), ("month", &m)]);
    client::get_json(&format!("/reports/monthly{q}")).await
}

pub async fn yearly(year: i32) -> Result<YearlyReportDto, FrontendError> {
    let y = year.to_string();
    let q = client::query(&[("year", &y)]);
    client::get_json(&format!("/reports/yearly{q}")).await
}

pub async fn staff_monthly(
    user_id: Uuid,
    year: i32,
    month: u8,
) -> Result<StaffMonthlyReportDto, FrontendError> {
    let (y, m) = (year.to_string(), month.to_string());
    let q = client::query(&[("year", &y), ("month", &m)]);
    client::get_json(&format!("/reports/staff/{user_id}/monthly{q}")).await
}

pub async fn archive_list() -> Result<Vec<ReportSummaryDto>, FrontendError> {
    client::get_json("/reports").await
}

pub async fn generate_monthly(year: i32, month: u8) -> Result<ReportSummaryDto, FrontendError> {
    let (y, m) = (year.to_string(), month.to_string());
    let q = client::query(&[("year", &y), ("month", &m)]);
    client::post_empty(&format!("/reports/monthly/generate{q}")).await
}

pub async fn generate_yearly(year: i32) -> Result<ReportSummaryDto, FrontendError> {
    let y = year.to_string();
    let q = client::query(&[("year", &y)]);
    client::post_empty(&format!("/reports/yearly/generate{q}")).await
}
