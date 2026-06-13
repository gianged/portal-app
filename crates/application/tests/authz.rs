//! Behavior guard for `OpenFGA` tuple writes.
//!
//! The vocabulary unit test in `permissions.rs` proves the relation *names* line
//! up with the model; this proves the *services actually write the tuples* the
//! model needs to traverse (the second half of the original bug, where no
//! `owner_group` / `company` / `requester` tuples were ever written). Repositories and
//! the authz client are in-memory fakes; the fake authz client records every
//! tuple so the tests can assert on them.

use std::sync::{Arc, Mutex};

use application::{
    ChatService, Error, EventBus, GroupService, Permissions, ProjectService, TicketService,
    commands::{
        group::{AddMembershipCommand, CreateGroupCommand},
        project::CreateProjectCommand,
        ticket::RaiseTicketCommand,
    },
};
use async_trait::async_trait;
use domain::{
    error::{AuthzError, EventError, JobError, RepositoryError, StorageError},
    ids::{
        ChannelId, GroupId, MembershipId, MessageId, ProjectCollaboratorId, ProjectId,
        ProjectInviteId, RequestId, TicketId, UserId,
    },
    model::{
        Announcement, Channel, ChannelKind, ChannelMembership, ChatAttachment, Group, GroupKind,
        GroupRole, Membership, Message, Project, ProjectCollaborator, ProjectInvite, Request,
        RequestAttachment, RequestStatus, SystemRole, Ticket, TicketCategory, TicketStatus, User,
        UserStatus,
    },
    ports::{
        authz_client::{AuthzClient, RelationTuple},
        event_publisher::EventPublisher,
        file_storage::{FileStorage, StorageObject},
        job_queue::JobQueue,
    },
    repository::{
        ChatAttachmentRepository, ChatRepository, GroupRepository, ProjectRepository,
        RequestRepository, TicketRepository, UserRepository,
    },
};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

// --- recording fake authz client ----------------------------------------------

type Tuple = (String, String, String);

#[derive(Default)]
struct FakeAuthz {
    writes: Mutex<Vec<Tuple>>,
}

impl FakeAuthz {
    fn writes(&self) -> Vec<Tuple> {
        self.writes.lock().unwrap().clone()
    }
    fn record(&self, subject: &str, relation: &str, object: &str) {
        self.writes
            .lock()
            .unwrap()
            .push((subject.into(), relation.into(), object.into()));
    }
}

#[async_trait]
impl AuthzClient for FakeAuthz {
    async fn check(
        &self,
        _user: UserId,
        _relation: &str,
        _object: &str,
    ) -> Result<bool, AuthzError> {
        Ok(false)
    }
    async fn write_tuple(
        &self,
        subject: &str,
        relation: &str,
        object: &str,
    ) -> Result<(), AuthzError> {
        self.record(subject, relation, object);
        Ok(())
    }
    async fn delete_tuple(
        &self,
        _subject: &str,
        _relation: &str,
        _object: &str,
    ) -> Result<(), AuthzError> {
        Ok(())
    }
    async fn write_tuples(
        &self,
        writes: &[RelationTuple],
        _deletes: &[RelationTuple],
    ) -> Result<(), AuthzError> {
        for t in writes {
            self.record(&t.subject, &t.relation, &t.object);
        }
        Ok(())
    }
    async fn list_objects(
        &self,
        _user: UserId,
        _relation: &str,
        _object_type: &str,
    ) -> Result<Vec<String>, AuthzError> {
        Ok(Vec::new())
    }
}

// --- fake repositories (only the exercised methods do anything) ----------------

#[derive(Default)]
struct FakeUsers {
    users: Mutex<Vec<User>>,
}

#[async_trait]
impl UserRepository for FakeUsers {
    async fn find_by_id(&self, id: UserId) -> Result<Option<User>, RepositoryError> {
        Ok(self
            .users
            .lock()
            .unwrap()
            .iter()
            .find(|u| u.id == id)
            .cloned())
    }
    async fn find_by_email(&self, _email: &str) -> Result<Option<User>, RepositoryError> {
        Ok(None)
    }
    async fn list_active(
        &self,
        _limit: u32,
        _offset: u32,
        _q: Option<&str>,
    ) -> Result<Vec<User>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn save(&self, _user: &User) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn list_avatar_keys(&self) -> Result<Vec<String>, RepositoryError> {
        Ok(Vec::new())
    }
}

