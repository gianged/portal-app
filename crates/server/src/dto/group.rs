//! Domain <-> wire projections for groups and memberships.

use application::commands::group::{
    AddMembershipCommand, CreateGroupCommand, UpdateGroupMetadataCommand,
};
use domain::{ids, model};
use shared::dto::{
    common::{GroupSummaryDto, UserSummaryDto},
    group::{
        AddMemberRequest, CreateGroupRequest, GroupDto, GroupKind as WireGroupKind,
        GroupRole as WireGroupRole, MembershipDto, UpdateGroupRequest,
    },
};

use super::{group_id, membership_id};

#[must_use]
pub fn group_kind_dto(kind: model::GroupKind) -> WireGroupKind {
    match kind {
        model::GroupKind::Standard => WireGroupKind::Standard,
        model::GroupKind::It => WireGroupKind::It,
    }
}

#[must_use]
pub fn group_kind_domain(kind: WireGroupKind) -> model::GroupKind {
    match kind {
        WireGroupKind::Standard => model::GroupKind::Standard,
        WireGroupKind::It => model::GroupKind::It,
    }
}

#[must_use]
pub fn group_role_dto(role: model::GroupRole) -> WireGroupRole {
    match role {
        model::GroupRole::Leader => WireGroupRole::Leader,
        model::GroupRole::SubLeader => WireGroupRole::SubLeader,
        model::GroupRole::Member => WireGroupRole::Member,
    }
}

#[must_use]
pub fn group_role_domain(role: WireGroupRole) -> model::GroupRole {
    match role {
        WireGroupRole::Leader => model::GroupRole::Leader,
        WireGroupRole::SubLeader => model::GroupRole::SubLeader,
        WireGroupRole::Member => model::GroupRole::Member,
    }
}

#[must_use]
pub fn group_dto(group: &model::Group, member_count: u32) -> GroupDto {
    GroupDto {
        id: group_id(group.id),
        name: group.name.clone(),
        description: group.description.clone(),
        kind: group_kind_dto(group.kind),
        member_count,
        created_at: group.created_at,
    }
}

#[must_use]
pub fn membership_dto(membership: &model::Membership, user: UserSummaryDto) -> MembershipDto {
    MembershipDto {
        id: membership_id(membership.id),
        user,
        role: group_role_dto(membership.role),
        joined_at: membership.joined_at,
        active: membership.deactivated_at.is_none(),
    }
}

#[must_use]
pub fn group_summary_dto(group: &model::Group) -> GroupSummaryDto {
    GroupSummaryDto {
        id: group_id(group.id),
        name: group.name.clone(),
        kind: group_kind_dto(group.kind),
    }
}

#[must_use]
pub fn unknown_group_summary(id: ids::GroupId) -> GroupSummaryDto {
    GroupSummaryDto {
        id: group_id(id),
        name: "Unknown group".to_owned(),
        kind: WireGroupKind::Standard,
    }
}

#[must_use]
pub fn create_group_command(req: CreateGroupRequest) -> CreateGroupCommand {
    CreateGroupCommand {
        name: req.name,
        description: req.description,
        kind: group_kind_domain(req.kind),
    }
}

#[must_use]
pub fn update_group_metadata_command(req: UpdateGroupRequest) -> UpdateGroupMetadataCommand {
    UpdateGroupMetadataCommand {
        name: req.name,
        description: req.description,
    }
}

#[must_use]
pub fn add_membership_command(group: ids::GroupId, req: &AddMemberRequest) -> AddMembershipCommand {
    AddMembershipCommand {
        group_id: group,
        user_id: ids::UserId(req.user_id.0),
        role: group_role_domain(req.role),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_group_summary_is_a_renderable_placeholder() {
        let summary = unknown_group_summary(ids::GroupId(uuid::Uuid::nil()));
        assert_eq!(summary.name, "Unknown group");
    }
}
