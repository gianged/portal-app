//! Group + membership administration. Most mutations are HR-gated in the
//! service; the roster read is gated to members / HR / Directors. The group
//! directory (`GET /groups`) is open to any active user.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch, post},
};
use serde::Deserialize;
use uuid::Uuid;

use domain::ids::{GroupId, UserId};
use shared::dto::group::{
    AddMemberRequest, ChangeMemberRoleRequest, CreateGroupRequest, GroupDetailDto, GroupDto,
    MembershipDto, UpdateGroupRequest,
};
use shared::validation::group::{validate_group_description, validate_group_name};

use crate::{app::AppState, dto, error::AppError, extractors::auth_user::AuthUser, resolve};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/groups", post(create).get(list))
        .route("/groups/{id}", get(detail).patch(update))
        .route("/groups/{id}/members", post(add_member))
        .route(
            "/groups/{id}/members/{user_id}",
            patch(change_role).delete(remove_member),
        )
        .route(
            "/groups/{id}/transfer-leadership",
            post(transfer_leadership),
        )
}

async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateGroupRequest>,
) -> Result<Json<GroupDto>, AppError> {
    validate_group_name(&body.name).map_err(|e| AppError::Validation(e.to_string()))?;
    validate_group_description(&body.description)
        .map_err(|e| AppError::Validation(e.to_string()))?;
    let group = state
        .group
        .create_group(auth.user_id, dto::create_group_command(body))
        .await?;
    Ok(Json(dto::group_dto(&group, 0)))
}

async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<GroupDto>>, AppError> {
    let groups = state.group.list_all(auth.user_id).await?;
    // Member counts are resolved per group (N reads); fine at this scale, and
    // consistent with the other denormalized list endpoints.
    let mut out = Vec::with_capacity(groups.len());
    for group in &groups {
        let count = state.group.active_member_count(group.id).await?;
        out.push(dto::group_dto(group, count));
    }
    Ok(Json(out))
}

async fn update(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateGroupRequest>,
) -> Result<Json<GroupDto>, AppError> {
    if let Some(name) = &body.name {
        validate_group_name(name).map_err(|e| AppError::Validation(e.to_string()))?;
    }
    if let Some(description) = &body.description {
        validate_group_description(description).map_err(|e| AppError::Validation(e.to_string()))?;
    }
    let group = state
        .group
        .update_metadata(
            auth.user_id,
            GroupId(id),
            dto::update_group_metadata_command(body),
        )
        .await?;
    let count = state.group.active_member_count(group.id).await?;
    Ok(Json(dto::group_dto(&group, count)))
}

async fn detail(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<GroupDetailDto>, AppError> {
    let gid = GroupId(id);
    let group = state
        .group
        .find(gid)
        .await?
        .ok_or(application::Error::NotFound("group"))?;
    let memberships = state.group.list_memberships(auth.user_id, gid).await?;

    let ids: Vec<UserId> = memberships.iter().map(|m| m.user_id).collect();
    let users = resolve::user_map(&state.user, &state.group, ids).await?;
    let active_count = u32::try_from(
        memberships
            .iter()
            .filter(|m| m.deactivated_at.is_none())
            .count(),
    )
    .unwrap_or(u32::MAX);
    let members: Vec<MembershipDto> = memberships
        .iter()
        .map(|m| dto::membership_dto(m, resolve::summary_from(&users, m.user_id)))
        .collect();
    Ok(Json(GroupDetailDto {
        group: dto::group_dto(&group, active_count),
        members,
    }))
}

async fn add_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AddMemberRequest>,
) -> Result<Json<MembershipDto>, AppError> {
    let membership = state
        .group
        .add_membership(
            auth.user_id,
            dto::add_membership_command(GroupId(id), &body),
        )
        .await?;
    let user = resolve::user_summary(&state.user, &state.group, membership.user_id).await?;
    Ok(Json(dto::membership_dto(&membership, user)))
}

async fn change_role(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, user_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<ChangeMemberRoleRequest>,
) -> Result<Json<MembershipDto>, AppError> {
    let membership = state
        .group
        .change_role(
            auth.user_id,
            GroupId(id),
            UserId(user_id),
            dto::group_role_domain(body.role),
        )
        .await?;
    let user = resolve::user_summary(&state.user, &state.group, membership.user_id).await?;
    Ok(Json(dto::membership_dto(&membership, user)))
}

async fn remove_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    state
        .group
        .deactivate_membership(auth.user_id, GroupId(id), UserId(user_id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Server-local until the frontend consumes it; promote to `shared::dto::group`
/// when wired into the UI.
#[derive(Deserialize)]
struct TransferLeadershipRequest {
    from_user_id: Uuid,
    to_user_id: Uuid,
}

async fn transfer_leadership(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<TransferLeadershipRequest>,
) -> Result<StatusCode, AppError> {
    state
        .group
        .transfer_leadership(
            auth.user_id,
            GroupId(id),
            UserId(body.from_user_id),
            UserId(body.to_user_id),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
