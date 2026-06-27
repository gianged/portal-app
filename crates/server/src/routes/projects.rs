//! Project endpoints, plus the project-invite lifecycle under `/project-invites`.
//!
//! Note: there is no list-all-projects service method (only by owner group, via
//! `?owner_group=`). The project detail view surfaces its own pending invites.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
};
use serde::Deserialize;
use uuid::Uuid;

use domain::{
    ids::{GroupId, ProjectId, ProjectInviteId, UserId},
    model::{Project, ProjectInvite, ProjectStatus},
};
use shared::{
    dto::project::{
        ChangeProjectStatusRequest, CreateProjectRequest, InviteGroupRequest, ProjectDetailDto,
        ProjectDto, ProjectInviteDto, ProjectStatus as WireProjectStatus, RespondInviteRequest,
        SetProjectProgressRequest, UpdateProjectMetadataRequest,
    },
    validation::project::{
        validate_project_description, validate_project_name, validate_project_progress,
    },
};

use crate::{app::AppState, dto, error::AppError, extractors::auth_user::AuthUser, resolve};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects", post(create).get(list))
        .route("/projects/{id}", get(detail).patch(update))
        .route("/projects/{id}/status", post(change_status))
        .route("/projects/{id}/progress", post(set_progress))
        .route("/projects/{id}/invites", post(invite))
        .route(
            "/projects/{id}/collaborators/{group_id}",
            delete(remove_collaborator),
        )
        .route("/project-invites", get(list_invites))
        .route("/project-invites/{invite_id}/respond", post(respond))
        .route("/project-invites/{invite_id}/revoke", post(revoke))
}

#[derive(Deserialize)]
struct ListQuery {
    owner_group: Option<Uuid>,
    /// Substring search on the project name.
    q: Option<String>,
}

#[derive(Deserialize)]
struct InviteQuery {
    group: Uuid,
}

async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateProjectRequest>,
) -> Result<Json<ProjectDto>, AppError> {
    validate_project_name(&body.name).map_err(|e| AppError::Validation(e.to_string()))?;
    validate_project_description(&body.description)
        .map_err(|e| AppError::Validation(e.to_string()))?;
    let project = state
        .project
        .create_project(auth.user_id, dto::create_project_command(body))
        .await?;
    Ok(Json(project_dto(&state, &project).await?))
}

async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<ProjectDto>>, AppError> {
    let Some(group) = q.owner_group else {
        return Err(AppError::Validation(
            "owner_group query parameter is required".into(),
        ));
    };
    let search = crate::routes::norm_q(q.q);
    let projects = state
        .project
        .list_for_owner_group(auth.user_id, GroupId(group), search.as_deref())
        .await?;
    Ok(Json(project_many(&state, projects).await?))
}

