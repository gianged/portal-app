use async_trait::async_trait;

use crate::{
    error::RepositoryError,
    ids::{GroupId, ProjectCollaboratorId, ProjectId, ProjectInviteId},
    model::{Project, ProjectCollaborator, ProjectInvite},
};

#[async_trait]
pub trait ProjectRepository: Send + Sync {
    async fn find_by_id(&self, id: ProjectId) -> Result<Option<Project>, RepositoryError>;

    async fn list_for_owner_group(
        &self,
        group_id: GroupId,
    ) -> Result<Vec<Project>, RepositoryError>;

    async fn list_for_collaborator_group(
        &self,
        group_id: GroupId,
    ) -> Result<Vec<Project>, RepositoryError>;

    async fn save_project(&self, project: &Project) -> Result<(), RepositoryError>;

    async fn list_collaborators(
        &self,
        project_id: ProjectId,
    ) -> Result<Vec<ProjectCollaborator>, RepositoryError>;

    async fn save_collaborator(
        &self,
        collaborator: &ProjectCollaborator,
    ) -> Result<(), RepositoryError>;

    async fn delete_collaborator(&self, id: ProjectCollaboratorId) -> Result<(), RepositoryError>;

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

    async fn save_invite(&self, invite: &ProjectInvite) -> Result<(), RepositoryError>;
}
