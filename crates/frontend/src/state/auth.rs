use leptos::prelude::*;

use shared::dto::{group::GroupRole, ids::GroupId, user::UserDto};

#[derive(Clone, Copy)]
pub struct AuthState {
    pub user: RwSignal<Option<UserDto>>,
    /// `false` until the initial `GET /auth/me` bootstrap resolves. Route guards
    /// wait on this so a page refresh does not flash-redirect to `/login`.
    pub loaded: RwSignal<bool>,
}

impl Default for AuthState {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            user: RwSignal::new(None),
            loaded: RwSignal::new(false),
        }
    }

    pub fn set_user(&self, user: UserDto) {
        self.user.set(Some(user));
    }

    pub fn clear(&self) {
        self.user.set(None);
    }

    #[must_use]
    pub fn is_authenticated(&self) -> bool {
        self.user.with(Option::is_some)
    }

    /// Whether the signed-in user leads `group`. Untracked read.
    #[must_use]
    pub fn is_leader_of(&self, group: GroupId) -> bool {
        self.has_role_in(group, |r| r == GroupRole::Leader)
    }

    /// Whether the signed-in user leads at least one group. Untracked read.
    #[must_use]
    pub fn leads_any_group(&self) -> bool {
        self.user.with_untracked(|u| {
            u.as_ref()
                .is_some_and(|x| x.memberships.iter().any(|m| m.role == GroupRole::Leader))
        })
    }

    /// Whether the signed-in user leads or sub-leads `group`. Untracked read.
    #[must_use]
    pub fn leads_or_subleads(&self, group: GroupId) -> bool {
        self.has_role_in(group, |r| {
            matches!(r, GroupRole::Leader | GroupRole::SubLeader)
        })
    }

    fn has_role_in(&self, group: GroupId, pred: impl Fn(GroupRole) -> bool) -> bool {
        self.user.with_untracked(|u| {
            u.as_ref().is_some_and(|x| {
                x.memberships
                    .iter()
                    .any(|m| m.group_id == group && pred(m.role))
            })
        })
    }
}
