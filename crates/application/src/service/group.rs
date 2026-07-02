use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

use domain::{
    ids::{ChannelId, GroupId, MembershipId, UserId},
    model::{Channel, ChannelKind, Group, GroupChannel, GroupRole, Membership, ProjectStatus},
    repository::{ChatRepository, GroupRepository, ProjectRepository},
};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    commands::group::{AddMembershipCommand, CreateGroupCommand, UpdateGroupMetadataCommand},
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
    /// Memoized id of the single IT group, a stable org invariant. Populated on
    /// first sighting so role resolution stops re-querying it on every login.
    it_group: OnceLock<GroupId>,
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
            it_group: OnceLock::new(),
        }
    }

    /// Creates a group and its group channel.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not HR, a repository error if the
    /// datastore or authz backend is unavailable, or an event error if the event
    /// bus fails.
    #[tracing::instrument(skip_all, fields(actor = ?actor))]
    pub async fn create_group(&self, actor: UserId, cmd: CreateGroupCommand) -> Result<Group> {
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

        let channel_id = ChannelId(Uuid::now_v7());
        let channel = Channel::Group(GroupChannel {
            id: channel_id,
            group_id: group.id,
            name: cmd.name,
            created_at: now,
        });
        self.chats.save_channel(&channel).await?;

        // OpenFGA: tie the group and its channel to the company singleton so the
        // org-wide (Director) viewer branches resolve.
        self.perms.grant_group_created(group.id).await?;
        self.perms
            .grant_group_channel_created(group.id, channel_id)
            .await?;

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

    /// Signals deletion of a group; the row removal happens via the emitted event.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not HR, `NotFound` if the group does
    /// not exist, `Conflict` if the group still owns active projects, a repository
    /// error if the datastore is unavailable, or an event error if the event bus
    /// fails.
    #[tracing::instrument(skip_all, fields(actor = ?actor, group_id = ?group_id))]
    pub async fn delete_group(&self, actor: UserId, group_id: GroupId) -> Result<()> {
        self.perms.require_hr(actor).await?;
        let now = OffsetDateTime::now_utc();

        let group = self
            .groups
            .find_group(group_id)
            .await?
            .ok_or(Error::NotFound("group"))?;

        let projects = self.projects.list_for_owner_group(group_id, None).await?;
        let has_active = projects.iter().any(|p| {
            !matches!(
                p.status,
                ProjectStatus::Completed | ProjectStatus::Cancelled
            )
        });
        if has_active {
            return Err(Error::Conflict("group_has_active_projects".into()));
        }

        // Row deletion happens in infrastructure on the GroupDeleted event; the
        // service only signals intent.
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

    /// Adds a user to a group with the requested role.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not HR, `NotFound` if the group does
    /// not exist, `Conflict` if the user is already an active member or the role
    /// is `Leader` while the group already has one, a repository error if the
    /// datastore or authz backend is unavailable, or an event error if the event
    /// bus fails.
    #[tracing::instrument(skip_all, fields(actor = ?actor))]
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
        // Subscribe the new member to the group channel so it appears in their
        // channel list (channels_by_user is per-user denormalised in Scylla).
        if let Some(channel) = self.chats.find_group_channel(cmd.group_id).await? {
            self.chats
                .subscribe_member(cmd.user_id, channel.id(), ChannelKind::Group)
                .await?;
        }
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

    /// Changes a member's role within a group.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not HR, `NotFound` if the membership
    /// does not exist, `Conflict` if the membership is inactive, the change would
    /// demote the last leader, or the group already has a leader when promoting,
    /// a repository error if the datastore or authz backend is unavailable, or an
    /// event error if the event bus fails.
    #[tracing::instrument(skip_all, fields(actor = ?actor, group_id = ?group_id, user_id = ?user_id))]
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

    /// Deactivates a user's membership in a group.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not HR, `NotFound` if the membership
    /// does not exist, `Conflict` if the member is the last leader (transfer
    /// leadership first), a repository error if the datastore or authz backend is
    /// unavailable, or an event error if the event bus fails.
    #[tracing::instrument(skip_all, fields(actor = ?actor, group_id = ?group_id, user_id = ?user_id))]
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
        if membership.role == GroupRole::Leader && self.count_active_leaders(group_id).await? <= 1 {
            return Err(Error::Conflict("transfer_leadership_first".into()));
        }

        self.perms.revoke_group_membership(&membership).await?;
        membership.deactivate(now);
        self.groups.save_membership(&membership).await?;
        if let Some(channel) = self.chats.find_group_channel(group_id).await? {
            self.chats.unsubscribe_member(user_id, channel.id()).await?;
        }

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

    /// Transfers leadership of a group from one active member to another.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not HR, `Validation` if the source and
    /// target are the same user, `NotFound` if either membership does not exist,
    /// `Conflict` if the source is not an active leader or the target is inactive,
    /// a repository error if the datastore or authz backend is unavailable, or an
    /// event error if the event bus fails.
    #[tracing::instrument(skip_all, fields(actor = ?actor, group_id = ?group_id, from_user = ?from_user, to_user = ?to_user))]
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

    /// Looks up a group by id, returning `None` if it does not exist.
    ///
    /// # Errors
    /// Returns a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(id = ?id))]
    pub async fn find(&self, id: GroupId) -> Result<Option<Group>> {
        Ok(self.groups.find_group(id).await?)
    }

    /// Groups for a batch of ids; missing ids are simply absent.
    ///
    /// # Errors
    /// Returns a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(count = ids.len()))]
    pub async fn find_by_ids(&self, ids: &[GroupId]) -> Result<Vec<Group>> {
        Ok(self.groups.find_by_ids(ids).await?)
    }

    /// Org-wide group directory; any active user may read it.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active, or a repository error if
    /// the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor))]
    pub async fn list_all(&self, actor: UserId) -> Result<Vec<Group>> {
        self.perms.require_active(actor).await?;
        Ok(self.groups.list_all().await?)
    }

    /// Updates a group's name and/or description.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not HR, `NotFound` if the group does
    /// not exist, a repository error if the datastore is unavailable, or an event
    /// error if the event bus fails.
    #[tracing::instrument(skip_all, fields(actor = ?actor, group_id = ?group_id))]
    pub async fn update_metadata(
        &self,
        actor: UserId,
        group_id: GroupId,
        cmd: UpdateGroupMetadataCommand,
    ) -> Result<Group> {
        self.perms.require_hr(actor).await?;
        let mut group = self
            .groups
            .find_group(group_id)
            .await?
            .ok_or(Error::NotFound("group"))?;
        let before = group.clone();
        let now = OffsetDateTime::now_utc();
        if let Some(name) = cmd.name {
            group.name = name;
        }
        if let Some(description) = cmd.description {
            group.description = description;
        }
        group.updated_at = now;
        self.groups.save_group(&group).await?;
        self.events
            .emit(DomainEvent::GroupMetadataUpdated {
                group_id: group.id,
                actor,
                at: now,
                before,
                after: group.clone(),
            })
            .await?;
        Ok(group)
    }

    /// Active membership count for the group (for directory/detail headers).
    ///
    /// # Errors
    /// Returns a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(group_id = ?group_id))]
    pub async fn active_member_count(&self, group_id: GroupId) -> Result<u32> {
        let memberships = self.groups.list_memberships_for_group(group_id).await?;
        let count = memberships.iter().filter(|m| m.is_active()).count();
        Ok(u32::try_from(count).unwrap_or(u32::MAX))
    }

    /// Active memberships for a batch of users, grouped by user. Backs the
    /// display-role resolution for denormalized user summaries (no auth gate:
    /// it only enriches already-authorized responses).
    ///
    /// # Errors
    /// Returns a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all)]
    pub async fn active_memberships_for_users(
        &self,
        user_ids: &[UserId],
    ) -> Result<HashMap<UserId, Vec<Membership>>> {
        let memberships = self
            .groups
            .list_active_memberships_for_users(user_ids)
            .await?;
        let mut map: HashMap<UserId, Vec<Membership>> = HashMap::new();
        for m in memberships {
            map.entry(m.user_id).or_default().push(m);
        }
        Ok(map)
    }

    /// Id of the single `GroupKind::It` group, if one exists.
    ///
    /// Memoized after the first hit (the id is stable); until one exists the lookup
    /// keeps hitting the datastore, and a recreated id needs a restart to clear.
    ///
    /// # Errors
    /// Returns a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all)]
    pub async fn it_group_id(&self) -> Result<Option<GroupId>> {
        if let Some(id) = self.it_group.get() {
            return Ok(Some(*id));
        }
        let Some(group) = self.groups.find_it_group().await? else {
            return Ok(None);
        };
        // First writer wins; a concurrent racer resolves the same id.
        let _ = self.it_group.set(group.id);
        Ok(Some(group.id))
    }

    /// Lists the membership roster for a group. Members of the group, HR, and
    /// Directors may read it.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active or is not a member, HR, or
    /// Director, or a repository error if the datastore or authz backend is
    /// unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, group_id = ?group_id))]
    pub async fn list_memberships(
        &self,
        actor: UserId,
        group_id: GroupId,
    ) -> Result<Vec<Membership>> {
        self.perms.require_active(actor).await?;
        let is_member = self.perms.group_role(actor, group_id).await?.is_some();
        if !is_member && !self.perms.is_hr(actor).await? && !self.perms.is_director(actor).await? {
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
