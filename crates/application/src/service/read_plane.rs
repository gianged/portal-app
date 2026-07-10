use std::sync::Arc;

use uuid::Uuid;

use domain::{
    ids::{ProjectId, RequestId},
    model::{MonthlyReportData, Project, Request, YearlyReportData},
    repository::{ProjectRepository, RequestRepository},
};

use crate::{
    error::{Error, Result},
    service::ReportService,
};

const DEFAULT_LIMIT: u32 = 100;
const MAX_LIMIT: u32 = 500;

/// One keyset page; `next_cursor` is the last item's id when the page came
/// back full, i.e. more rows may remain.
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<Uuid>,
}

fn page<T>(items: Vec<T>, limit: u32, id_of: impl Fn(&T) -> Uuid) -> Page<T> {
    let next_cursor = if items.len() == limit as usize {
        items.last().map(id_of)
    } else {
        None
    };
    Page { items, next_cursor }
}

fn clamp_limit(limit: Option<u32>) -> u32 {
    match limit {
        None | Some(0) => DEFAULT_LIMIT,
        Some(n) => n.min(MAX_LIMIT),
    }
}

/// Un-scoped reads over projects, requests, and report aggregates: the shared
/// core behind the internal gRPC query plane (token-gated listener) and
/// [`ExtReadService`](crate::service::ExtReadService) (per-account scopes).
/// Strictly no mutations.
pub struct ReadPlaneService {
    projects: Arc<dyn ProjectRepository>,
    requests: Arc<dyn RequestRepository>,
    report: Arc<ReportService>,
}

impl ReadPlaneService {
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

    /// Keyset page over every project; `after` is the previous page's last id.
    ///
    /// # Errors
    /// Returns a repository error.
    #[tracing::instrument(skip_all)]
    pub async fn list_projects(
        &self,
        after: Option<ProjectId>,
        limit: Option<u32>,
    ) -> Result<Page<Project>> {
        let limit = clamp_limit(limit);
        let rows = self.projects.list_page(after, limit).await?;
        Ok(page(rows, limit, |p| p.id.0))
    }

    /// # Errors
    /// Returns `NotFound` for an unknown id, or a repository error.
    #[tracing::instrument(skip_all, fields(id = ?id))]
    pub async fn get_project(&self, id: ProjectId) -> Result<Project> {
        self.projects
            .find_by_id(id)
            .await?
            .ok_or(Error::NotFound("project"))
    }

    /// Keyset page over requests, optionally filtered to one project.
    ///
    /// # Errors
    /// Returns a repository error.
    #[tracing::instrument(skip_all)]
    pub async fn list_requests(
        &self,
        project: Option<ProjectId>,
        after: Option<RequestId>,
        limit: Option<u32>,
    ) -> Result<Page<Request>> {
        let limit = clamp_limit(limit);
        let rows = self.requests.list_page(project, after, limit).await?;
        Ok(page(rows, limit, |r| r.id.0))
    }

    /// # Errors
    /// Returns `NotFound` for an unknown id, or a repository error.
    #[tracing::instrument(skip_all, fields(id = ?id))]
    pub async fn get_request(&self, id: RequestId) -> Result<Request> {
        self.requests
            .find_by_id(id)
            .await?
            .ok_or(Error::NotFound("request"))
    }

    /// # Errors
    /// Returns `Validation` for an invalid month, or a repository error.
    #[tracing::instrument(skip_all, fields(year, month))]
    pub async fn monthly_report(&self, year: i32, month: u8) -> Result<MonthlyReportData> {
        self.report.monthly_stats(year, month).await
    }

    /// # Errors
    /// Returns `Validation` for an invalid year, or a repository error.
    #[tracing::instrument(skip_all, fields(year))]
    pub async fn yearly_report(&self, year: i32) -> Result<YearlyReportData> {
        self.report.yearly_stats(year).await
    }
}