#[derive(Default)]
struct FakeGroups {
    groups: Mutex<Vec<Group>>,
    memberships: Mutex<Vec<Membership>>,
    it_group: Mutex<Option<Group>>,
}

#[async_trait]
impl GroupRepository for FakeGroups {
    async fn find_group(&self, id: GroupId) -> Result<Option<Group>, RepositoryError> {
        Ok(self
            .groups
            .lock()
            .unwrap()
            .iter()
            .find(|g| g.id == id)
            .cloned())
    }
    async fn list_all(&self) -> Result<Vec<Group>, RepositoryError> {
        Ok(self.groups.lock().unwrap().clone())
    }
    async fn find_it_group(&self) -> Result<Option<Group>, RepositoryError> {
        Ok(self.it_group.lock().unwrap().clone())
    }
    async fn save_group(&self, _group: &Group) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn find_membership(
        &self,
        group_id: GroupId,
        user_id: UserId,
    ) -> Result<Option<Membership>, RepositoryError> {
        Ok(self
            .memberships
            .lock()
            .unwrap()
            .iter()
            .find(|m| m.group_id == group_id && m.user_id == user_id)
            .cloned())
    }
    async fn list_memberships_for_group(
        &self,
        group_id: GroupId,
    ) -> Result<Vec<Membership>, RepositoryError> {
        Ok(self
            .memberships
            .lock()
            .unwrap()
            .iter()
            .filter(|m| m.group_id == group_id)
            .cloned()
            .collect())
    }
    async fn list_active_memberships_for_user(
        &self,
        _user_id: UserId,
    ) -> Result<Vec<Membership>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn list_active_memberships_for_users(
        &self,
        user_ids: &[UserId],
    ) -> Result<Vec<Membership>, RepositoryError> {
        Ok(self
            .memberships
            .lock()
            .unwrap()
            .iter()
            .filter(|m| m.deactivated_at.is_none() && user_ids.contains(&m.user_id))
            .cloned()
            .collect())
    }
    async fn save_membership(&self, membership: &Membership) -> Result<(), RepositoryError> {
        self.memberships.lock().unwrap().push(membership.clone());
        Ok(())
    }
}

#[derive(Default)]
struct FakeProjects;

#[async_trait]
impl ProjectRepository for FakeProjects {
    async fn find_by_id(&self, _id: ProjectId) -> Result<Option<Project>, RepositoryError> {
        Ok(None)
    }
    async fn list_for_owner_group(
        &self,
        _group_id: GroupId,
        _q: Option<&str>,
    ) -> Result<Vec<Project>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn list_for_collaborator_group(
        &self,
        _group_id: GroupId,
    ) -> Result<Vec<Project>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn save_project(&self, _project: &Project) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn list_collaborators(
        &self,
        _project_id: ProjectId,
    ) -> Result<Vec<ProjectCollaborator>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn save_collaborator(
        &self,
        _collaborator: &ProjectCollaborator,
    ) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn delete_collaborator(&self, _id: ProjectCollaboratorId) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn find_invite(
        &self,
        _id: ProjectInviteId,
    ) -> Result<Option<ProjectInvite>, RepositoryError> {
        Ok(None)
    }
    async fn list_pending_invites_for_group(
        &self,
        _group_id: GroupId,
    ) -> Result<Vec<ProjectInvite>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn list_pending_invites_for_project(
        &self,
        _project_id: ProjectId,
    ) -> Result<Vec<ProjectInvite>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn save_invite(&self, _invite: &ProjectInvite) -> Result<(), RepositoryError> {
        Ok(())
    }
}

#[derive(Default)]
struct FakeRequests;

