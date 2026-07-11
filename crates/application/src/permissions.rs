use std::{collections::HashSet, sync::Arc};

use domain::{
    error::{AuthzError, RepositoryError},
    ids::{ChannelId, GroupId, ProjectId, ServiceAccountId, TicketId, UserId},
    model::{Channel, GroupRole, Membership, SystemRole, User, UserStatus},
    ports::authz_client::{AuthzClient, RelationTuple},
    repository::{GroupRepository, UserRepository},
};

use crate::error::{ConflictCode, Error, Result};

// OpenFGA relation strings; MUST match the relations in
// infra/openfga/authorization-model.json exactly or checks silently deny.
const REL_MEMBER: &str = "member";
const REL_DIRECT_MEMBER: &str = "direct_member";
const REL_LEADER: &str = "leader";
const REL_SUB_LEADER: &str = "sub_leader";
const REL_VIEWER: &str = "viewer";
const REL_CAN_ASSIGN_REQUEST: &str = "can_assign_request";
const REL_HR: &str = "hr";
const REL_DIRECTOR: &str = "director";
const REL_COMPANY: &str = "company";
const REL_OWNER_GROUP: &str = "owner_group";
const REL_COLLABORATOR_GROUP: &str = "collaborator_group";
const REL_PARENT_GROUP: &str = "parent_group";
const REL_REQUESTER: &str = "requester";
const REL_ASSIGNEE: &str = "assignee";
const REL_IT_GROUP: &str = "it_group";
const REL_PROJECT_READER: &str = "project_reader";
const REL_REQUEST_READER: &str = "request_reader";
const REL_REPORT_READER: &str = "report_reader";

/// The single well-known `company` object holding the org-wide `director`, `hr`,
/// and `member` (wildcard) relations; reused when tying a resource to the company.
const COMPANY_OBJECT: &str = "company:portal";
/// Type-bound wildcard subject: every user. Backs `company#member`, which the
/// general channel's viewer computes from.
const USER_WILDCARD: &str = "user:*";

fn subj_user(id: UserId) -> String {
    format!("user:{}", id.0)
}

fn subj_service_account(id: ServiceAccountId) -> String {
    format!("service_account:{}", id.0)
}

fn subj_group(id: GroupId) -> String {
    format!("group:{}", id.0)
}

fn obj_group(id: GroupId) -> String {
    format!("group:{}", id.0)
}

fn obj_project(id: ProjectId) -> String {
    format!("project:{}", id.0)
}

fn obj_ticket(id: TicketId) -> String {
    format!("ticket:{}", id.0)
}

fn obj_group_channel(id: ChannelId) -> String {
    format!("group_channel:{}", id.0)
}

fn obj_general_channel(id: ChannelId) -> String {
    format!("general_channel:{}", id.0)
}

fn role_relation(role: GroupRole) -> &'static str {
    match role {
        GroupRole::Leader => REL_LEADER,
        GroupRole::SubLeader => REL_SUB_LEADER,
        // `group#member` is a computed union, not directly assignable; write plain
        // members to `direct_member`, which the union folds back in.
        GroupRole::Member => REL_DIRECT_MEMBER,
    }
}

fn company_role_relation(role: SystemRole) -> &'static str {
    match role {
        SystemRole::Hr => REL_HR,
        SystemRole::Director => REL_DIRECTOR,
    }
}

/// Read scope grantable to a service account on the external API, mapped 1:1 to
/// a `company` relation assignable to `service_account` subjects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceAccountScope {
    Projects,
    Requests,
    Reports,
}

impl ServiceAccountScope {
    pub const ALL: [ServiceAccountScope; 3] = [Self::Projects, Self::Requests, Self::Reports];

    const fn relation(self) -> &'static str {
        match self {
            Self::Projects => REL_PROJECT_READER,
            Self::Requests => REL_REQUEST_READER,
            Self::Reports => REL_REPORT_READER,
        }
    }
}

