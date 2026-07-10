use std::sync::Arc;

use domain::{
    ids::{ProjectId, RequestId, ServiceAccountId},
    model::{MonthlyReportData, Project, Request, YearlyReportData},
};

use crate::{
    error::Result,
    permissions::{Permissions, ServiceAccountScope},
    service::{Page, ReadPlaneService},
};

/// Read-only queries backing the external `/api/ext/v1` surface. Every method
/// gates on the calling service account's granted scope, then delegates to the
/// shared [`ReadPlaneService`]. Strictly no mutations.
pub struct ExtReadService {
    read: Arc<ReadPlaneService>,
    perms: Arc<Permissions>,
}

impl ExtReadService {
    #[must_use]
    pub fn new(read: Arc<ReadPlaneService>, perms: Arc<Permissions>) -> Self {
        Self { read, perms }
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
    ) -> Result<Page<Project>> {
        self.perms
            .require_service_account_scope(account, ServiceAccountScope::Projects)
            .await?;
        self.read.list_projects(after, limit).await
    }

    /// # Errors
    /// Returns `Forbidden` without the `Projects` scope, `NotFound` for an
    /// unknown id, or a repository error.
    #[tracing::instrument(skip_all, fields(account = ?account, id = ?id))]
    pub async fn get_project(&self, account: ServiceAccountId, id: ProjectId) -> Result<Project> {
        self.perms
            .require_service_account_scope(account, ServiceAccountScope::Projects)
            .await?;
        self.read.get_project(id).await
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
    ) -> Result<Page<Request>> {
        self.perms
            .require_service_account_scope(account, ServiceAccountScope::Requests)
            .await?;
        self.read.list_requests(project, after, limit).await
    }

    /// # Errors
    /// Returns `Forbidden` without the `Requests` scope, `NotFound` for an
    /// unknown id, or a repository error.
    #[tracing::instrument(skip_all, fields(account = ?account, id = ?id))]
    pub async fn get_request(&self, account: ServiceAccountId, id: RequestId) -> Result<Request> {
        self.perms
            .require_service_account_scope(account, ServiceAccountScope::Requests)
            .await?;
        self.read.get_request(id).await
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
        self.read.monthly_report(year, month).await
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
        self.read.yearly_report(year).await
    }
}
