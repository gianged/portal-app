use std::sync::Arc;

use domain::{
    chat::{Channel, GroupChannel},
    group::{Group, GroupRole, Membership},
    ids::{ChannelId, GroupId, MembershipId, UserId},
    ports::{
        chat_repository::ChatRepository, group_repository::GroupRepository,
        project_repository::ProjectRepository,
    },
    project::ProjectStatus,
};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    commands::group::{AddMembershipCommand, CreateGroupCommand},
    error::{Error, Result},
    events::{DomainEvent, EventBus},
    permissions::Permissions,
};

pub struct GroupService {
    groups: Arc<dyn GroupRepository>,
    projects: Arc<dyn ProjectRepository>,
    chats: Arc<dyn ChatRepository>,
    perms: Arc<Permissions>,
    events: Arc<EventBus>,
}

impl GroupService {
    #[must_use]
    pub fn new(
        groups: Arc<dyn GroupRepository>,
        projects: Arc<dyn ProjectRepository>,
        chats: Arc<dyn ChatRepository>,
        perms: Arc<Permissions>,
        events: Arc<EventBus>,
    ) -> Self {
        Self {
            groups,
            projects,
            chats,
            perms,
            events,
        }
    }

    pub async fn create_group(
        &self,
        actor: UserId,
        cmd: CreateGroupCommand,
    ) -> Result<Group> {
        self.perms.require_hr(actor).await?;
        let now = OffsetDateTime::now_utc();

        let group = Group {
            id: GroupId(Uuid::now_v7()),
            name: cmd.name.clone(),
            description: cmd.description,
            kind: cmd.kind,
            created_at: now,
            updated_at: now,
        };
        self.groups.save_group(&group).await?;

        let channel = Channel::Group(GroupChannel {
            id: ChannelId(Uuid::now_v7()),
            group_id: group.id,
            name: cmd.name,
            created_at: now,
        });
        self.chats.save_channel(&channel).await?;

        self.events
            .emit(DomainEvent::GroupCreated {
                group_id: group.id,
                actor,
                at: now,
                after: group.clone(),
            })
            .await?;
        Ok(group)
    }

    pub async fn delete_group(&self, actor: UserId, group_id: GroupId) -> Result<()> {
        self.perms.require_hr(actor).await?;
        let now = OffsetDateTime::now_utc();

        let group = self
            .groups
            .find_group(group_id)
            .await?
            .ok_or(Error::NotFound("group"))?;

        let projects = self.projects.list_for_owner_group(group_id).await?;
        let has_active = projects
            .iter()
            .any(|p| !matches!(p.status, ProjectStatus::Completed | ProjectStatus::Cancelled));
        if has_active {
            return Err(Error::Conflict("group_has_active_projects".into()));
        }

        // The actual row deletion happens at the infrastructure layer in
        // response to the GroupDeleted event; emitting the event is the service
        // signaling intent.
        self.events
            .emit(DomainEvent::GroupDeleted {
                group_id: group.id,
                actor,
                at: now,
                before: group,
            })
            .await?;
        Ok(())
    }

    pub async fn add_membership(
        &self,
        actor: UserId,
        cmd: AddMembershipCommand,
    ) -> Result<Membership> {
        self.perms.require_hr(actor).await?;
        let now = OffsetDateTime::now_utc();

        if self.groups.find_group(cmd.group_id).await?.is_none() {
            return Err(Error::NotFound("group"));
        }
        if let Some(existing) = self
            .groups
            .find_membership(cmd.group_id, cmd.user_id)
            .await?
            && existing.is_active()
        {
            return Err(Error::Conflict("user_already_member".into()));
        }

        if cmd.role == GroupRole::Leader && self.active_leader_exists(cmd.group_id).await? {
            return Err(Error::Conflict("group_already_has_leader".into()));
        }

        let membership = Membership {
            id: MembershipId(Uuid::now_v7()),
            group_id: cmd.group_id,
            user_id: cmd.user_id,
            role: cmd.role,
            joined_at: now,
            deactivated_at: None,
            created_at: now,
            updated_at: now,
        };
        self.groups.save_membership(&membership).await?;
        self.perms.grant_group_membership(&membership).await?;
        self.events
            .emit(DomainEvent::MembershipAdded {
                membership_id: membership.id,
                group_id: cmd.group_id,
                user_id: cmd.user_id,
                role: cmd.role,
                actor,
                at: now,
            })
            .await?;
        Ok(membership)
    }

    pub async fn change_role(
        &self,
        actor: UserId,
        group_id: GroupId,
        user_id: UserId,
        new_role: GroupRole,
    ) -> Result<Membership> {
        self.perms.require_hr(actor).await?;
        let now = OffsetDateTime::now_utc();

        let mut membership = self
            .groups
            .find_membership(group_id, user_id)
            .await?
            .ok_or(Error::NotFound("membership"))?;
        if !membership.is_active() {
            return Err(Error::Conflict("membership_inactive".into()));
        }
        let from_role = membership.role;
        if from_role == new_role {
            return Ok(membership);
        }

        if from_role == GroupRole::Leader && self.count_active_leaders(group_id).await? <= 1 {
            return Err(Error::Conflict("cannot_demote_last_leader".into()));
        }
        if new_role == GroupRole::Leader && self.active_leader_exists(group_id).await? {
            return Err(Error::Conflict("group_already_has_leader".into()));
        }

        // Update OpenFGA: revoke old role tuple before granting the new one.
        self.perms.revoke_group_membership(&membership).await?;
        membership.change_role(new_role, now);
        self.groups.save_membership(&membership).await?;
        self.perms.grant_group_membership(&membership).await?;

        self.events
            .emit(DomainEvent::MembershipRoleChanged {
                membership_id: membership.id,
                group_id,
                user_id,
                from: from_role,
                to: new_role,
                actor,
                at: now,
            })
            .await?;
        Ok(membership)
    }