/// Authorization gate over the org graph: resolves actors and memberships through
/// the [`UserRepository`] / [`GroupRepository`] and answers permission questions or
/// writes the relation tuples backing a state change through the [`AuthzClient`].
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
    ///
    /// # Errors
    /// Returns `NotFound` if the actor does not exist, `Forbidden` if the actor is not active, or a repository error if the datastore is unavailable.
    pub async fn require_active(&self, actor: UserId) -> Result<User> {
        let user = self.load_user(actor).await?;
        if user.status == UserStatus::Active {
            Ok(user)
        } else {
            Err(Error::Forbidden)
        }
    }

    /// Reports whether the actor holds the org-wide `Director` system role.
    ///
    /// # Errors
    /// Returns `NotFound` if the actor does not exist, or a repository error if the datastore is unavailable.
    pub async fn is_director(&self, actor: UserId) -> Result<bool> {
        let user = self.load_user(actor).await?;
        Ok(matches!(user.system_role, Some(SystemRole::Director)))
    }

    /// Reports whether the actor holds the org-wide `Hr` system role.
    ///
    /// # Errors
    /// Returns `NotFound` if the actor does not exist, or a repository error if the datastore is unavailable.
    pub async fn is_hr(&self, actor: UserId) -> Result<bool> {
        let user = self.load_user(actor).await?;
        Ok(matches!(user.system_role, Some(SystemRole::Hr)))
    }

    /// Reports whether the given user's status is `Active`.
    ///
    /// # Errors
    /// Returns `NotFound` if the user does not exist, or a repository error if the datastore is unavailable.
    pub async fn is_user_active(&self, user_id: UserId) -> Result<bool> {
        let user = self.load_user(user_id).await?;
        Ok(user.status == UserStatus::Active)
    }

    /// Verifies the actor is active and holds the org-wide `Hr` system role.
    ///
    /// # Errors
    /// Returns `NotFound` if the actor does not exist, `Forbidden` if the actor is inactive or is not `Hr`, or a repository error if the datastore is unavailable.
    pub async fn require_hr(&self, actor: UserId) -> Result<()> {
        let user = self.require_active(actor).await?;
        if matches!(user.system_role, Some(SystemRole::Hr)) {
            Ok(())
        } else {
            Err(Error::Forbidden)
        }
    }

    /// Verifies the actor is active and holds the org-wide `Director` system role.
    ///
    /// # Errors
    /// Returns `NotFound` if the actor does not exist, `Forbidden` if the actor is inactive or is not `Director`, or a repository error if the datastore is unavailable.
    pub async fn require_director(&self, actor: UserId) -> Result<()> {
        let user = self.require_active(actor).await?;
        if matches!(user.system_role, Some(SystemRole::Director)) {
            Ok(())
        } else {
            Err(Error::Forbidden)
        }
    }

    /// Verifies the actor is active and holds an org-wide system role (Director or
    /// HR), the admin tier that may read the audit log.
    ///
    /// # Errors
    /// Returns `NotFound` if the actor does not exist, `Forbidden` if the actor is inactive or holds no system role, or a repository error if the datastore is unavailable.
    pub async fn require_admin(&self, actor: UserId) -> Result<()> {
        let user = self.require_active(actor).await?;
        if user.system_role.is_some() {
            Ok(())
        } else {
            Err(Error::Forbidden)
        }
    }

    /// Returns the actor's role in `group` only if their membership is active.
    ///
    /// # Errors
    /// Returns a repository error if the datastore is unavailable.
    pub async fn group_role(&self, actor: UserId, group: GroupId) -> Result<Option<GroupRole>> {
        let Some(membership) = self.groups.find_membership(group, actor).await? else {
            return Ok(None);
        };
        if membership.is_active() {
            Ok(Some(membership.role))
        } else {
            Ok(None)
        }
    }

    /// Verifies the actor is the active leader of `group`.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not the group's active leader, or a repository error if the datastore is unavailable.
    pub async fn require_group_leader(&self, actor: UserId, group: GroupId) -> Result<()> {
        match self.group_role(actor, group).await? {
            Some(GroupRole::Leader) => Ok(()),
            _ => Err(Error::Forbidden),
        }
    }

    /// Verifies the actor is the active leader or sub-leader of `group`.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not an active leader or sub-leader of the group, or a repository error if the datastore is unavailable.
    pub async fn require_group_leader_or_sub(&self, actor: UserId, group: GroupId) -> Result<()> {
        match self.group_role(actor, group).await? {
            Some(GroupRole::Leader | GroupRole::SubLeader) => Ok(()),
            _ => Err(Error::Forbidden),
        }
    }

    /// Reports whether the actor is the active leader of at least one group the
    /// `member` actively belongs to. Backs leader review/approval of a member's
    /// attendance records (daily reports, leave, overtime, flex).
    ///
    /// # Errors
    /// Returns a repository error if the datastore is unavailable.
    pub async fn is_leader_of_member(&self, actor: UserId, member: UserId) -> Result<bool> {
        let member_groups = self.groups.list_active_memberships_for_user(member).await?;
        if member_groups.is_empty() {
            return Ok(false);
        }
        let led: HashSet<GroupId> = self
            .groups
            .list_active_memberships_for_user(actor)
            .await?
            .into_iter()
            .filter(|m| m.role == GroupRole::Leader)
            .map(|m| m.group_id)
            .collect();
        Ok(member_groups.iter().any(|m| led.contains(&m.group_id)))
    }

    /// Verifies the actor is active and the leader of at least one of `member`'s groups.
    ///
    /// # Errors
    /// Returns `NotFound` if the actor does not exist, `Forbidden` if the actor is inactive or leads none of the member's groups, or a repository error if the datastore is unavailable.
    pub async fn require_leader_of_member(&self, actor: UserId, member: UserId) -> Result<()> {
        self.require_active(actor).await?;
        if self.is_leader_of_member(actor, member).await? {
            Ok(())
        } else {
            Err(Error::Forbidden)
        }
    }

    /// Verifies the actor holds any active role in `group`.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor has no active membership in the group, or a repository error if the datastore is unavailable.
    pub async fn require_group_member(&self, actor: UserId, group: GroupId) -> Result<()> {
        match self.group_role(actor, group).await? {
            Some(_) => Ok(()),
            None => Err(Error::Forbidden),
        }
    }

    /// Reports whether the actor holds any active role in the IT group.
    ///
    /// # Errors
    /// Returns a repository error if the datastore is unavailable.
    pub async fn is_it_member(&self, actor: UserId) -> Result<bool> {
        let Some(it_group) = self.groups.find_it_group().await? else {
            return Ok(false);
        };
        Ok(self.group_role(actor, it_group.id).await?.is_some())
    }

    /// Verifies the actor holds an active role in the IT group.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not an active IT-group member, or a repository error if the datastore is unavailable.
    pub async fn require_it_member(&self, actor: UserId) -> Result<()> {
        if self.is_it_member(actor).await? {
            Ok(())
        } else {
            Err(Error::Forbidden)
        }
    }

    // --- Resource read gates ---
    //
    // Reads for projects, requests, tickets, and group channels go through the
    // OpenFGA `viewer` relation, which unions `director from company` so Directors
    // read everything except direct messages (invariant 10). Write/management gates
    // stay as the role checks above.

    /// Run an `OpenFGA` check and map a negative result to `Forbidden`. Shared by
    /// the resource read gates below.
    async fn require_relation(&self, actor: UserId, relation: &str, object: &str) -> Result<()> {
        if self.authz.check(actor, relation, object).await? {
            Ok(())
        } else {
            Err(Error::Forbidden)
        }
    }

    /// Reports whether the actor has the `viewer` relation on `project`.
    ///
    /// # Errors
    /// Returns a repository error if the authz backend is unavailable.
    pub async fn can_view_project(&self, actor: UserId, project: ProjectId) -> Result<bool> {
        Ok(self
            .authz
            .check(actor, REL_VIEWER, &obj_project(project))
            .await?)
    }

    /// Verifies the actor can view `project`.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor cannot view the project, or a repository error if the authz backend is unavailable.
    pub async fn require_can_view_project(&self, actor: UserId, project: ProjectId) -> Result<()> {
        if self.can_view_project(actor, project).await? {
            Ok(())
        } else {
            Err(Error::Forbidden)
        }
    }

    /// Verifies the actor can assign requests within `project`.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor cannot assign requests in the project, or a repository error if the authz backend is unavailable.
    pub async fn require_can_assign_request(
        &self,
        actor: UserId,
        project: ProjectId,
    ) -> Result<()> {
        self.require_relation(actor, REL_CAN_ASSIGN_REQUEST, &obj_project(project))
            .await
    }

    /// Verifies the actor can view `ticket`.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor cannot view the ticket, or a repository error if the authz backend is unavailable.
    pub async fn require_can_view_ticket(&self, actor: UserId, ticket: TicketId) -> Result<()> {
        self.require_relation(actor, REL_VIEWER, &obj_ticket(ticket))
            .await
    }

    /// Verifies the actor can read `channel`: group channels resolve through the
    /// authz `viewer` relation, the general channel requires an active user, and
    /// direct channels are restricted to their two participants.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor cannot view the channel, `NotFound` if the actor does not exist (general channel path), or a repository error if the datastore or authz backend is unavailable.
    pub async fn require_can_view_channel(&self, actor: UserId, channel: &Channel) -> Result<()> {
        match channel {
            // Group reads go through OpenFGA so Directors can read any group's chat.
            Channel::Group(c) => {
                self.require_relation(actor, REL_VIEWER, &obj_group_channel(c.id))
                    .await
            }
            Channel::General(_) => {
                self.require_active(actor).await?;
                Ok(())
            }
            // Direct channels are participant-only, enforced by identity, so there
            // is no OpenFGA branch and no Director backdoor (invariant 10).
            Channel::Direct(c) => {
                if actor == c.user_low_id || actor == c.user_high_id {
                    Ok(())
                } else {
                    Err(Error::Forbidden)
                }
            }
        }
    }

    /// Verifies the actor may post in `channel`: group members may post in their
    /// group channel, `Hr` may post in the general channel, and direct channels
    /// are restricted to their two participants.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor may not post in the channel, `NotFound` if the actor does not exist (general channel path), or a repository error if the datastore is unavailable.
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

    /// Verifies the actor may post an announcement in `channel`: group leaders or
    /// sub-leaders for their group channel, `Hr` for the general channel; direct
    /// channels never permit announcements.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor may not announce in the channel, `NotFound` if the actor does not exist (general channel path), or a repository error if the datastore is unavailable.
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
    ///
    /// # Errors
    /// Returns a repository error if the datastore is unavailable.
    pub async fn is_channel_moderator(&self, actor: UserId, channel: &Channel) -> Result<bool> {
        match channel {
            Channel::Group(c) => Ok(matches!(
                self.group_role(actor, c.group_id).await?,
                Some(GroupRole::Leader)
            )),
            Channel::General(_) | Channel::Direct(_) => Ok(false),
        }
    }

    // --- Tuple writes ---
    //
    // Grants funnel through the AuthzClient; helpers format the `ReBAC` ids from
    // domain types, so services never touch raw tuple strings.

    /// Writes the authz tuple granting `member` their role in the group.
    ///
    /// # Errors
    /// Returns `Conflict` if the authz backend rejects the write, or a repository error if the authz backend is unavailable.
    pub async fn grant_group_membership(&self, member: &Membership) -> Result<()> {
        self.authz
            .write_tuple(
                &subj_user(member.user_id),
                role_relation(member.role),
                &obj_group(member.group_id),
            )
            .await
            .map_err(map_authz_write)
    }

    /// Deletes the authz tuple for `member`'s role in the group.
    ///
    /// # Errors
    /// Returns `Conflict` if the authz backend rejects the delete, or a repository error if the authz backend is unavailable.
    pub async fn revoke_group_membership(&self, member: &Membership) -> Result<()> {
        self.authz
            .delete_tuple(
                &subj_user(member.user_id),
                role_relation(member.role),
                &obj_group(member.group_id),
            )
            .await
            .map_err(map_authz_write)
    }

    /// Mirror a user's org-wide `SystemRole` into `company#hr` / `company#director`
    /// so the model's `... or director from company` viewer branches resolve.
    ///
    /// # Errors
    /// Returns `Conflict` if the authz backend rejects the write, or a repository error if the authz backend is unavailable.
    pub async fn grant_company_role(&self, user: UserId, role: SystemRole) -> Result<()> {
        self.authz
            .write_tuple(
                &subj_user(user),
                company_role_relation(role),
                COMPANY_OBJECT,
            )
            .await
            .map_err(map_authz_write)
    }

    /// Removes a user's org-wide `SystemRole` tuple from the company singleton.
    ///
    /// # Errors
    /// Returns `Conflict` if the authz backend rejects the delete, or a repository error if the authz backend is unavailable.
    pub async fn revoke_company_role(&self, user: UserId, role: SystemRole) -> Result<()> {
        self.authz
            .delete_tuple(
                &subj_user(user),
                company_role_relation(role),
                COMPANY_OBJECT,
            )
            .await
            .map_err(map_authz_write)
    }

    /// Seed the `company#member` wildcard (`user:*`) at startup. Idempotent: the
    /// `AuthzClient` adapter treats an already-existing tuple as a no-op.
    ///
    /// # Errors
    /// Returns `Conflict` if the authz backend rejects the write, or a repository error if the authz backend is unavailable.
    pub async fn seed_company_member_wildcard(&self) -> Result<()> {
        self.authz
            .write_tuple(USER_WILDCARD, REL_MEMBER, COMPANY_OBJECT)
            .await
            .map_err(map_authz_write)
    }

    /// On project creation: tie it to its owner group and the company singleton,
    /// atomically so a half-written project can't exist.
    ///
    /// # Errors
    /// Returns `Conflict` if the authz backend rejects the write, or a repository error if the authz backend is unavailable.
    pub async fn grant_project_created(
        &self,
        owner_group: GroupId,
        project: ProjectId,
    ) -> Result<()> {
        let object = obj_project(project);
        let writes = [
            RelationTuple::new(subj_group(owner_group), REL_OWNER_GROUP, object.clone()),
            RelationTuple::new(COMPANY_OBJECT, REL_COMPANY, object),
        ];
        self.authz
            .write_tuples(&writes, &[])
            .await
            .map_err(map_authz_write)
    }

    /// Writes the authz tuple making `group` a collaborator on `project`.
    ///
    /// # Errors
    /// Returns `Conflict` if the authz backend rejects the write, or a repository error if the authz backend is unavailable.
    pub async fn grant_project_collaborator(
        &self,
        group: GroupId,
        project: ProjectId,
    ) -> Result<()> {
        self.authz
            .write_tuple(
                &subj_group(group),
                REL_COLLABORATOR_GROUP,
                &obj_project(project),
            )
            .await
            .map_err(map_authz_write)
    }

    /// Deletes the authz tuple making `group` a collaborator on `project`.
    ///
    /// # Errors
    /// Returns `Conflict` if the authz backend rejects the delete, or a repository error if the authz backend is unavailable.
    pub async fn revoke_project_collaborator(
        &self,
        group: GroupId,
        project: ProjectId,
    ) -> Result<()> {
        self.authz
            .delete_tuple(
                &subj_group(group),
                REL_COLLABORATOR_GROUP,
                &obj_project(project),
            )
            .await
            .map_err(map_authz_write)
    }

    /// On ticket creation: writes the requester, IT group, and company-singleton
    /// tuples that drive the ticket viewer. The IT group is resolved here.
    ///
    /// # Errors
    /// Returns `Conflict` if the authz backend rejects the write, or a repository error if the datastore or authz backend is unavailable.
    pub async fn grant_ticket_created(&self, requester: UserId, ticket: TicketId) -> Result<()> {
        let object = obj_ticket(ticket);
        let mut writes = vec![
            RelationTuple::new(subj_user(requester), REL_REQUESTER, object.clone()),
            RelationTuple::new(COMPANY_OBJECT, REL_COMPANY, object.clone()),
        ];
        // If the IT group isn't provisioned yet, the requester and Directors can
        // still view the ticket; only the IT-member branch is deferred.
        if let Some(it_group) = self.groups.find_it_group().await? {
            writes.push(RelationTuple::new(
                subj_group(it_group.id),
                REL_IT_GROUP,
                object,
            ));
        }
        self.authz
            .write_tuples(&writes, &[])
            .await
            .map_err(map_authz_write)
    }

    /// Writes the authz tuple assigning `ticket` to `assignee`.
    ///
    /// # Errors
    /// Returns `Conflict` if the authz backend rejects the write, or a repository error if the authz backend is unavailable.
    pub async fn grant_ticket_assignee(&self, assignee: UserId, ticket: TicketId) -> Result<()> {
        self.authz
            .write_tuple(&subj_user(assignee), REL_ASSIGNEE, &obj_ticket(ticket))
            .await
            .map_err(map_authz_write)
    }

    /// Tie a group to the company singleton (so org-wide branches resolve).
    ///
    /// # Errors
    /// Returns `Conflict` if the authz backend rejects the write, or a repository error if the authz backend is unavailable.
    pub async fn grant_group_created(&self, group: GroupId) -> Result<()> {
        self.authz
            .write_tuple(COMPANY_OBJECT, REL_COMPANY, &obj_group(group))
            .await
            .map_err(map_authz_write)
    }

    /// On group-channel creation: tie it to its parent group (drives
    /// `parent_member`) and the company singleton.
    ///
    /// # Errors
    /// Returns `Conflict` if the authz backend rejects the write, or a repository error if the authz backend is unavailable.
    pub async fn grant_group_channel_created(
        &self,
        group: GroupId,
        channel: ChannelId,
    ) -> Result<()> {
        let object = obj_group_channel(channel);
        let writes = [
            RelationTuple::new(subj_group(group), REL_PARENT_GROUP, object.clone()),
            RelationTuple::new(COMPANY_OBJECT, REL_COMPANY, object),
        ];
        self.authz
            .write_tuples(&writes, &[])
            .await
            .map_err(map_authz_write)
    }

    /// Tie the general channel to the company singleton; its viewer is
    /// `member from company`, i.e. every user.
    ///
    /// # Errors
    /// Returns `Conflict` if the authz backend rejects the write, or a repository error if the authz backend is unavailable.
    pub async fn grant_general_channel_company(&self, channel: ChannelId) -> Result<()> {
        self.authz
            .write_tuple(COMPANY_OBJECT, REL_COMPANY, &obj_general_channel(channel))
            .await
            .map_err(map_authz_write)
    }

    // --- Service accounts (external read API) ---

    /// Verifies the service account holds `scope` on the company singleton.
    ///
    /// # Errors
    /// Returns `Forbidden` when the scope was not granted, or a repository error if the authz backend is unavailable.
    pub async fn require_service_account_scope(
        &self,
        account: ServiceAccountId,
        scope: ServiceAccountScope,
    ) -> Result<()> {
        if self
            .authz
            .check_subject(
                &subj_service_account(account),
                scope.relation(),
                COMPANY_OBJECT,
            )
            .await?
        {
            Ok(())
        } else {
            Err(Error::Forbidden)
        }
    }

    /// Grants the given read scopes to a service account, atomically.
    ///
    /// # Errors
    /// Returns `Conflict` if the authz backend rejects the write, or a repository error if the authz backend is unavailable.
    pub async fn grant_service_account_scopes(
        &self,
        account: ServiceAccountId,
        scopes: &[ServiceAccountScope],
    ) -> Result<()> {
        let subject = subj_service_account(account);
        let writes: Vec<RelationTuple> = scopes
            .iter()
            .map(|s| RelationTuple::new(subject.clone(), s.relation(), COMPANY_OBJECT))
            .collect();
        self.authz
            .write_tuples(&writes, &[])
            .await
            .map_err(map_authz_write)
    }

    /// Best-effort cleanup of every scope tuple on revocation. Correctness does
    /// not depend on it (revoked keys no longer authenticate), so a missing
    /// tuple is ignored: the single-tuple delete path is idempotent.
    pub async fn revoke_service_account_scopes(&self, account: ServiceAccountId) {
        let subject = subj_service_account(account);
        for scope in ServiceAccountScope::ALL {
            if let Err(e) = self
                .authz
                .delete_tuple(&subject, scope.relation(), COMPANY_OBJECT)
                .await
            {
                tracing::warn!(error = %e, scope = scope.relation(), "service account scope cleanup failed");
            }
        }
    }
}

