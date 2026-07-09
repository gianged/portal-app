//! External read-only API for scripts (`/api/ext/v1`). Authenticated by the
//! service-account middleware; scope checks live in `ExtReadService`.

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    routing,
};
use serde::Deserialize;
use uuid::Uuid;

use application::service::ExtPage;
use domain::ids::{ProjectId, RequestId};
use shared::dto::{
    ext::{ExtProjectDto, ExtRequestDto, PageDto},
    report::{MonthlyReportDto, YearlyReportDto},
};

use crate::{app::AppState, dto, error::AppError, middleware::service_account::ServiceAccountCtx};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects", routing::get(list_projects))
        .route("/projects/{id}", routing::get(get_project))
        .route("/requests", routing::get(list_requests))
        .route("/requests/{id}", routing::get(get_request))
        .route("/reports/monthly", routing::get(monthly_report))
        .route("/reports/yearly", routing::get(yearly_report))
}

#[derive(Deserialize)]
struct PageQuery {
    after: Option<Uuid>,
    limit: Option<u32>,
}

#[derive(Deserialize)]
struct RequestsQuery {
    project: Option<Uuid>,
    after: Option<Uuid>,
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

fn page_dto<T, D>(page: &ExtPage<T>, map: impl Fn(&T) -> D) -> PageDto<D> {
    PageDto {
        items: page.items.iter().map(map).collect(),
        next_cursor: page.next_cursor.map(|id| id.to_string()),
    }
}

async fn list_projects(
    State(state): State<AppState>,
    Extension(ctx): Extension<ServiceAccountCtx>,
    Query(q): Query<PageQuery>,
) -> Result<Json<PageDto<ExtProjectDto>>, AppError> {
    let page = state
        .ext_read
        .list_projects(ctx.id, q.after.map(ProjectId), q.limit)
        .await?;
    Ok(Json(page_dto(&page, dto::ext_project_dto)))
}

async fn get_project(
    State(state): State<AppState>,
    Extension(ctx): Extension<ServiceAccountCtx>,
    Path(id): Path<Uuid>,
) -> Result<Json<ExtProjectDto>, AppError> {
    let project = state.ext_read.get_project(ctx.id, ProjectId(id)).await?;
    Ok(Json(dto::ext_project_dto(&project)))
}

async fn list_requests(
    State(state): State<AppState>,
    Extension(ctx): Extension<ServiceAccountCtx>,
    Query(q): Query<RequestsQuery>,
) -> Result<Json<PageDto<ExtRequestDto>>, AppError> {
    let page = state
        .ext_read
        .list_requests(
            ctx.id,
            q.project.map(ProjectId),
            q.after.map(RequestId),
            q.limit,
        )
        .await?;
    Ok(Json(page_dto(&page, dto::ext_request_dto)))
}

async fn get_request(
    State(state): State<AppState>,
    Extension(ctx): Extension<ServiceAccountCtx>,
    Path(id): Path<Uuid>,
) -> Result<Json<ExtRequestDto>, AppError> {
    let request = state.ext_read.get_request(ctx.id, RequestId(id)).await?;
    Ok(Json(dto::ext_request_dto(&request)))
}

async fn monthly_report(
    State(state): State<AppState>,
    Extension(ctx): Extension<ServiceAccountCtx>,
    Query(q): Query<MonthlyQuery>,
) -> Result<Json<MonthlyReportDto>, AppError> {
    let data = state
        .ext_read
        .monthly_report(ctx.id, q.year, q.month)
        .await?;
    Ok(Json(dto::monthly_report_dto(&data)))
}

async fn yearly_report(
    State(state): State<AppState>,
    Extension(ctx): Extension<ServiceAccountCtx>,
    Query(q): Query<YearlyQuery>,
) -> Result<Json<YearlyReportDto>, AppError> {
    let data = state.ext_read.yearly_report(ctx.id, q.year).await?;
    Ok(Json(dto::yearly_report_dto(&data)))
}
