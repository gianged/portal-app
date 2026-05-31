//! Async resolution of denormalized references for wire DTOs.
//!
//! Several DTOs embed `UserSummaryDto`, but domain models carry only ids. These
//! helpers fetch and project the referenced users. List endpoints dedupe via a
//! map — an N-query resolution that is fine at this scale; replace it with a
//! repository-side join if ticket/request volume grows.

use std::collections::{HashMap, HashSet};

use application::{GroupService, UserService};
use domain::{
    ids::{GroupId, UserId},
    model::{Group, GroupRole, Membership, User},
};
use shared::dto::{
    common::{GroupSummaryDto, UserSummaryDto},
    user::UserRole,
};

use crate::{dto, error::AppError};

/// Synthetic display role for a user from its active memberships + the IT group.
fn compute_role(user: &User, memberships: &[Membership], it_group: Option<GroupId>) -> UserRole {
    let roles: Vec<GroupRole> = memberships.iter().map(|m| m.role).collect();
    let is_it = it_group.is_some_and(|it| memberships.iter().any(|m| m.group_id == it));
    dto::resolve_user_role(user.system_role, &roles, is_it)
}

/// Fetches one user and projects a summary with its resolved display role,
/// falling back to a placeholder for a dangling reference.
pub async fn user_summary(
    users: &UserService,
    groups: &GroupService,
    id: UserId,
) -> Result<UserSummaryDto, AppError> {
    Ok(match users.find(id).await? {
        Some(user) => {
            let role = role_for_user(groups, &user).await?;
            dto::user_summary_dto(&user, role)
        }
        None => dto::unknown_user_summary(id),
    })
}

/// `user_summary` for an optional id (assignee, etc.).
pub async fn opt_user_summary(
    users: &UserService,
    groups: &GroupService,
    id: Option<UserId>,
) -> Result<Option<UserSummaryDto>, AppError> {
    match id {
        Some(id) => Ok(Some(user_summary(users, groups, id).await?)),
        None => Ok(None),
    }
}

/// Loads a deduplicated map of resolved user summaries for a batch of ids. Roles
/// come from one batched membership fetch plus one IT-group lookup.
pub async fn user_map(
    users: &UserService,
    groups: &GroupService,
    ids: impl IntoIterator<Item = UserId>,
) -> Result<HashMap<UserId, UserSummaryDto>, AppError> {
    let unique: HashSet<UserId> = ids.into_iter().collect();
    let id_vec: Vec<UserId> = unique.iter().copied().collect();
    let memberships = groups.active_memberships_for_users(&id_vec).await?;
    let it_group = groups.it_group_id().await?;
    let mut map = HashMap::with_capacity(unique.len());
    for id in unique {
        if let Some(user) = users.find(id).await? {
            let m = memberships.get(&id).map_or(&[][..], Vec::as_slice);
            map.insert(
                id,
                dto::user_summary_dto(&user, compute_role(&user, m, it_group)),
            );
        }
    }
    Ok(map)
}

/// Summary from a preloaded map, with the dangling-ref fallback.
#[must_use]
pub fn summary_from(map: &HashMap<UserId, UserSummaryDto>, id: UserId) -> UserSummaryDto {
    map.get(&id)
        .cloned()
        .unwrap_or_else(|| dto::unknown_user_summary(id))
}

/// Resolves the synthetic display role for a single user.
pub async fn role_for_user(groups: &GroupService, user: &User) -> Result<UserRole, AppError> {
    let memberships = groups.active_memberships_for_users(&[user.id]).await?;
    let m = memberships.get(&user.id).map_or(&[][..], Vec::as_slice);
    let it_group = groups.it_group_id().await?;
    Ok(compute_role(user, m, it_group))
}

/// Resolves display roles for a batch of users (one membership fetch + one
/// IT-group lookup). Used where the response carries `UserDto`, not summaries.
pub async fn role_map(
    groups: &GroupService,
    users: &[User],
) -> Result<HashMap<UserId, UserRole>, AppError> {
    let ids: Vec<UserId> = users.iter().map(|u| u.id).collect();
    let memberships = groups.active_memberships_for_users(&ids).await?;
    let it_group = groups.it_group_id().await?;
    let mut map = HashMap::with_capacity(users.len());
    for user in users {
        let m = memberships.get(&user.id).map_or(&[][..], Vec::as_slice);
        map.insert(user.id, compute_role(user, m, it_group));
    }
    Ok(map)
}

/// Fetches one group and projects a summary, with a dangling-ref fallback.
pub async fn group_summary(
    groups: &GroupService,
    id: GroupId,
) -> Result<GroupSummaryDto, AppError> {
    Ok(match groups.find(id).await? {
        Some(group) => dto::group_summary_dto(&group),
        None => dto::unknown_group_summary(id),
    })
}

/// Loads a deduplicated map of groups for a batch of ids.
pub async fn group_map(
    groups: &GroupService,
    ids: impl IntoIterator<Item = GroupId>,
) -> Result<HashMap<GroupId, Group>, AppError> {
    let unique: HashSet<GroupId> = ids.into_iter().collect();
    let mut map = HashMap::with_capacity(unique.len());
    for id in unique {
        if let Some(group) = groups.find(id).await? {
            map.insert(id, group);
        }
    }
    Ok(map)
}

/// Group summary from a preloaded map, with the dangling-ref fallback.
#[must_use]
pub fn group_summary_from(map: &HashMap<GroupId, Group>, id: GroupId) -> GroupSummaryDto {
    map.get(&id)
        .map_or_else(|| dto::unknown_group_summary(id), dto::group_summary_dto)
}