/// Maps a tuple-write `AuthzError::Denied` to `Conflict` (the backend rejected the
/// write, not a domain authz failure) so it isn't surfaced as 403 to the caller.
fn map_authz_write(err: AuthzError) -> Error {
    match err {
        AuthzError::Denied => Error::Conflict(ConflictCode::AuthzWriteDenied),
        AuthzError::Backend(msg) => Error::Repository(RepositoryError::Backend(msg)),
    }
}

#[cfg(test)]
mod tests {
    //! Vocabulary guard: the relation strings this module sends to `OpenFGA` must
    //! match the loaded authorization model, or checks silently deny.
    use std::collections::HashSet;

    use serde_json::Value;

    const MODEL_JSON: &str = include_str!("../../../infra/openfga/authorization-model.json");

    fn model() -> Value {
        serde_json::from_str(MODEL_JSON).expect("authorization-model.json is valid JSON")
    }

    fn type_def(model: &Value, ty: &str) -> Value {
        model["type_definitions"]
            .as_array()
            .expect("type_definitions array")
            .iter()
            .find(|t| t["type"] == ty)
            .unwrap_or_else(|| panic!("type `{ty}` missing from model"))
            .clone()
    }

    /// All relations declared on a type (computed or directly assignable).
    fn relations(model: &Value, ty: &str) -> HashSet<String> {
        type_def(model, ty)["relations"]
            .as_object()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Only the directly-assignable relations, listed under `metadata.relations`;
    /// writing a tuple to any other relation is rejected by `OpenFGA`.
    fn assignable(model: &Value, ty: &str) -> HashSet<String> {
        type_def(model, ty)["metadata"]["relations"]
            .as_object()
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    }

    #[test]
    fn check_relations_exist_in_model() {
        let m = model();
        let checks = [
            ("project", super::REL_VIEWER),
            ("project", super::REL_CAN_ASSIGN_REQUEST),
            ("ticket", super::REL_VIEWER),
            ("group_channel", super::REL_VIEWER),
        ];
        for (ty, rel) in checks {
            assert!(
                relations(&m, ty).contains(rel),
                "check relation `{rel}` does not exist on type `{ty}` in the model"
            );
        }
    }

    #[test]
    fn written_relations_are_directly_assignable() {
        let m = model();
        let writes = [
            ("group", super::REL_LEADER),
            ("group", super::REL_SUB_LEADER),
            ("group", super::REL_DIRECT_MEMBER),
            ("group", super::REL_COMPANY),
            ("company", super::REL_HR),
            ("company", super::REL_DIRECTOR),
            ("company", super::REL_MEMBER),
            ("company", super::REL_PROJECT_READER),
            ("company", super::REL_REQUEST_READER),
            ("company", super::REL_REPORT_READER),
            ("project", super::REL_OWNER_GROUP),
            ("project", super::REL_COLLABORATOR_GROUP),
            ("project", super::REL_COMPANY),
            ("ticket", super::REL_REQUESTER),
            ("ticket", super::REL_ASSIGNEE),
            ("ticket", super::REL_IT_GROUP),
            ("ticket", super::REL_COMPANY),
            ("group_channel", super::REL_PARENT_GROUP),
            ("group_channel", super::REL_COMPANY),
            ("general_channel", super::REL_COMPANY),
        ];
        for (ty, rel) in writes {
            assert!(
                assignable(&m, ty).contains(rel),
                "written relation `{rel}` on type `{ty}` is not directly assignable \
                 (OpenFGA would reject the write) — e.g. `member` is computed, write `direct_member`"
            );
        }
    }

    #[test]
    fn director_reads_all_resources_except_direct_messages() {
        // Invariant 10: Directors read everything non-direct; DMs stay private.
        let m = model();
        for ty in ["project", "ticket", "group_channel"] {
            let viewer = type_def(&m, ty)["relations"]["viewer"].to_string();
            assert!(
                viewer.contains("director"),
                "type `{ty}` viewer must union `director from company`"
            );
        }
        let direct = type_def(&m, "direct_channel")["relations"]["viewer"].to_string();
        assert!(
            !direct.contains("director"),
            "direct_channel viewer must NOT include director (invariant 10)"
        );
    }
}
