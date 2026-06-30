//! Daily-report HTTP wrappers. Dates are `"YYYY-MM-DD"` strings; `get_for_date`
//! returns `None` when no report exists yet for that day.

use shared::dto::daily_report::{
    DailyReportDto, ReviewDailyReportRequest, UpsertDailyReportRequest,
};
use shared::dto::ids::{DailyReportId, GroupId};

use crate::api::client;
use crate::api::error::FrontendError;

pub async fn get_for_date(date: &str) -> Result<Option<DailyReportDto>, FrontendError> {
    client::get_json(&format!("/daily-reports/{date}")).await
}

#[allow(dead_code)]
pub async fn list_mine(from: &str, to: &str) -> Result<Vec<DailyReportDto>, FrontendError> {
    let q = client::query(&[("from", from), ("to", to)]);
    client::get_json(&format!("/daily-reports{q}")).await
}

pub async fn upsert(
    date: &str,
    req: &UpsertDailyReportRequest,
) -> Result<DailyReportDto, FrontendError> {
    client::put_json(&format!("/daily-reports/{date}"), req).await
}

pub async fn submit(id: DailyReportId) -> Result<DailyReportDto, FrontendError> {
    client::post_empty(&format!("/daily-reports/{}/submit", id.0)).await
}

pub async fn review(
    id: DailyReportId,
    req: &ReviewDailyReportRequest,
) -> Result<DailyReportDto, FrontendError> {
    client::post_json(&format!("/daily-reports/{}/review", id.0), req).await
}

pub async fn list_for_group(
    group: GroupId,
    from: &str,
    to: &str,
) -> Result<Vec<DailyReportDto>, FrontendError> {
    let q = client::query(&[("from", from), ("to", to)]);
    client::get_json(&format!("/groups/{}/daily-reports{q}", group.0)).await
}
