use std::sync::Arc;

use domain::{
    error::AuthzError,
    ids::{ChannelId, GroupId, ProjectId, UserId},
    model::{Channel, GroupRole, Membership, SystemRole, User, UserStatus},
    ports::authz_client::AuthzClient,
    repository::{GroupRepository, UserRepository},
};

use crate::error::{Error, Result};

// OpenFGA relation strings. Kept in one place so the vocabulary doesn't drift.
const REL_MEMBER: &str = "member";
const REL_LEADER: &str = "leader";
const REL_SUB_LEADER: &str = "sub_leader";
const REL_CAN_VIEW: &str = "can_view";
const REL_CAN_MANAGE: &str = "can_manage";
const REL_CAN_ASSIGN: &str = "can_assign";
const REL_PARTICIPANT: &str = "participant";

fn obj_project(id: ProjectId) -> String {
    format!("project:{}", id.0)
}

fn obj_group(id: GroupId) -> String {
    format!("group:{}", id.0)
}

fn obj_channel(id: ChannelId) -> String {
    format!("channel:{}", id.0)
}

fn role_relation(role: GroupRole) -> &'static str {
    match role {
        GroupRole::Leader => REL_LEADER,
        GroupRole::SubLeader => REL_SUB_LEADER,
        GroupRole::Member => REL_MEMBER,
    }
}

pub struct Permissions {
    users: Arc<dyn UserRepository>,
    groups: Arc<dyn GroupRepository>,
    authz: Arc<dyn AuthzClient>,
}

impl Permissions {
    #[must_use]
    pub fn new(
        users: Arc<dyn UserRepository>,
        groups: Arc<dyn GroupRepository>,
        authz: Arc<dyn AuthzClient>,
    ) -> Self {
        Self {
            users,
            groups,
            authz,
        }
    }

    async fn load_user(&self, actor: UserId) -> Result<User> {
        self.users
            .find_by_id(actor)
            .await?
            .ok_or(Error::NotFound("user"))
    }

    /// Loads the actor and verifies they are `Active`. Use to gate any write.
    pub async fn require_active(&self, actor: UserId) -> Result<User> {
        let user = self.load_user(actor).await?;
        if user.status == UserStatus::Active {
            Ok(user)
        } else {
            Err(Error::Forbidden)
        }
    }

    pub async fn is_director(&self, actor: UserId) -> Result<bool> {
        let user = self.load_user(actor).await?;
        Ok(matches!(user.system_role, Some(SystemRole::Director)))
    }

    pub async fn is_hr(&self, actor: UserId) -> Result<bool> {
        let user = self.load_user(actor).await?;
        Ok(matches!(user.system_role, Some(SystemRole::Hr)))
    }

    pub async fn is_user_active(&self, user_id: UserId) -> Result<bool> {
        let user = self.load_user(user_id).await?;
        Ok(user.status == UserStatus::Active)
    }

    pub async fn require_hr(&self, actor: UserId) -> Result<()> {
        let user = self.require_active(actor).await?;
        if matches!(user.system_role, Some(SystemRole::Hr)) {
            Ok(())
        } else {
            Err(Error::Forbidden)
        }
    }

    pub async fn require_director(&self, actor: UserId) -> Result<()> {
        let user = self.require_active(actor).await?;
        if matches!(user.system_role, Some(SystemRole::Director)) {
            Ok(())
        } else {
            Err(Error::Forbidden)
        }
    }

    /// Returns the actor's role in `group` only if their membership is active.
    pub async fn group_role(
        &self,
        actor: UserId,
        group: GroupId,
    ) -> Result<Option<GroupRole>> {
        let Some(membership) = self.groups.find_membership(group, actor).await? else {
            return Ok(None);
        };
        if membership.is_active() {
            Ok(Some(membership.role))
        } else {
            Ok(None)
        }
    }

    pub async fn require_group_leader(&self, actor: UserId, group: GroupId) -> Result<()> {
        match self.group_role(actor, group).await? {
            Some(GroupRole::Leader) => Ok(()),
            _ => Err(Error::Forbidden),
        }
    }

    pub async fn require_group_leader_or_sub(
        &self,
        actor: UserId,
        group: GroupId,
    ) -> Result<()> {
        match self.group_role(actor, group).await? {
            Some(GroupRole::Leader | GroupRole::SubLeader) => Ok(()),
            _ => Err(Error::Forbidden),
        }
    }

    pub async fn require_group_member(&self, actor: UserId, group: GroupId) -> Result<()> {
        match self.group_role(actor, group).await? {
            Some(_) => Ok(()),
            None => Err(Error::Forbidden),
        }
    }

    pub async fn is_it_member(&self, actor: UserId) -> Result<bool> {
        let Some(it_group) = self.groups.find_it_group().await? else {
            return Ok(false);
        };
        Ok(self.group_role(actor, it_group.id).await?.is_some())
    }