#[async_trait]
impl RequestRepository for FakeRequests {
    async fn find_by_id(&self, _id: RequestId) -> Result<Option<Request>, RepositoryError> {
        Ok(None)
    }
    async fn list_for_project(
        &self,
        _project_id: ProjectId,
        _status: Option<RequestStatus>,
        _q: Option<&str>,
    ) -> Result<Vec<Request>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn list_for_assignee(
        &self,
        _assignee: UserId,
        _status: Option<RequestStatus>,
        _q: Option<&str>,
    ) -> Result<Vec<Request>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn save(&self, _request: &Request) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn list_attachments(
        &self,
        _request_id: RequestId,
    ) -> Result<Vec<RequestAttachment>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn save_attachment(
        &self,
        _attachment: &RequestAttachment,
    ) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn list_all_attachment_keys(&self) -> Result<Vec<String>, RepositoryError> {
        Ok(Vec::new())
    }
}

#[derive(Default)]
struct FakeTickets {
    tickets: Mutex<Vec<Ticket>>,
}

#[async_trait]
impl TicketRepository for FakeTickets {
    async fn find_by_id(&self, id: TicketId) -> Result<Option<Ticket>, RepositoryError> {
        Ok(self
            .tickets
            .lock()
            .unwrap()
            .iter()
            .find(|t| t.id == id)
            .cloned())
    }
    async fn list_open_for_triage(
        &self,
        _limit: u32,
        _q: Option<&str>,
    ) -> Result<Vec<Ticket>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn list_for_assignee(
        &self,
        _assignee: UserId,
        _q: Option<&str>,
    ) -> Result<Vec<Ticket>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn list_for_requester(
        &self,
        _requester: UserId,
        _q: Option<&str>,
    ) -> Result<Vec<Ticket>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn list_resolved_before(
        &self,
        _cutoff: OffsetDateTime,
        _limit: u32,
    ) -> Result<Vec<Ticket>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn save(&self, ticket: &Ticket) -> Result<(), RepositoryError> {
        let mut v = self.tickets.lock().unwrap();
        if let Some(existing) = v.iter_mut().find(|t| t.id == ticket.id) {
            *existing = ticket.clone();
        } else {
            v.push(ticket.clone());
        }
        Ok(())
    }
}

#[derive(Default)]
struct FakeChatAttachments;

#[async_trait]
impl ChatAttachmentRepository for FakeChatAttachments {
    async fn save(&self, _attachment: &ChatAttachment) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn find_by_keys(&self, _keys: &[String]) -> Result<Vec<ChatAttachment>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn list_all_keys(&self) -> Result<Vec<String>, RepositoryError> {
        Ok(Vec::new())
    }
}

#[derive(Default)]
struct FakeStorage;

#[async_trait]
impl FileStorage for FakeStorage {
    async fn put(&self, _key: &str, _ct: &str, _bytes: Vec<u8>) -> Result<(), StorageError> {
        Ok(())
    }
    async fn get(&self, _key: &str) -> Result<Vec<u8>, StorageError> {
        Ok(Vec::new())
    }
    async fn delete(&self, _key: &str) -> Result<(), StorageError> {
        Ok(())
    }
    async fn presign_get(
        &self,
        key: &str,
        _ttl: std::time::Duration,
        _user: UserId,
    ) -> Result<String, StorageError> {
        Ok(format!("/files/{key}"))
    }
    async fn list(&self, _prefix: &str) -> Result<Vec<StorageObject>, StorageError> {
        Ok(Vec::new())
    }
}

#[derive(Default)]
struct FakeChats;

