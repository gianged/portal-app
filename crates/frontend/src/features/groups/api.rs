//! Group + membership HTTP wrappers. The directory (`list`) backs group pickers
//! across features; the rest drive the group detail / roster admin page.

use serde::Serialize;

use shared::dto::group::{
    AddMemberRequest, ChangeMemberRoleRequest, CreateGroupRequest, GroupDetailDto, GroupDto,
    MembershipDto, UpdateGroupRequest,
};
use shared::dto::ids::{GroupId, UserId};

use crate::api::client;
use crate::api::error::FrontendError;

/// All groups in the org (`GET /groups`).
pub async fn list() -> Result<Vec<GroupDto>, FrontendError> {
    client::get_json("/groups").await
}

/// Group header + roster (`GET /groups/{id}`).
pub async fn get(id: GroupId) -> Result<GroupDetailDto, FrontendError> {
    client::get_json(&format!("/groups/{}", id.0)).await
}

pub async fn create(req: &CreateGroupRequest) -> Result<GroupDto, FrontendError> {
    client::post_json("/groups", req).await
}

pub async fn update(id: GroupId, req: &UpdateGroupRequest) -> Result<GroupDto, FrontendError> {
    client::patch_json(&format!("/groups/{}", id.0), req).await
}

pub async fn add_member(
    id: GroupId,
    req: &AddMemberRequest,
) -> Result<MembershipDto, FrontendError> {
    client::post_json(&format!("/groups/{}/members", id.0), req).await
}

pub async fn change_role(
    group: GroupId,
    user: UserId,
    req: &ChangeMemberRoleRequest,
) -> Result<MembershipDto, FrontendError> {
    client::patch_json(&format!("/groups/{}/members/{}", group.0, user.0), req).await
}

pub async fn remove_member(group: GroupId, user: UserId) -> Result<(), FrontendError> {
    client::del(&format!("/groups/{}/members/{}", group.0, user.0)).await
}

/// `POST /groups/{id}/transfer-leadership` — the body shape is server-local (not
/// in `shared::dto`), so it is declared here.
#[derive(Serialize)]
struct TransferLeadership {
    from_user_id: UserId,
    to_user_id: UserId,
}

pub async fn transfer_leadership(
    group: GroupId,
    from: UserId,
    to: UserId,
) -> Result<(), FrontendError> {
    let body = TransferLeadership {
        from_user_id: from,
        to_user_id: to,
    };
    client::post_json_no_content(&format!("/groups/{}/transfer-leadership", group.0), &body).await
}
