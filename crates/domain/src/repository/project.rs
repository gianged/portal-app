use async_trait::async_trait;

use crate::{
    error::RepositoryError,
    ids::{GroupId, ProjectCollaboratorId, ProjectId, ProjectInviteId},
    model::{Project, ProjectCollaborator, ProjectInvite},
    repository::OutboxRecord,
};

#[async_trait]
pub trait ProjectRepository: Send + Sync {
    async fn find_by_id(&self, id: ProjectId) -> Result<Option<Project>, RepositoryError>;

    /// `q` is a case-insensitive substring filter on the project name.
    async fn list_for_owner_group(
        &self,
        group_id: GroupId,
        q: Option<&str>,
    ) -> Result<Vec<Project>, RepositoryError>;

    async fn list_for_collaborator_group(
        &self,
        group_id: GroupId,
    ) -> Result<Vec<Project>, RepositoryError>;

    /// Keyset page over every project, ordered by id ascending; `after` is
    /// exclusive. Backs the internal query plane and the external read API.
    async fn list_page(
        &self,
        after: Option<ProjectId>,
        limit: u32,
    ) -> Result<Vec<Project>, RepositoryError>;

    /// `outbox` rows commit in the same transaction as the entity write, so an
    /// audited event cannot be lost between commit and projection.
    async fn save_project(
        &self,
        project: &Project,
        outbox: &[OutboxRecord],
    ) -> Result<(), RepositoryError>;

    async fn list_collaborators(
        &self,
        project_id: ProjectId,
    ) -> Result<Vec<ProjectCollaborator>, RepositoryError>;

    async fn save_collaborator(
        &self,
        collaborator: &ProjectCollaborator,
    ) -> Result<(), RepositoryError>;

    /// `outbox` rows commit in the same transaction as the entity write, so an
    /// audited event cannot be lost between commit and projection.
    async fn delete_collaborator(
        &self,
        id: ProjectCollaboratorId,
        outbox: &[OutboxRecord],
    ) -> Result<(), RepositoryError>;

    async fn find_invite(
        &self,
        id: ProjectInviteId,
    ) -> Result<Option<ProjectInvite>, RepositoryError>;

    async fn list_pending_invites_for_group(
        &self,
        group_id: GroupId,
    ) -> Result<Vec<ProjectInvite>, RepositoryError>;

    async fn list_pending_invites_for_project(
        &self,
        project_id: ProjectId,
    ) -> Result<Vec<ProjectInvite>, RepositoryError>;

    /// `outbox` rows commit in the same transaction as the entity write, so an
    /// audited event cannot be lost between commit and projection.
    async fn save_invite(
        &self,
        invite: &ProjectInvite,
        outbox: &[OutboxRecord],
    ) -> Result<(), RepositoryError>;
}