#[async_trait]
impl ChatRepository for FakeChats {
    async fn find_channel(&self, _id: ChannelId) -> Result<Option<Channel>, RepositoryError> {
        Ok(None)
    }
    async fn find_direct_channel(
        &self,
        _a: UserId,
        _b: UserId,
    ) -> Result<Option<Channel>, RepositoryError> {
        Ok(None)
    }
    async fn save_channel(&self, _channel: &Channel) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn find_group_channel(
        &self,
        _group_id: GroupId,
    ) -> Result<Option<Channel>, RepositoryError> {
        Ok(None)
    }
    async fn find_general_channel(&self) -> Result<Option<Channel>, RepositoryError> {
        Ok(None)
    }
    async fn subscribe_member(
        &self,
        _user_id: UserId,
        _channel_id: ChannelId,
        _kind: ChannelKind,
    ) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn unsubscribe_member(
        &self,
        _user_id: UserId,
        _channel_id: ChannelId,
    ) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn list_channels_for_user(
        &self,
        _user_id: UserId,
    ) -> Result<Vec<ChannelMembership>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn update_last_read(
        &self,
        _user_id: UserId,
        _channel_id: ChannelId,
        _at: OffsetDateTime,
    ) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn list_messages(
        &self,
        _channel_id: ChannelId,
        _before: Option<MessageId>,
        _limit: u32,
    ) -> Result<Vec<Message>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn find_message(
        &self,
        _channel_id: ChannelId,
        _message_id: MessageId,
    ) -> Result<Option<Message>, RepositoryError> {
        Ok(None)
    }
    async fn save_message(&self, _message: &Message) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn find_announcement(
        &self,
        _channel_id: ChannelId,
        _message_id: MessageId,
    ) -> Result<Option<Announcement>, RepositoryError> {
        Ok(None)
    }
    async fn list_announcements(
        &self,
        _channel_id: ChannelId,
    ) -> Result<Vec<Announcement>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn save_announcement(&self, _announcement: &Announcement) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn delete_announcement(
        &self,
        _channel_id: ChannelId,
        _message_id: MessageId,
    ) -> Result<(), RepositoryError> {
        Ok(())
    }
}

#[derive(Default)]
struct FakePublisher;

#[async_trait]
impl EventPublisher for FakePublisher {
    async fn publish(&self, _topic: &str, _payload: &[u8]) -> Result<(), EventError> {
        Ok(())
    }
}

#[derive(Default)]
struct FakeJobs;

#[async_trait]
impl JobQueue for FakeJobs {
    async fn enqueue(&self, _queue: &str, _payload: &[u8]) -> Result<(), JobError> {
        Ok(())
    }
}

// --- builders + assertions -----------------------------------------------------

fn user(system_role: Option<SystemRole>) -> User {
    let now = OffsetDateTime::now_utc();
    User {
        id: UserId(Uuid::now_v7()),
        email: "a@b.c".into(),
        password_hash: String::new(),
        full_name: "Test".into(),
        avatar_storage_key: None,
        phone: None,
        timezone: "UTC".into(),
        status: UserStatus::Active,
        system_role,
        first_logged_in_at: Some(now),
        deactivated_at: None,
        created_at: now,
        updated_at: now,
    }
}

fn group(kind: GroupKind) -> Group {
    let now = OffsetDateTime::now_utc();
    Group {
        id: GroupId(Uuid::now_v7()),
        name: "G".into(),
        description: String::new(),
        kind,
        created_at: now,
        updated_at: now,
    }
}

fn membership(group_id: GroupId, user_id: UserId, role: GroupRole) -> Membership {
    let now = OffsetDateTime::now_utc();
    Membership {
        id: MembershipId(Uuid::now_v7()),
        group_id,
        user_id,
        role,
        joined_at: now,
        deactivated_at: None,
        created_at: now,
        updated_at: now,
    }
}

fn events() -> Arc<EventBus> {
    Arc::new(EventBus::new(
        Arc::new(FakePublisher),
        Arc::new(FakeJobs),
        Arc::new(FakeJobs),
    ))
}

fn has(writes: &[Tuple], subject: &str, relation: &str, object: &str) -> bool {
    writes
        .iter()
        .any(|(s, r, o)| s == subject && r == relation && o == object)
}

// --- tests ---------------------------------------------------------------------

#[tokio::test]
async fn create_project_writes_owner_group_and_company_tuples() {
    let leader = user(None);
    let owner = group(GroupKind::Standard);

    let users = Arc::new(FakeUsers::default());
    let groups = Arc::new(FakeGroups::default());
    groups
        .memberships
        .lock()
        .unwrap()
        .push(membership(owner.id, leader.id, GroupRole::Leader));

    let authz = Arc::new(FakeAuthz::default());
    let perms = Arc::new(Permissions::new(users, groups, authz.clone()));
    let svc = ProjectService::new(
        Arc::new(FakeProjects),
        Arc::new(FakeRequests),
        perms,
        events(),
    );

    let project = svc
        .create_project(
            leader.id,
            CreateProjectCommand {
                owner_group_id: owner.id,
                name: "P".into(),
                description: String::new(),
            },
        )
        .await
        .expect("create_project");

    let writes = authz.writes();
    let obj = format!("project:{}", project.id.0);
    assert!(
        has(
            &writes,
            &format!("group:{}", owner.id.0),
            "owner_group",
            &obj
        ),
        "must bind the owner group to the project: {writes:?}"
    );
    assert!(
        has(&writes, "company:portal", "company", &obj),
        "must bind the project to the company singleton: {writes:?}"
    );
}

