use std::sync::Arc;

use uuid::Uuid;

use domain::{
    ids::{ProjectId, RequestId, ServiceAccountId},
    model::{MonthlyReportData, Project, Request, YearlyReportData},
    repository::{ProjectRepository, RequestRepository},
};

use crate::{
    error::{Error, Result},
    permissions::{Permissions, ServiceAccountScope},
    service::ReportService,
};

const DEFAULT_LIMIT: u32 = 100;
const MAX_LIMIT: u32 = 500;

/// One keyset page; `next_cursor` is the last item's id when the page came
/// back full, i.e. more rows may remain.
pub struct ExtPage<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<Uuid>,
}

fn page<T>(items: Vec<T>, limit: u32, id_of: impl Fn(&T) -> Uuid) -> ExtPage<T> {
    let next_cursor = if items.len() == limit as usize {
        items.last().map(id_of)
    } else {
        None
    };
    ExtPage { items, next_cursor }
}

/// Read-only queries backing the external `/api/ext/v1` surface. Every method
/// gates on the calling service account's granted scope, then reads via the
/// repositories / report service. Strictly no mutations.
pub struct ExtReadService {
    projects: Arc<dyn ProjectRepository>,
    requests: Arc<dyn RequestRepository>,
    report: Arc<ReportService>,
    perms: Arc<Permissions>,
}

impl ExtReadService {
    #[must_use]
    pub fn new(
        projects: Arc<dyn ProjectRepository>,
        requests: Arc<dyn RequestRepository>,
        report: Arc<ReportService>,
        perms: Arc<Permissions>,
    ) -> Self {
        Self {
            projects,
            requests,
            report,
            perms,
        }
    }

    /// Keyset page over every project; `after` is the previous page's last id.
    ///
    /// # Errors
    /// Returns `Forbidden` without the `Projects` scope, or a repository error.
    #[tracing::instrument(skip_all, fields(account = ?account))]
    pub async fn list_projects(
        &self,
        account: ServiceAccountId,
        after: Option<ProjectId>,
        limit: Option<u32>,
    ) -> Result<ExtPage<Project>> {
        self.perms
            .require_service_account_scope(account, ServiceAccountScope::Projects)
            .await?;
        let limit = clamp_limit(limit);
        let rows = self.projects.list_page(after, limit).await?;
        Ok(page(rows, limit, |p| p.id.0))
    }

    /// # Errors
    /// Returns `Forbidden` without the `Projects` scope, `NotFound` for an
    /// unknown id, or a repository error.
    #[tracing::instrument(skip_all, fields(account = ?account, id = ?id))]
    pub async fn get_project(&self, account: ServiceAccountId, id: ProjectId) -> Result<Project> {
        self.perms
            .require_service_account_scope(account, ServiceAccountScope::Projects)
            .await?;
        self.projects
            .find_by_id(id)
            .await?
            .ok_or(Error::NotFound("project"))
    }

    /// Keyset page over requests, optionally filtered to one project.
    ///
    /// # Errors
    /// Returns `Forbidden` without the `Requests` scope, or a repository error.
    #[tracing::instrument(skip_all, fields(account = ?account))]
    pub async fn list_requests(
        &self,
        account: ServiceAccountId,
        project: Option<ProjectId>,
        after: Option<RequestId>,
        limit: Option<u32>,
    ) -> Result<ExtPage<Request>> {
        self.perms
            .require_service_account_scope(account, ServiceAccountScope::Requests)
            .await?;
        let limit = clamp_limit(limit);
        let rows = self.requests.list_page(project, after, limit).await?;
        Ok(page(rows, limit, |r| r.id.0))
    }

    /// # Errors
    /// Returns `Forbidden` without the `Requests` scope, `NotFound` for an
    /// unknown id, or a repository error.
    #[tracing::instrument(skip_all, fields(account = ?account, id = ?id))]
    pub async fn get_request(&self, account: ServiceAccountId, id: RequestId) -> Result<Request> {
        self.perms
            .require_service_account_scope(account, ServiceAccountScope::Requests)
            .await?;
        self.requests
            .find_by_id(id)
            .await?
            .ok_or(Error::NotFound("request"))
    }

    /// # Errors
    /// Returns `Forbidden` without the `Reports` scope, `Validation` for an
    /// invalid month, or a repository error.
    #[tracing::instrument(skip_all, fields(account = ?account, year, month))]
    pub async fn monthly_report(
        &self,
        account: ServiceAccountId,
        year: i32,
        month: u8,
    ) -> Result<MonthlyReportData> {
        self.perms
            .require_service_account_scope(account, ServiceAccountScope::Reports)
            .await?;
        self.report.monthly_stats(year, month).await
    }

    /// # Errors
    /// Returns `Forbidden` without the `Reports` scope, `Validation` for an
    /// invalid year, or a repository error.
    #[tracing::instrument(skip_all, fields(account = ?account, year))]
    pub async fn yearly_report(
        &self,
        account: ServiceAccountId,
        year: i32,
    ) -> Result<YearlyReportData> {
        self.perms
            .require_service_account_scope(account, ServiceAccountScope::Reports)
            .await?;
        self.report.yearly_stats(year).await
    }
}

fn clamp_limit(limit: Option<u32>) -> u32 {
    match limit {
        None | Some(0) => DEFAULT_LIMIT,
        Some(n) => n.min(MAX_LIMIT),
    }
}
