//! Trusted internal read plane: projects, requests, and report aggregates
//! straight from the repositories/report service, no per-user authz. The token
//! interceptor on the listener is the only gate.

use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use application::service::ReportService;
use domain::{
    error::RepositoryError,
    ids::{ProjectId, RequestId},
    model::{MonthlyReportData, Project, YearlyReportData},
    repository::{ProjectRepository, RequestRepository},
};
use proto::{
    internal::v1::{
        GetProjectRequest, GetProjectResponse, GetRequestRequest, GetRequestResponse,
        GroupHeadcount, LabelCount, ListProjectsRequest, ListProjectsResponse, ListRequestsRequest,
        ListRequestsResponse, MonthlyReportRequest, MonthlyReportStats, ProjectRecord,
        RequestRecord, YearlyReportRequest, YearlyReportStats, query_server::Query,
    },
    tonic::{Response, Status},
};

const DEFAULT_LIMIT: u32 = 100;
const MAX_LIMIT: u32 = 500;

pub struct QueryService {
    projects: Arc<dyn ProjectRepository>,
    requests: Arc<dyn RequestRepository>,
    report: Arc<ReportService>,
}

impl QueryService {
    #[must_use]
    pub fn new(
        projects: Arc<dyn ProjectRepository>,
        requests: Arc<dyn RequestRepository>,
        report: Arc<ReportService>,
    ) -> Self {
        Self {
            projects,
            requests,
            report,
        }
    }
}

#[proto::tonic::async_trait]
impl Query for QueryService {
    async fn list_projects(
        &self,
        request: proto::tonic::Request<ListProjectsRequest>,
    ) -> Result<Response<ListProjectsResponse>, Status> {
        let ListProjectsRequest { after, limit } = request.into_inner();
        let after = parse_cursor(&after)?.map(ProjectId);
        let limit = clamp_limit(limit);
        let rows = self
            .projects
            .list_page(after, limit)
            .await
            .map_err(repo_status)?;
        let next_cursor = next_cursor(rows.len(), limit, rows.last().map(|p| p.id.0));
        Ok(Response::new(ListProjectsResponse {
            projects: rows.iter().map(project_record).collect(),
            next_cursor,
        }))
    }

    async fn get_project(
        &self,
        request: proto::tonic::Request<GetProjectRequest>,
    ) -> Result<Response<GetProjectResponse>, Status> {
        let id = parse_id(&request.into_inner().id).map(ProjectId)?;
        let project = self
            .projects
            .find_by_id(id)
            .await
            .map_err(repo_status)?
            .ok_or_else(|| Status::not_found("project not found"))?;
        Ok(Response::new(GetProjectResponse {
            project: Some(project_record(&project)),
        }))
    }

    async fn list_requests(
        &self,
        request: proto::tonic::Request<ListRequestsRequest>,
    ) -> Result<Response<ListRequestsResponse>, Status> {
        let ListRequestsRequest {
            project_id,
            after,
            limit,
        } = request.into_inner();
        let project = parse_cursor(&project_id)?.map(ProjectId);
        let after = parse_cursor(&after)?.map(RequestId);
        let limit = clamp_limit(limit);
        let rows = self
            .requests
            .list_page(project, after, limit)
            .await
            .map_err(repo_status)?;
        let next_cursor = next_cursor(rows.len(), limit, rows.last().map(|r| r.id.0));
        Ok(Response::new(ListRequestsResponse {
            requests: rows.iter().map(request_record).collect(),
            next_cursor,
        }))
    }

    async fn get_request(
        &self,
        request: proto::tonic::Request<GetRequestRequest>,
    ) -> Result<Response<GetRequestResponse>, Status> {
        let id = parse_id(&request.into_inner().id).map(RequestId)?;
        let found = self
            .requests
            .find_by_id(id)
            .await
            .map_err(repo_status)?
            .ok_or_else(|| Status::not_found("request not found"))?;
        Ok(Response::new(GetRequestResponse {
            request: Some(request_record(&found)),
        }))
    }

    async fn monthly_report(
        &self,
        request: proto::tonic::Request<MonthlyReportRequest>,
    ) -> Result<Response<MonthlyReportStats>, Status> {
        let MonthlyReportRequest { year, month } = request.into_inner();
        let month =
            u8::try_from(month).map_err(|_| Status::invalid_argument("month must be 1-12"))?;
        let data = self
            .report
            .monthly_stats(year, month)
            .await
            .map_err(app_status)?;
        Ok(Response::new(monthly_stats(&data)))
    }

    async fn yearly_report(
        &self,
        request: proto::tonic::Request<YearlyReportRequest>,
    ) -> Result<Response<YearlyReportStats>, Status> {
        let year = request.into_inner().year;
        let data = self.report.yearly_stats(year).await.map_err(app_status)?;
        Ok(Response::new(yearly_stats(&data)))
    }
}

// --- conversions ---

fn project_record(p: &Project) -> ProjectRecord {
    ProjectRecord {
        id: p.id.0.to_string(),
        owner_group_id: p.owner_group_id.0.to_string(),
        created_by_user_id: p.created_by_user_id.0.to_string(),
        name: p.name.clone(),
        description: p.description.clone(),
        status: token(&p.status),
        progress: u32::from(p.progress),
        completed_at: opt_rfc3339(p.completed_at),
        created_at: rfc3339(p.created_at),
        updated_at: rfc3339(p.updated_at),
    }
}

