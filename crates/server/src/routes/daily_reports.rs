//! Daily-report endpoints. Staff upsert and submit their own day; a leader (or
//! HR) reviews a group's reports. Dates are `"YYYY-MM-DD"` path/query values.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing,
};
use serde::Deserialize;
use uuid::Uuid;

use domain::{
    ids::{DailyReportId, GroupId, UserId},
    model::DailyReport,
};
use shared::dto::daily_report::{
    DailyReportDto, ReviewDailyReportRequest, UpsertDailyReportRequest,
};

use crate::{
    app::AppState,
    dto,
    error::AppError,
    extractors::{auth_user::AuthUser, validated_json::ValidatedJson},
    resolve, routes,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/daily-reports", routing::get(list_mine))
        .route(
            "/daily-reports/{date}",
            routing::get(get_by_date).put(upsert),
        )
        .route("/daily-reports/{id}/submit", routing::post(submit))
        .route("/daily-reports/{id}/review", routing::post(review))
        .route("/groups/{id}/daily-reports", routing::get(list_for_group))
}

#[derive(Deserialize)]
struct RangeQuery {
    from: String,
    to: String,
}

async fn list_mine(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<RangeQuery>,
) -> Result<Json<Vec<DailyReportDto>>, AppError> {
    let from = routes::parse_date(&q.from)?;
    let to = routes::parse_date(&q.to)?;
    let reports = state.daily_report.list_mine(auth.user_id, from, to).await?;
    Ok(Json(many(&state, reports).await?))
}

/// The actor's report for a date, or `null` when none exists yet (so the editor
/// can render a blank draft).
async fn get_by_date(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(date): Path<String>,
) -> Result<Json<Option<DailyReportDto>>, AppError> {
    let date = routes::parse_date(&date)?;
    let report = state
        .daily_report
        .list_mine(auth.user_id, date, date)
        .await?
        .into_iter()
        .next();
    match report {
        Some(r) => Ok(Json(Some(single(&state, &r).await?))),
        None => Ok(Json(None)),
    }
}

async fn upsert(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(date): Path<String>,
    ValidatedJson(body): ValidatedJson<UpsertDailyReportRequest>,
) -> Result<Json<DailyReportDto>, AppError> {
    let date = routes::parse_date(&date)?;
    let cmd = dto::upsert_daily_report_command(date, body);
    let report = state.daily_report.upsert_draft(auth.user_id, cmd).await?;
    Ok(Json(single(&state, &report).await?))
}

async fn submit(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<DailyReportDto>, AppError> {
    let report = state
        .daily_report
        .submit(auth.user_id, DailyReportId(id))
        .await?;
    Ok(Json(single(&state, &report).await?))
}

async fn review(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    ValidatedJson(body): ValidatedJson<ReviewDailyReportRequest>,
) -> Result<Json<DailyReportDto>, AppError> {
    let report = state
        .daily_report
        .review(
            auth.user_id,
            DailyReportId(id),
            dto::review_daily_report_command(body),
        )
        .await?;
    Ok(Json(single(&state, &report).await?))
}

async fn list_for_group(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(q): Query<RangeQuery>,
) -> Result<Json<Vec<DailyReportDto>>, AppError> {
    let from = routes::parse_date(&q.from)?;
    let to = routes::parse_date(&q.to)?;
    let reports = state
        .daily_report
        .list_for_group(auth.user_id, GroupId(id), from, to)
        .await?;
    Ok(Json(many(&state, reports).await?))
}

/// Resolves one report's owner + reviewer summaries.
async fn single(state: &AppState, report: &DailyReport) -> Result<DailyReportDto, AppError> {
    let user = resolve::user_summary(&state.user, &state.group, report.user_id).await?;
    let reviewed_by =
        resolve::opt_user_summary(&state.user, &state.group, report.reviewed_by).await?;
    Ok(dto::daily_report_dto(report, user, reviewed_by))
}

/// Resolves a batch of reports, deduplicating owner / reviewer lookups.
async fn many(
    state: &AppState,
    reports: Vec<DailyReport>,
) -> Result<Vec<DailyReportDto>, AppError> {
    let mut ids: Vec<UserId> = Vec::with_capacity(reports.len() * 2);
    for r in &reports {
        ids.push(r.user_id);
        if let Some(reviewer) = r.reviewed_by {
            ids.push(reviewer);
        }
    }
    let users = resolve::user_map(&state.user, &state.group, ids).await?;
    Ok(reports
        .iter()
        .map(|r| {
            let user = resolve::summary_from(&users, r.user_id);
            let reviewed_by = r.reviewed_by.map(|id| resolve::summary_from(&users, id));
            dto::daily_report_dto(r, user, reviewed_by)
        })
        .collect())
}