#[tokio::test]
async fn raise_ticket_writes_requester_it_group_and_company() {
    let requester = user(None);
    let it = group(GroupKind::It);

    let users = Arc::new(FakeUsers::default());
    users.users.lock().unwrap().push(requester.clone());
    let groups = Arc::new(FakeGroups::default());
    *groups.it_group.lock().unwrap() = Some(it.clone());

    let authz = Arc::new(FakeAuthz::default());
    let perms = Arc::new(Permissions::new(users, groups, authz.clone()));
    let svc = TicketService::new(Arc::new(FakeTickets::default()), perms, events());

    let ticket = svc
        .raise(
            requester.id,
            RaiseTicketCommand {
                title: "broken".into(),
                description: String::new(),
                category: TicketCategory::Hardware,
            },
        )
        .await
        .expect("raise");

    let writes = authz.writes();
    let obj = format!("ticket:{}", ticket.id.0);
    assert!(
        has(
            &writes,
            &format!("user:{}", requester.id.0),
            "requester",
            &obj
        ),
        "requester tuple: {writes:?}"
    );
    assert!(
        has(&writes, &format!("group:{}", it.id.0), "it_group", &obj),
        "it_group tuple: {writes:?}"
    );
    assert!(
        has(&writes, "company:portal", "company", &obj),
        "company tuple: {writes:?}"
    );
}

#[tokio::test]
async fn create_group_writes_group_and_channel_company_tuples() {
    let hr = user(Some(SystemRole::Hr));

    let users = Arc::new(FakeUsers::default());
    users.users.lock().unwrap().push(hr.clone());
    let authz = Arc::new(FakeAuthz::default());
    let perms = Arc::new(Permissions::new(
        users,
        Arc::new(FakeGroups::default()),
        authz.clone(),
    ));
    let svc = GroupService::new(
        Arc::new(FakeGroups::default()),
        Arc::new(FakeProjects),
        Arc::new(FakeChats),
        perms,
        events(),
    );

    let created = svc
        .create_group(
            hr.id,
            CreateGroupCommand {
                name: "Eng".into(),
                description: String::new(),
                kind: GroupKind::Standard,
            },
        )
        .await
        .expect("create_group");

    let writes = authz.writes();
    let group_obj = format!("group:{}", created.id.0);
    assert!(
        has(&writes, "company:portal", "company", &group_obj),
        "group company tuple: {writes:?}"
    );
    // One group_channel was created; assert its parent_group + company tuples exist.
    let parent = writes.iter().find(|(_, r, _)| r == "parent_group");
    let (subj, _, chan_obj) = parent.expect("a parent_group tuple must be written");
    assert_eq!(
        subj, &group_obj,
        "channel parent_group must point at the new group"
    );
    assert!(
        chan_obj.starts_with("group_channel:"),
        "parent_group object must be a group_channel"
    );
    assert!(
        has(&writes, "company:portal", "company", chan_obj),
        "channel company tuple: {writes:?}"
    );
}