    pub async fn deactivate_membership(
        &self,
        actor: UserId,
        group_id: GroupId,
        user_id: UserId,
    ) -> Result<()> {
        self.perms.require_hr(actor).await?;
        let now = OffsetDateTime::now_utc();

        let mut membership = self
            .groups
            .find_membership(group_id, user_id)
            .await?
            .ok_or(Error::NotFound("membership"))?;
        if !membership.is_active() {
            return Ok(());
        }
        if membership.role == GroupRole::Leader
            && self.count_active_leaders(group_id).await? <= 1
        {
            return Err(Error::Conflict("transfer_leadership_first".into()));
        }

        self.perms.revoke_group_membership(&membership).await?;
        membership.deactivate(now);
        self.groups.save_membership(&membership).await?;

        self.events
            .emit(DomainEvent::MembershipDeactivated {
                membership_id: membership.id,
                group_id,
                user_id,
                actor,
                at: now,
            })
            .await?;
        Ok(())
    }

    pub async fn transfer_leadership(
        &self,
        actor: UserId,
        group_id: GroupId,
        from_user: UserId,
        to_user: UserId,
    ) -> Result<()> {
        self.perms.require_hr(actor).await?;
        if from_user == to_user {
            return Err(Error::Validation("from_and_to_same_user".into()));
        }
        let now = OffsetDateTime::now_utc();

        let mut from_membership = self
            .groups
            .find_membership(group_id, from_user)
            .await?
            .ok_or(Error::NotFound("from_membership"))?;
        if from_membership.role != GroupRole::Leader || !from_membership.is_active() {
            return Err(Error::Conflict("from_user_not_leader".into()));
        }
        let mut to_membership = self
            .groups
            .find_membership(group_id, to_user)
            .await?
            .ok_or(Error::NotFound("to_membership"))?;
        if !to_membership.is_active() {
            return Err(Error::Conflict("to_user_inactive".into()));
        }

        // Demote first so the partial unique index on (group_id WHERE role=leader)
        // doesn't reject the promotion.
        self.perms.revoke_group_membership(&from_membership).await?;
        let from_was = from_membership.role;
        from_membership.change_role(GroupRole::Member, now);
        self.groups.save_membership(&from_membership).await?;
        self.perms.grant_group_membership(&from_membership).await?;

        self.perms.revoke_group_membership(&to_membership).await?;
        let to_was = to_membership.role;
        to_membership.change_role(GroupRole::Leader, now);
        self.groups.save_membership(&to_membership).await?;
        self.perms.grant_group_membership(&to_membership).await?;

        self.events
            .emit(DomainEvent::MembershipRoleChanged {
                membership_id: from_membership.id,
                group_id,
                user_id: from_user,
                from: from_was,
                to: GroupRole::Member,
                actor,
                at: now,
            })
            .await?;
        self.events
            .emit(DomainEvent::MembershipRoleChanged {
                membership_id: to_membership.id,
                group_id,
                user_id: to_user,
                from: to_was,
                to: GroupRole::Leader,
                actor,
                at: now,
            })
            .await?;
        self.events
            .emit(DomainEvent::LeadershipTransferred {
                group_id,
                from_user,
                to_user,
                actor,
                at: now,
            })
            .await?;
        Ok(())
    }

    pub async fn find(&self, id: GroupId) -> Result<Option<Group>> {
        Ok(self.groups.find_group(id).await?)
    }

    pub async fn list_memberships(
        &self,
        actor: UserId,
        group_id: GroupId,
    ) -> Result<Vec<Membership>> {
        self.perms.require_active(actor).await?;
        // Members of the group, HR, and Directors can see the membership roster.
        let is_member = self.perms.group_role(actor, group_id).await?.is_some();
        if !is_member
            && !self.perms.is_hr(actor).await?
            && !self.perms.is_director(actor).await?
        {
            return Err(Error::Forbidden);
        }
        Ok(self.groups.list_memberships_for_group(group_id).await?)
    }

    async fn active_leader_exists(&self, group_id: GroupId) -> Result<bool> {
        let memberships = self.groups.list_memberships_for_group(group_id).await?;
        Ok(memberships
            .iter()
            .any(|m| m.is_active() && m.role == GroupRole::Leader))
    }

    async fn count_active_leaders(&self, group_id: GroupId) -> Result<usize> {
        let memberships = self.groups.list_memberships_for_group(group_id).await?;
        Ok(memberships
            .iter()
            .filter(|m| m.is_active() && m.role == GroupRole::Leader)
            .count())
    }
}