fn request_record(r: &domain::model::Request) -> RequestRecord {
    RequestRecord {
        id: r.id.0.to_string(),
        project_id: r.project_id.0.to_string(),
        creator_user_id: r.creator_user_id.0.to_string(),
        assignee_user_id: r
            .assignee_user_id
            .map(|u| u.0.to_string())
            .unwrap_or_default(),
        title: r.title.clone(),
        description: r.description.clone(),
        status: token(&r.status),
        priority: token(&r.priority),
        progress: u32::from(r.progress),
        due_at: opt_rfc3339(r.due_at),
        completed_at: opt_rfc3339(r.completed_at),
        created_at: rfc3339(r.created_at),
        updated_at: rfc3339(r.updated_at),
    }
}

fn monthly_stats(data: &MonthlyReportData) -> MonthlyReportStats {
    MonthlyReportStats {
        period_start: rfc3339(data.period.start),
        period_end: rfc3339(data.period.end),
        groups: data.groups.iter().map(group_row).collect(),
        tickets: Some(ticket_stats(&data.tickets)),
        staff: Some(staff_summary(&data.staff)),
    }
}

fn group_row(g: &domain::model::GroupReportRow) -> proto::internal::v1::GroupReportRow {
    proto::internal::v1::GroupReportRow {
        group_id: g.group_id.0.to_string(),
        group_name: g.group_name.clone(),
        group_kind: token(&g.group_kind),
        projects_total: g.projects_total,
        projects_completed: g.projects_completed,
        projects_active: g.projects_active,
        projects_on_hold: g.projects_on_hold,
        projects_planning: g.projects_planning,
        projects_cancelled: g.projects_cancelled,
        projects_stuck: g.projects_stuck,
        avg_project_progress: u32::from(g.avg_project_progress),
        requests_total: g.requests_total,
        requests_completed: g.requests_completed,
        requests_open: g.requests_open,
        request_completion_pct: u32::from(g.request_completion_pct),
        headcount: g.headcount,
    }
}

fn ticket_stats(t: &domain::model::TicketStats) -> proto::internal::v1::TicketStats {
    proto::internal::v1::TicketStats {
        created_in_period: t.created_in_period,
        resolved_in_period: t.resolved_in_period,
        by_status: t
            .by_status
            .iter()
            .map(|(status, count)| LabelCount {
                label: token(status),
                count: *count,
            })
            .collect(),
        by_category: t
            .by_category
            .iter()
            .map(|(category, count)| LabelCount {
                label: token(category),
                count: *count,
            })
            .collect(),
        avg_resolve_hours: t.avg_resolve_hours,
    }
}

fn staff_summary(s: &domain::model::StaffSummary) -> proto::internal::v1::StaffSummary {
    proto::internal::v1::StaffSummary {
        company_headcount: s.company_headcount,
        new_joiners: s.new_joiners,
        deactivations: s.deactivations,
        per_group: s
            .per_group
            .iter()
            .map(|(id, name, headcount)| GroupHeadcount {
                group_id: id.0.to_string(),
                group_name: name.clone(),
                headcount: *headcount,
            })
            .collect(),
    }
}

fn yearly_stats(data: &YearlyReportData) -> YearlyReportStats {
    let growth = &data.growth;
    let totals = &data.totals;
    YearlyReportStats {
        year: data.year,
        growth: Some(proto::internal::v1::GrowthSeries {
            headcount: growth.headcount.iter().map(growth_point).collect(),
            new_joiners: growth.new_joiners.iter().map(growth_point).collect(),
            tickets_created: growth.tickets_created.iter().map(growth_point).collect(),
            projects_completed: growth.projects_completed.iter().map(growth_point).collect(),
            requests_completed: growth.requests_completed.iter().map(growth_point).collect(),
        }),
        totals: Some(proto::internal::v1::YearlyTotals {
            company_headcount: totals.company_headcount,
            net_headcount_change: totals.net_headcount_change,
            new_hires: totals.new_hires,
            departures: totals.departures,
            tickets_created: totals.tickets_created,
            projects_completed: totals.projects_completed,
            requests_completed: totals.requests_completed,
        }),
    }
}

fn growth_point(p: &domain::model::GrowthPoint) -> proto::internal::v1::GrowthPoint {
    proto::internal::v1::GrowthPoint {
        year: p.year,
        month: u32::from(p.month),
        value: p.value,
    }
}

// --- helpers ---

/// Serde `snake_case` wire token of a domain enum, so gRPC and REST agree.
fn token<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(Value::String(s)) => s,
        _ => String::new(),
    }
}

fn rfc3339(t: OffsetDateTime) -> String {
    t.format(&Rfc3339).unwrap_or_default()
}

fn opt_rfc3339(t: Option<OffsetDateTime>) -> String {
    t.map(rfc3339).unwrap_or_default()
}

fn parse_id(raw: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(raw).map_err(|_| Status::invalid_argument("invalid id"))
}

/// Empty string means "no cursor"; anything else must be a uuid.
fn parse_cursor(raw: &str) -> Result<Option<Uuid>, Status> {
    if raw.is_empty() {
        return Ok(None);
    }
    parse_id(raw).map(Some)
}

fn clamp_limit(limit: u32) -> u32 {
    if limit == 0 {
        DEFAULT_LIMIT
    } else {
        limit.min(MAX_LIMIT)
    }
}

/// Cursor for the next page: the last id when the page came back full.
fn next_cursor(returned: usize, limit: u32, last: Option<Uuid>) -> String {
    if returned == limit as usize {
        last.map(|id| id.to_string()).unwrap_or_default()
    } else {
        String::new()
    }
}

fn repo_status(e: RepositoryError) -> Status {
    match e {
        RepositoryError::NotFound => Status::not_found("not found"),
        other => Status::internal(other.to_string()),
    }
}

fn app_status(e: application::Error) -> Status {
    match e {
        application::Error::NotFound(what) => Status::not_found(what),
        application::Error::Validation(msg) => Status::invalid_argument(msg),
        other => Status::internal(other.to_string()),
    }
}
