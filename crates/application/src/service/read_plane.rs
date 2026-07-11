use std::sync::Arc;

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

/// Keyset identity of a listable entity: which id newtype cursors a page of
/// `Self`, so a request cursor cannot be fed into a project listing.
pub trait Keyed {
    type Id: Copy;
    fn key(&self) -> Self::Id;
}

impl Keyed for Project {
    type Id = ProjectId;
    fn key(&self) -> ProjectId {
        self.id
    }
}

impl Keyed for Request {
    type Id = RequestId;
    fn key(&self) -> RequestId {
        self.id
    }
}

/// One keyset page; `next_cursor` is the last item's id when the page came
/// back full, i.e. more rows may remain.
pub struct Page<T: Keyed> {
    pub items: Vec<T>,
    pub next_cursor: Option<T::Id>,
}

fn page<T: Keyed>(items: Vec<T>, limit: u32) -> Page<T> {
    let next_cursor = if items.len() == limit as usize {
        items.last().map(Keyed::key)
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
        Ok(page(rows, limit))
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
        Ok(page(rows, limit))
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
        self.report.monthly_stats_unscoped(year, month).await
    }

    /// # Errors
    /// Returns `Validation` for an invalid year, or a repository error.
    #[tracing::instrument(skip_all, fields(year))]
    pub async fn yearly_report(&self, year: i32) -> Result<YearlyReportData> {
        self.report.yearly_stats_unscoped(year).await
    }
}
