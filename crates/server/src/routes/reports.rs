//! Company report endpoints (Director/HR only, gated inside `ReportService`).
//!
//! Stats endpoints back the in-app dashboard; the `generate` endpoints render and
//! store a PDF and return a signed download URL (the same `/files` mechanism the
//! attachments use). The archive list carries a fresh signed URL per item.

use std::time::Duration;

use axum::{
    Json, Router,
    extract::{Query, State},
    routing::{get, post},
};
use serde::Deserialize;

use domain::{ids::UserId, model::Report, ports::file_storage::FileStorage};
use shared::dto::report::{MonthlyReportDto, ReportSummaryDto, YearlyReportDto};

use crate::{app::AppState, dto, error::AppError, extractors::auth_user::AuthUser};

/// Signed report links live an hour: long enough to click through, short enough
/// not to linger as a bearer token.
const URL_TTL: Duration = Duration::from_secs(3600);
const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 200;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/reports", get(list))
        .route("/reports/monthly", get(monthly_stats))
        .route("/reports/yearly", get(yearly_stats))
        .route("/reports/monthly/generate", post(generate_monthly))
        .route("/reports/yearly/generate", post(generate_yearly))
}

#[derive(Deserialize)]
struct ListQuery {
    limit: Option<u32>,
}

#[derive(Deserialize)]
struct MonthlyQuery {
    year: i32,
    month: u8,
}

#[derive(Deserialize)]
struct YearlyQuery {
    year: i32,
}

async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<ReportSummaryDto>>, AppError> {
    state.perms.require_admin(auth.user_id).await?;
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let reports = state.report.list_reports(limit).await?;
    let mut out = Vec::with_capacity(reports.len());
    for report in &reports {
        out.push(summary(&state, report, auth.user_id).await?);
    }
    Ok(Json(out))
}

async fn monthly_stats(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<MonthlyQuery>,
) -> Result<Json<MonthlyReportDto>, AppError> {
    state.perms.require_admin(auth.user_id).await?;
    let data = state.report.monthly_stats(q.year, q.month).await?;
    Ok(Json(dto::monthly_report_dto(&data)))
}

async fn yearly_stats(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<YearlyQuery>,
) -> Result<Json<YearlyReportDto>, AppError> {
    state.perms.require_admin(auth.user_id).await?;
    let data = state.report.yearly_stats(q.year).await?;
    Ok(Json(dto::yearly_report_dto(&data)))
}

async fn generate_monthly(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<MonthlyQuery>,
) -> Result<Json<ReportSummaryDto>, AppError> {
    state.perms.require_admin(auth.user_id).await?;
    let report = state
        .report
        .generate_monthly(q.year, q.month, Some(auth.user_id))
        .await?;
    Ok(Json(summary(&state, &report, auth.user_id).await?))
}

async fn generate_yearly(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<YearlyQuery>,
) -> Result<Json<ReportSummaryDto>, AppError> {
    state.perms.require_admin(auth.user_id).await?;
    let report = state
        .report
        .generate_yearly(q.year, Some(auth.user_id))
        .await?;
    Ok(Json(summary(&state, &report, auth.user_id).await?))
}

/// Mints a viewer-bound signed download URL for the report's stored PDF.
async fn summary(
    state: &AppState,
    report: &Report,
    viewer: UserId,
) -> Result<ReportSummaryDto, AppError> {
    let url = state
        .storage
        .presign_get(&report.storage_key, URL_TTL, viewer)
        .await
        .map_err(|e| AppError::Domain(application::Error::Storage(e)))?;
    Ok(dto::report_summary_dto(report, url))
}
