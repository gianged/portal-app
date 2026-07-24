use async_trait::async_trait;

use crate::{
    error::RepositoryError,
    ids::{ProjectId, RequestId, UserId},
    model::{Request, RequestAttachment, RequestStatus},
    repository::OutboxRecord,
};

#[async_trait]
pub trait RequestRepository: Send + Sync {
    async fn find_by_id(&self, id: RequestId) -> Result<Option<Request>, RepositoryError>;

    /// `q` is a case-insensitive substring filter on the request title.
    async fn list_for_project(
        &self,
        project_id: ProjectId,
        status: Option<RequestStatus>,
        q: Option<&str>,
    ) -> Result<Vec<Request>, RepositoryError>;

    /// `q` is a case-insensitive substring filter on the request title.
    async fn list_for_assignee(
        &self,
        assignee: UserId,
        status: Option<RequestStatus>,
        q: Option<&str>,
    ) -> Result<Vec<Request>, RepositoryError>;

    /// Keyset page ordered by id ascending; `after` is exclusive and `project`
    /// optionally filters. Backs the internal query plane and the external
    /// read API.
    async fn list_page(
        &self,
        project: Option<ProjectId>,
        after: Option<RequestId>,
        limit: u32,
    ) -> Result<Vec<Request>, RepositoryError>;

    /// `outbox` rows commit in the same transaction as the entity write, so an
    /// audited event cannot be lost between commit and projection.
    async fn save(&self, request: &Request, outbox: &[OutboxRecord])
    -> Result<(), RepositoryError>;

    async fn list_attachments(
        &self,
        request_id: RequestId,
    ) -> Result<Vec<RequestAttachment>, RepositoryError>;

    async fn save_attachment(&self, attachment: &RequestAttachment) -> Result<(), RepositoryError>;

    /// Every attachment's storage key across all requests. Backs the upload
    /// orphan-sweep job.
    async fn list_all_attachment_keys(&self) -> Result<Vec<String>, RepositoryError>;
}
