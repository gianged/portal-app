//! Project + project-invite HTTP wrappers. Projects are listed per owning group
//! (there is no global list); invites are listed per group.

use shared::dto::ids::{GroupId, ProjectId, ProjectInviteId};
use shared::dto::project::{
    ChangeProjectStatusRequest, CreateProjectRequest, InviteGroupRequest, ProjectDetailDto,
    ProjectDto, ProjectInviteDto, RespondInviteRequest, UpdateProjectMetadataRequest,
};

use crate::api::client;
use crate::api::error::FrontendError;

/// Projects owned by a group (`GET /projects?owner_group=…`).
pub async fn list_for_owner_group(group: GroupId) -> Result<Vec<ProjectDto>, FrontendError> {
    let gid = group.0.to_string();
    let q = client::query(&[("owner_group", &gid)]);
    client::get_json(&format!("/projects{q}")).await
}

/// Project header + collaborators + pending invites (`GET /projects/{id}`).
pub async fn get(id: ProjectId) -> Result<ProjectDetailDto, FrontendError> {
    client::get_json(&format!("/projects/{}", id.0)).await
}

pub async fn create(req: &CreateProjectRequest) -> Result<ProjectDto, FrontendError> {
    client::post_json("/projects", req).await
}

pub async fn update(
    id: ProjectId,
    req: &UpdateProjectMetadataRequest,
) -> Result<ProjectDto, FrontendError> {
    client::patch_json(&format!("/projects/{}", id.0), req).await
}

pub async fn change_status(
    id: ProjectId,
    req: &ChangeProjectStatusRequest,
) -> Result<ProjectDto, FrontendError> {
    client::post_json(&format!("/projects/{}/status", id.0), req).await
}

pub async fn invite_group(
    id: ProjectId,
    req: &InviteGroupRequest,
) -> Result<ProjectInviteDto, FrontendError> {
    client::post_json(&format!("/projects/{}/invites", id.0), req).await
}

pub async fn remove_collaborator(
    project: ProjectId,
    group: GroupId,
) -> Result<(), FrontendError> {
    client::del(&format!(
        "/projects/{}/collaborators/{}",
        project.0, group.0
    ))
    .await
}

/// Pending invites addressed to a group (`GET /project-invites?group=…`).
pub async fn list_invites_for_group(
    group: GroupId,
) -> Result<Vec<ProjectInviteDto>, FrontendError> {
    let gid = group.0.to_string();
    let q = client::query(&[("group", &gid)]);
    client::get_json(&format!("/project-invites{q}")).await
}

pub async fn respond_invite(
    invite: ProjectInviteId,
    req: &RespondInviteRequest,
) -> Result<ProjectInviteDto, FrontendError> {
    client::post_json(&format!("/project-invites/{}/respond", invite.0), req).await
}

pub async fn revoke_invite(invite: ProjectInviteId) -> Result<ProjectInviteDto, FrontendError> {
    client::post_empty(&format!("/project-invites/{}/revoke", invite.0)).await
}