#[tokio::test]
async fn add_member_writes_direct_member_not_computed_member() {
    let hr = user(Some(SystemRole::Hr));
    let newbie = user(None);
    let g = group(GroupKind::Standard);

    // Permissions only needs the HR user (require_hr reads system_role).
    let perm_users = Arc::new(FakeUsers::default());
    perm_users.users.lock().unwrap().push(hr.clone());
    let authz = Arc::new(FakeAuthz::default());
    let perms = Arc::new(Permissions::new(
        perm_users,
        Arc::new(FakeGroups::default()),
        authz.clone(),
    ));

    // The service's own groups fake must hold the target group.
    let groups = Arc::new(FakeGroups::default());
    groups.groups.lock().unwrap().push(g.clone());
    let svc = GroupService::new(
        groups,
        Arc::new(FakeProjects),
        Arc::new(FakeChats),
        perms,
        events(),
    );

    svc.add_membership(
        hr.id,
        AddMembershipCommand {
            group_id: g.id,
            user_id: newbie.id,
            role: GroupRole::Member,
        },
    )
    .await
    .expect("add_membership");

    let writes = authz.writes();
    assert!(
        has(
            &writes,
            &format!("user:{}", newbie.id.0),
            "direct_member",
            &format!("group:{}", g.id.0)
        ),
        "a plain member must be written to the directly-assignable `direct_member` relation: {writes:?}"
    );
    assert!(
        !writes.iter().any(|(_, r, _)| r == "member"),
        "must NOT write the computed `member` relation (OpenFGA would reject it): {writes:?}"
    );
}

// --- cross-cutting invariant tests ---------------------------------------------
//
// These exercise the application services' enforcement of the documented domain
// invariants (CLAUDE.md), using the same in-memory fakes as the tuple-write tests.

fn closed_ticket(requester: UserId, closed_at: OffsetDateTime) -> Ticket {
    Ticket {
        id: TicketId(Uuid::now_v7()),
        requester_user_id: requester,
        assignee_user_id: None,
        title: "broken".into(),
        description: String::new(),
        status: TicketStatus::Closed,
        priority: None,
        category: TicketCategory::Hardware,
        triaged_at: Some(closed_at),
        resolved_at: Some(closed_at),
        closed_at: Some(closed_at),
        created_at: closed_at,
        updated_at: closed_at,
    }
}

/// Invariant 1: a group has exactly one leader — a second leader is rejected.
#[tokio::test]
async fn invariant_group_has_one_leader() {
    let hr = user(Some(SystemRole::Hr));
    let existing_leader = user(None);
    let newcomer = user(None);
    let g = group(GroupKind::Standard);

    let users = Arc::new(FakeUsers::default());
    users.users.lock().unwrap().push(hr.clone());
    let groups = Arc::new(FakeGroups::default());
    groups.groups.lock().unwrap().push(g.clone());
    groups.memberships.lock().unwrap().push(membership(
        g.id,
        existing_leader.id,
        GroupRole::Leader,
    ));

    let authz = Arc::new(FakeAuthz::default());
    let perms = Arc::new(Permissions::new(users, groups.clone(), authz));
    let svc = GroupService::new(
        groups,
        Arc::new(FakeProjects),
        Arc::new(FakeChats),
        perms,
        events(),
    );

    let err = svc
        .add_membership(
            hr.id,
            AddMembershipCommand {
                group_id: g.id,
                user_id: newcomer.id,
                role: GroupRole::Leader,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::Conflict(ref m) if m == "group_already_has_leader"),
        "a second leader must be rejected, got {err:?}"
    );
}

/// Invariant 3: one role per user per group — re-adding an active member is rejected.
#[tokio::test]
async fn invariant_one_membership_per_user_per_group() {
    let hr = user(Some(SystemRole::Hr));
    let member = user(None);
    let g = group(GroupKind::Standard);

    let users = Arc::new(FakeUsers::default());
    users.users.lock().unwrap().push(hr.clone());
    let groups = Arc::new(FakeGroups::default());
    groups.groups.lock().unwrap().push(g.clone());
    groups
        .memberships
        .lock()
        .unwrap()
        .push(membership(g.id, member.id, GroupRole::Member));

    let authz = Arc::new(FakeAuthz::default());
    let perms = Arc::new(Permissions::new(users, groups.clone(), authz));
    let svc = GroupService::new(
        groups,
        Arc::new(FakeProjects),
        Arc::new(FakeChats),
        perms,
        events(),
    );

    let err = svc
        .add_membership(
            hr.id,
            AddMembershipCommand {
                group_id: g.id,
                user_id: member.id,
                role: GroupRole::SubLeader,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::Conflict(ref m) if m == "user_already_member"),
        "a duplicate membership must be rejected, got {err:?}"
    );
}