async fn detail(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ProjectDetailDto>, AppError> {
    let pid = ProjectId(id);
    let project = state.project.find(auth.user_id, pid).await?;
    let collaborators = state.project.list_collaborators(auth.user_id, pid).await?;

    let gids: Vec<GroupId> = collaborators.iter().map(|c| c.group_id).collect();
    let groups = resolve::group_map(&state.group, gids).await?;
    let collaborator_dtos = collaborators
        .iter()
        .map(|c| dto::project_collaborator_dto(c, resolve::group_summary_from(&groups, c.group_id)))
        .collect();

    let invites = state
        .project
        .list_pending_invites_for_project(auth.user_id, pid)
        .await?;
    let mut pending_invites = Vec::with_capacity(invites.len());
    for invite in &invites {
        pending_invites.push(invite_dto(&state, invite).await?);
    }

    Ok(Json(ProjectDetailDto {
        project: project_dto(&state, &project).await?,
        collaborators: collaborator_dtos,
        pending_invites,
    }))
}

async fn update(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateProjectMetadataRequest>,
) -> Result<Json<ProjectDto>, AppError> {
    if let Some(name) = &body.name {
        validate_project_name(name).map_err(|e| AppError::Validation(e.to_string()))?;
    }
    if let Some(description) = &body.description {
        validate_project_description(description)
            .map_err(|e| AppError::Validation(e.to_string()))?;
    }
    let project = state
        .project
        .update_metadata(
            auth.user_id,
            ProjectId(id),
            dto::update_project_metadata_command(body),
        )
        .await?;
    Ok(Json(project_dto(&state, &project).await?))
}

async fn change_status(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ChangeProjectStatusRequest>,
) -> Result<Json<ProjectDto>, AppError> {
    let pid = ProjectId(id);
    let project = match body.status {
        // `Active` is reachable from Planning (activate) and OnHold (resume); pick by current state.
        WireProjectStatus::Active => {
            let current = state.project.find(auth.user_id, pid).await?;
            if current.status == ProjectStatus::OnHold {
                state.project.resume(auth.user_id, pid).await?
            } else {
                state.project.activate(auth.user_id, pid).await?
            }
        }
        WireProjectStatus::OnHold => state.project.hold(auth.user_id, pid).await?,
        WireProjectStatus::Completed => state.project.complete(auth.user_id, pid).await?,
        WireProjectStatus::Cancelled => state.project.cancel(auth.user_id, pid).await?,
        WireProjectStatus::Planning => {
            return Err(AppError::Validation(
                "cannot transition a project back to planning".into(),
            ));
        }
    };
    Ok(Json(project_dto(&state, &project).await?))
}

async fn set_progress(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<SetProjectProgressRequest>,
) -> Result<Json<ProjectDto>, AppError> {
    validate_project_progress(body.progress).map_err(|e| AppError::Validation(e.to_string()))?;
    let project = state
        .project
        .set_progress(auth.user_id, ProjectId(id), body.progress)
        .await?;
    Ok(Json(project_dto(&state, &project).await?))
}

async fn invite(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<InviteGroupRequest>,
) -> Result<Json<ProjectInviteDto>, AppError> {
    let invite = state
        .project
        .invite_group(auth.user_id, ProjectId(id), GroupId(body.group_id.0))
        .await?;
    Ok(Json(invite_dto(&state, &invite).await?))
}

async fn remove_collaborator(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, group_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    state
        .project
        .remove_collaborator(auth.user_id, ProjectId(id), GroupId(group_id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_invites(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<InviteQuery>,
) -> Result<Json<Vec<ProjectInviteDto>>, AppError> {
    let invites = state
        .project
        .list_pending_invites_for_group(auth.user_id, GroupId(q.group))
        .await?;
    let mut out = Vec::with_capacity(invites.len());
    for invite in &invites {
        out.push(invite_dto(&state, invite).await?);
    }
    Ok(Json(out))
}

async fn respond(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(invite_id): Path<Uuid>,
    Json(body): Json<RespondInviteRequest>,
) -> Result<Json<ProjectInviteDto>, AppError> {
    let invite = if body.accept {
        state
            .project
            .accept_invite(auth.user_id, ProjectInviteId(invite_id))
            .await?
    } else {
        state
            .project
            .decline_invite(auth.user_id, ProjectInviteId(invite_id))
            .await?
    };
    Ok(Json(invite_dto(&state, &invite).await?))
}

async fn revoke(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(invite_id): Path<Uuid>,
) -> Result<Json<ProjectInviteDto>, AppError> {
    let invite = state
        .project
        .revoke_invite(auth.user_id, ProjectInviteId(invite_id))
        .await?;
    Ok(Json(invite_dto(&state, &invite).await?))
}

/// Resolves one project's owner-group + creator summaries.
async fn project_dto(state: &AppState, project: &Project) -> Result<ProjectDto, AppError> {
    let owner = resolve::group_summary(&state.group, project.owner_group_id).await?;
    let creator =
        resolve::user_summary(&state.user, &state.group, project.created_by_user_id).await?;
    Ok(dto::project_dto(project, owner, creator))
}

/// Resolves a batch of projects, deduplicating group + user lookups.
async fn project_many(
    state: &AppState,
    projects: Vec<Project>,
) -> Result<Vec<ProjectDto>, AppError> {
    let mut gids: Vec<GroupId> = Vec::with_capacity(projects.len());
    let mut uids: Vec<UserId> = Vec::with_capacity(projects.len());
    for p in &projects {
        gids.push(p.owner_group_id);
        uids.push(p.created_by_user_id);
    }
    let groups = resolve::group_map(&state.group, gids).await?;
    let users = resolve::user_map(&state.user, &state.group, uids).await?;
    Ok(projects
        .iter()
        .map(|p| {
            dto::project_dto(
                p,
                resolve::group_summary_from(&groups, p.owner_group_id),
                resolve::summary_from(&users, p.created_by_user_id),
            )
        })
        .collect())
}

/// Resolves an invite's invited-by, invited-group, and responder summaries.
async fn invite_dto(
    state: &AppState,
    invite: &ProjectInvite,
) -> Result<ProjectInviteDto, AppError> {
    let invited_by =
        resolve::user_summary(&state.user, &state.group, invite.invited_by_user_id).await?;
    let invited_group = resolve::group_summary(&state.group, invite.invited_group_id).await?;
    let responded_by =
        resolve::opt_user_summary(&state.user, &state.group, invite.responded_by_user_id).await?;
    Ok(dto::project_invite_dto(
        invite,
        invited_by,
        invited_group,
        responded_by,
    ))
}
