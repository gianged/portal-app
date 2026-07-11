//! Group + membership administration. Most mutations are HR-gated in the
//! service; the roster read is gated to members / HR / Directors. The group
//! directory (`GET /groups`) is open to any active user.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing,
};

use domain::ids::{GroupId, UserId};
use shared::dto::{
    group::{
        AddMemberRequest, ChangeMemberRoleRequest, CreateGroupRequest, GroupDetailDto, GroupDto,
        MembershipDto, TransferLeadershipRequest, UpdateGroupRequest,
    },
    ids as wire,
};

use crate::{
    app::AppState,
    dto,
    error::AppError,
    extractors::{app_json::AppJson, auth_user::AuthUser, validated_json::ValidatedJson},
    resolve,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/groups", routing::post(create).get(list))
        .route("/groups/{id}", routing::get(detail).patch(update))
        .route("/groups/{id}/members", routing::post(add_member))
        .route(
            "/groups/{id}/members/{user_id}",
            routing::patch(change_role).delete(remove_member),
        )
        .route(
            "/groups/{id}/transfer-leadership",
            routing::post(transfer_leadership),
        )
}

async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedJson(body): ValidatedJson<CreateGroupRequest>,
) -> Result<Json<GroupDto>, AppError> {
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
    // Member counts resolved per group (N reads); fine at this scale.
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
    Path(id): Path<wire::GroupId>,
    ValidatedJson(body): ValidatedJson<UpdateGroupRequest>,
) -> Result<Json<GroupDto>, AppError> {
    let group = state
        .group
        .update_metadata(
            auth.user_id,
            GroupId(id.0),
            dto::update_group_metadata_command(body),
        )
        .await?;
    let count = state.group.active_member_count(group.id).await?;
    Ok(Json(dto::group_dto(&group, count)))
}

async fn detail(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<wire::GroupId>,
) -> Result<Json<GroupDetailDto>, AppError> {
    let gid = GroupId(id.0);
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
    Path(id): Path<wire::GroupId>,
    AppJson(body): AppJson<AddMemberRequest>,
) -> Result<Json<MembershipDto>, AppError> {
    let membership = state
        .group
        .add_membership(
            auth.user_id,
            dto::add_membership_command(GroupId(id.0), &body),
        )
        .await?;
    let user = resolve::user_summary(&state.user, &state.group, membership.user_id).await?;
    Ok(Json(dto::membership_dto(&membership, user)))
}

async fn change_role(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, user_id)): Path<(wire::GroupId, wire::UserId)>,
    AppJson(body): AppJson<ChangeMemberRoleRequest>,
) -> Result<Json<MembershipDto>, AppError> {
    let membership = state
        .group
        .change_role(
            auth.user_id,
            GroupId(id.0),
            UserId(user_id.0),
            dto::group_role_domain(body.role),
        )
        .await?;
    let user = resolve::user_summary(&state.user, &state.group, membership.user_id).await?;
    Ok(Json(dto::membership_dto(&membership, user)))
}

async fn remove_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, user_id)): Path<(wire::GroupId, wire::UserId)>,
) -> Result<StatusCode, AppError> {
    state
        .group
        .deactivate_membership(auth.user_id, GroupId(id.0), UserId(user_id.0))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn transfer_leadership(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<wire::GroupId>,
    AppJson(body): AppJson<TransferLeadershipRequest>,
) -> Result<StatusCode, AppError> {
    if body.from_user_id == body.to_user_id {
        return Err(AppError::Validation(
            "cannot transfer leadership to the same user".into(),
        ));
    }
    state
        .group
        .transfer_leadership(
            auth.user_id,
            GroupId(id.0),
            UserId(body.from_user_id.0),
            UserId(body.to_user_id.0),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
