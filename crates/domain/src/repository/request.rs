use async_trait::async_trait;

use crate::{
    error::RepositoryError,
    ids::{ProjectId, RequestId, UserId},
    model::{Request, RequestAttachment, RequestStatus},
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

    async fn save(&self, request: &Request) -> Result<(), RepositoryError>;

    async fn list_attachments(
        &self,
        request_id: RequestId,
    ) -> Result<Vec<RequestAttachment>, RepositoryError>;

    async fn save_attachment(&self, attachment: &RequestAttachment) -> Result<(), RepositoryError>;

    /// Every attachment's storage key across all requests. Backs the upload
    /// orphan-sweep job.
    async fn list_all_attachment_keys(&self) -> Result<Vec<String>, RepositoryError>;
}