    pub async fn require_it_member(&self, actor: UserId) -> Result<()> {
        if self.is_it_member(actor).await? {
            Ok(())
        } else {
            Err(Error::Forbidden)
        }
    }

    pub async fn user_can_view_project(
        &self,
        actor: UserId,
        project: ProjectId,
    ) -> Result<bool> {
        let allowed = self
            .authz
            .check(actor, REL_CAN_VIEW, &obj_project(project))
            .await?;
        Ok(allowed)
    }

    pub async fn require_can_view_project(
        &self,
        actor: UserId,
        project: ProjectId,
    ) -> Result<()> {
        if self.user_can_view_project(actor, project).await? {
            Ok(())
        } else {
            Err(Error::Forbidden)
        }
    }

    pub async fn require_can_manage_project(
        &self,
        actor: UserId,
        project: ProjectId,
    ) -> Result<()> {
        let allowed = self
            .authz
            .check(actor, REL_CAN_MANAGE, &obj_project(project))
            .await?;
        if allowed {
            Ok(())
        } else {
            Err(Error::Forbidden)
        }
    }

    pub async fn require_can_assign_request(
        &self,
        actor: UserId,
        project: ProjectId,
    ) -> Result<()> {
        let allowed = self
            .authz
            .check(actor, REL_CAN_ASSIGN, &obj_project(project))
            .await?;
        if allowed {
            Ok(())
        } else {
            Err(Error::Forbidden)
        }
    }

    pub async fn require_can_view_channel(
        &self,
        actor: UserId,
        channel: &Channel,
    ) -> Result<()> {
        match channel {
            Channel::Group(c) => self.require_group_member(actor, c.group_id).await,
            Channel::General(_) => {
                self.require_active(actor).await?;
                Ok(())
            }
            Channel::Direct(c) => {
                if actor == c.user_low_id || actor == c.user_high_id {
                    Ok(())
                } else {
                    Err(Error::Forbidden)
                }
            }
        }
    }

    pub async fn require_can_post_in_channel(
        &self,
        actor: UserId,
        channel: &Channel,
    ) -> Result<()> {
        match channel {
            Channel::Group(c) => self.require_group_member(actor, c.group_id).await,
            Channel::General(_) => self.require_hr(actor).await,
            Channel::Direct(c) => {
                if actor == c.user_low_id || actor == c.user_high_id {
                    Ok(())
                } else {
                    Err(Error::Forbidden)
                }
            }
        }
    }

    pub async fn require_can_announce_in_channel(
        &self,
        actor: UserId,
        channel: &Channel,
    ) -> Result<()> {
        match channel {
            Channel::Group(c) => self.require_group_leader_or_sub(actor, c.group_id).await,
            Channel::General(_) => self.require_hr(actor).await,
            Channel::Direct(_) => Err(Error::Forbidden),
        }
    }

    /// Group-channel leaders can delete any message in their channel at any
    /// time. Other channels have no moderator beyond the sender.
    pub async fn user_is_channel_moderator(
        &self,
        actor: UserId,
        channel: &Channel,
    ) -> Result<bool> {
        match channel {
            Channel::Group(c) => Ok(matches!(
                self.group_role(actor, c.group_id).await?,
                Some(GroupRole::Leader)
            )),
            Channel::General(_) | Channel::Direct(_) => Ok(false),
        }
    }

    pub async fn grant_group_membership(&self, member: &Membership) -> Result<()> {
        self.authz
            .write_tuple(
                member.user_id,
                role_relation(member.role),
                &obj_group(member.group_id),
            )
            .await
            .map_err(map_authz_write)?;
        Ok(())
    }

    pub async fn revoke_group_membership(&self, member: &Membership) -> Result<()> {
        self.authz
            .delete_tuple(
                member.user_id,
                role_relation(member.role),
                &obj_group(member.group_id),
            )
            .await
            .map_err(map_authz_write)?;
        Ok(())
    }

    pub async fn grant_direct_channel_participant(
        &self,
        user: UserId,
        channel: ChannelId,
    ) -> Result<()> {
        self.authz
            .write_tuple(user, REL_PARTICIPANT, &obj_channel(channel))
            .await
            .map_err(map_authz_write)?;
        Ok(())
    }

    // TODO: project owner/collaborator tuple writes require userset-form
    // identifiers ("group:G#member") which the current AuthzClient port does
    // not expose. Add when the port is extended.
}

/// `AuthzError::Denied` on a tuple-write means the authz backend rejected the
/// write itself, not a domain authorization failure. Surface as Conflict so it
/// doesn't get mapped to 403 for the API caller.
fn map_authz_write(err: AuthzError) -> Error {
    match err {
        AuthzError::Denied => Error::Conflict("authz_write_denied".into()),
        AuthzError::Backend(msg) => {
            Error::Repository(domain::error::RepositoryError::Backend(msg))
        }
    }
}