/// Invariant 7: direct messages are private even from Directors — opening a DM
/// writes NO authz tuples, so there is no `viewer` relation a Director could
/// traverse.
#[tokio::test]
async fn invariant_direct_messages_write_no_authz_tuples() {
    let actor = user(None);
    let other = user(None);

    let users = Arc::new(FakeUsers::default());
    users.users.lock().unwrap().push(actor.clone());
    users.users.lock().unwrap().push(other.clone());

    let authz = Arc::new(FakeAuthz::default());
    let perms = Arc::new(Permissions::new(
        users.clone(),
        Arc::new(FakeGroups::default()),
        authz.clone(),
    ));
    let svc = ChatService::new(
        Arc::new(FakeChats),
        users,
        Arc::new(FakeChatAttachments),
        Arc::new(FakeStorage),
        perms,
        events(),
    );

    svc.open_direct_channel(actor.id, other.id)
        .await
        .expect("open dm");

    assert!(
        authz.writes().is_empty(),
        "a direct channel must write no authz tuples (no Director backdoor): {:?}",
        authz.writes()
    );
}

/// DM guardrails: cannot DM yourself, cannot DM a deactivated user.
#[tokio::test]
async fn direct_message_validation() {
    let actor = user(None);
    let mut inactive = user(None);
    inactive.status = UserStatus::Deactivated;

    let users = Arc::new(FakeUsers::default());
    users.users.lock().unwrap().push(actor.clone());
    users.users.lock().unwrap().push(inactive.clone());
    let authz = Arc::new(FakeAuthz::default());
    let perms = Arc::new(Permissions::new(
        users.clone(),
        Arc::new(FakeGroups::default()),
        authz,
    ));
    let svc = ChatService::new(
        Arc::new(FakeChats),
        users,
        Arc::new(FakeChatAttachments),
        Arc::new(FakeStorage),
        perms,
        events(),
    );

    let self_err = svc
        .open_direct_channel(actor.id, actor.id)
        .await
        .unwrap_err();
    assert!(
        matches!(self_err, Error::Validation(ref m) if m == "cannot_dm_self"),
        "got {self_err:?}"
    );

    let inactive_err = svc
        .open_direct_channel(actor.id, inactive.id)
        .await
        .unwrap_err();
    assert!(
        matches!(inactive_err, Error::Conflict(ref m) if m == "recipient_not_active"),
        "got {inactive_err:?}"
    );
}

/// Ticket reopen is bounded to a 7-day window after closing.
#[tokio::test]
async fn invariant_ticket_reopen_window() {
    let requester = user(None);
    let users = Arc::new(FakeUsers::default());
    users.users.lock().unwrap().push(requester.clone());
    let authz = Arc::new(FakeAuthz::default());
    let perms = Arc::new(Permissions::new(
        users,
        Arc::new(FakeGroups::default()),
        authz,
    ));
    let tickets = Arc::new(FakeTickets::default());
    let now = OffsetDateTime::now_utc();

    // Closed 6 days ago — inside the window, reopen succeeds.
    let t = closed_ticket(requester.id, now - Duration::days(6));
    let ticket_id = t.id;
    tickets.tickets.lock().unwrap().push(t);

    let svc = TicketService::new(tickets.clone(), perms, events());
    let reopened = svc
        .reopen(requester.id, ticket_id)
        .await
        .expect("reopen within window");
    assert_eq!(reopened.status, TicketStatus::Reopened);

    // Move the same ticket's close date to 8 days ago — past the window.
    {
        let mut v = tickets.tickets.lock().unwrap();
        let stored = v.iter_mut().find(|x| x.id == ticket_id).unwrap();
        stored.status = TicketStatus::Closed;
        stored.closed_at = Some(now - Duration::days(8));
        stored.resolved_at = Some(now - Duration::days(8));
    }
    let err = svc.reopen(requester.id, ticket_id).await.unwrap_err();
    assert!(
        matches!(err, Error::Conflict(ref m) if m == "reopen_window_expired"),
        "an expired reopen window must be rejected, got {err:?}"
    );
}
