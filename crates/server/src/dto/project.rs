//! Domain <-> wire projections for projects, collaborators, and invites.

use application::commands::project::{CreateProjectCommand, UpdateProjectMetadataCommand};
use domain::{ids, model};
use shared::dto::{
    common::{GroupSummaryDto, UserSummaryDto},
    project::{
        CreateProjectRequest, ProjectCollaboratorDto, ProjectDto, ProjectInviteDto,
        ProjectInviteStatus as WireProjectInviteStatus, ProjectStatus as WireProjectStatus,
        UpdateProjectMetadataRequest,
    },
};

use super::{project_collaborator_id, project_id, project_invite_id};

#[must_use]
pub fn project_status_dto(status: model::ProjectStatus) -> WireProjectStatus {
    match status {
        model::ProjectStatus::Planning => WireProjectStatus::Planning,
        model::ProjectStatus::Active => WireProjectStatus::Active,
        model::ProjectStatus::OnHold => WireProjectStatus::OnHold,
        model::ProjectStatus::Completed => WireProjectStatus::Completed,
        model::ProjectStatus::Cancelled => WireProjectStatus::Cancelled,
    }
}

#[must_use]
pub fn project_invite_status_dto(status: model::ProjectInviteStatus) -> WireProjectInviteStatus {
    match status {
        model::ProjectInviteStatus::Pending => WireProjectInviteStatus::Pending,
        model::ProjectInviteStatus::Accepted => WireProjectInviteStatus::Accepted,
        model::ProjectInviteStatus::Declined => WireProjectInviteStatus::Declined,
        model::ProjectInviteStatus::Revoked => WireProjectInviteStatus::Revoked,
    }
}

#[must_use]
pub fn project_dto(
    project: &model::Project,
    owner_group: GroupSummaryDto,
    created_by: UserSummaryDto,
) -> ProjectDto {
    ProjectDto {
        id: project_id(project.id),
        owner_group,
        created_by,
        name: project.name.clone(),
        description: project.description.clone(),
        status: project_status_dto(project.status),
        progress: project.progress,
        created_at: project.created_at,
        updated_at: project.updated_at,
    }
}

#[must_use]
pub fn project_collaborator_dto(
    collaborator: &model::ProjectCollaborator,
    group: GroupSummaryDto,
) -> ProjectCollaboratorDto {
    ProjectCollaboratorDto {
        id: project_collaborator_id(collaborator.id),
        group,
        created_at: collaborator.created_at,
    }
}

#[must_use]
pub fn project_invite_dto(
    invite: &model::ProjectInvite,
    invited_by: UserSummaryDto,
    invited_group: GroupSummaryDto,
    responded_by: Option<UserSummaryDto>,
) -> ProjectInviteDto {
    ProjectInviteDto {
        id: project_invite_id(invite.id),
        project_id: project_id(invite.project_id),
        invited_by,
        invited_group,
        responded_by,
        status: project_invite_status_dto(invite.status),
        responded_at: invite.responded_at,
        created_at: invite.created_at,
    }
}

#[must_use]
pub fn create_project_command(req: CreateProjectRequest) -> CreateProjectCommand {
    CreateProjectCommand {
        owner_group_id: ids::GroupId(req.owner_group_id.0),
        name: req.name,
        description: req.description,
    }
}

#[must_use]
pub fn update_project_metadata_command(
    req: UpdateProjectMetadataRequest,
) -> UpdateProjectMetadataCommand {
    UpdateProjectMetadataCommand {
        name: req.name,
        description: req.description,
    }
}
