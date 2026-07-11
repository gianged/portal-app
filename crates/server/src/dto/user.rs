//! Domain <-> wire projections for users: identity enums, views, and commands.

use application::commands::user::{CreateUserCommand, UpdateProfileCommand};
use domain::{ids, model};
use shared::dto::{
    common::UserSummaryDto,
    user::{
        CreateUserRequest, SystemRole as WireSystemRole, UpdateProfileRequest, UserDto,
        UserMembershipDto, UserProfileDto, UserRole, UserStatus as WireUserStatus,
    },
};

#[must_use]
pub fn system_role_dto(role: model::SystemRole) -> WireSystemRole {
    match role {
        model::SystemRole::Director => WireSystemRole::Director,
        model::SystemRole::Hr => WireSystemRole::Hr,
    }
}

#[must_use]
pub fn system_role_domain(role: WireSystemRole) -> model::SystemRole {
    match role {
        WireSystemRole::Director => model::SystemRole::Director,
        WireSystemRole::Hr => model::SystemRole::Hr,
    }
}

#[must_use]
pub fn user_status_dto(status: model::UserStatus) -> WireUserStatus {
    match status {
        model::UserStatus::Pending => WireUserStatus::Pending,
        model::UserStatus::Active => WireUserStatus::Active,
        model::UserStatus::Deactivated => WireUserStatus::Deactivated,
    }
}

/// Flattens the domain's split identity (`SystemRole` + per-group `GroupRole` +
/// `GroupKind::It`) into the single synthetic display role the UI shows.
/// Precedence: Director > HR > IT > Group Leader > Group Sub-leader > Member.
#[must_use]
pub fn resolve_user_role(
    system_role: Option<model::SystemRole>,
    group_roles: &[model::GroupRole],
    is_it: bool,
) -> UserRole {
    match system_role {
        Some(model::SystemRole::Director) => return UserRole::Director,
        Some(model::SystemRole::Hr) => return UserRole::Hr,
        None => {}
    }
    if is_it {
        UserRole::It
    } else if group_roles.contains(&model::GroupRole::Leader) {
        UserRole::GroupLeader
    } else if group_roles.contains(&model::GroupRole::SubLeader) {
        UserRole::GroupSubLeader
    } else {
        UserRole::Member
    }
}

#[must_use]
pub fn user_dto(
    user: &model::User,
    role: UserRole,
    memberships: Vec<UserMembershipDto>,
) -> UserDto {
    UserDto {
        id: super::user_id(user.id),
        full_name: user.full_name.clone(),
        email: user.email.clone(),
        role,
        memberships,
    }
}

#[must_use]
pub fn user_membership_dto(m: &model::Membership, group_name: String) -> UserMembershipDto {
    UserMembershipDto {
        group_id: super::group_id(m.group_id),
        group_name,
        role: super::group_role_dto(m.role),
    }
}

#[must_use]
pub fn user_profile_dto(user: &model::User) -> UserProfileDto {
    UserProfileDto {
        id: super::user_id(user.id),
        email: user.email.clone(),
        full_name: user.full_name.clone(),
        avatar_storage_key: user.avatar_storage_key.clone(),
        phone: user.phone.clone(),
        timezone: user.timezone.clone(),
        status: user_status_dto(user.status),
        system_role: user.system_role.map(system_role_dto),
        email_notifications: user.email_notifications,
        created_at: user.created_at,
    }
}

#[must_use]
pub fn user_summary_dto(user: &model::User, role: UserRole) -> UserSummaryDto {
    UserSummaryDto {
        id: super::user_id(user.id),
        full_name: user.full_name.clone(),
        avatar_storage_key: user.avatar_storage_key.clone(),
        role,
    }
}

/// Fallback for a dangling user reference (should not occur for FK-backed ids);
/// keeps a denormalized response renderable instead of failing the whole call.
#[must_use]
pub fn unknown_user_summary(id: ids::UserId) -> UserSummaryDto {
    UserSummaryDto {
        id: super::user_id(id),
        full_name: "Unknown user".to_owned(),
        avatar_storage_key: None,
        role: UserRole::Member,
    }
}

#[must_use]
pub fn create_user_command(req: CreateUserRequest) -> CreateUserCommand {
    CreateUserCommand {
        email: req.email,
        password: req.password,
        full_name: req.full_name,
        phone: req.phone,
        timezone: req.timezone,
        system_role: req.system_role.map(system_role_domain),
    }
}

#[must_use]
pub fn update_profile_command(req: UpdateProfileRequest) -> UpdateProfileCommand {
    UpdateProfileCommand {
        full_name: req.full_name,
        phone: req.phone,
        timezone: req.timezone,
        avatar_storage_key: req.avatar_storage_key,
        email_notifications: req.email_notifications,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_role_outranks_group_and_it_membership() {
        // A director who also leads a team still surfaces as Director; HR likewise.
        assert_eq!(
            resolve_user_role(
                Some(model::SystemRole::Director),
                &[model::GroupRole::Leader],
                true,
            ),
            UserRole::Director,
        );
        assert_eq!(
            resolve_user_role(
                Some(model::SystemRole::Hr),
                &[model::GroupRole::Member],
                false
            ),
            UserRole::Hr,
        );
    }

    #[test]
    fn it_membership_outranks_group_role() {
        assert_eq!(
            resolve_user_role(None, &[model::GroupRole::Leader], true),
            UserRole::It,
        );
    }

    #[test]
    fn group_roles_resolve_in_precedence_order() {
        assert_eq!(
            resolve_user_role(None, &[model::GroupRole::Leader], false),
            UserRole::GroupLeader,
        );
        assert_eq!(
            resolve_user_role(None, &[model::GroupRole::SubLeader], false),
            UserRole::GroupSubLeader,
        );
        assert_eq!(
            resolve_user_role(None, &[model::GroupRole::Member], false),
            UserRole::Member,
        );
        // Leader wins when a user holds several group roles at once.
        assert_eq!(
            resolve_user_role(
                None,
                &[model::GroupRole::Member, model::GroupRole::Leader],
                false,
            ),
            UserRole::GroupLeader,
        );
    }

    #[test]
    fn no_roles_default_to_member() {
        assert_eq!(resolve_user_role(None, &[], false), UserRole::Member);
    }

    #[test]
    fn unknown_user_summary_is_a_renderable_placeholder() {
        let summary = unknown_user_summary(ids::UserId(uuid::Uuid::nil()));
        assert_eq!(summary.full_name, "Unknown user");
        assert_eq!(summary.role, UserRole::Member);
        assert!(summary.avatar_storage_key.is_none());
    }
}
