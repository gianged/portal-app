//! In-memory test doubles and an [`AppState`] builder shared by the server's
//! integration tests. The fakes drive the real [`server::app::router`] without
//! standing up Postgres, Scylla, Redis, or OpenFGA.
//!
//! The fakes implement the `domain` traits with trivial in-memory behaviour;
//! untested-route methods return empty/`Ok` results. Repository shapes mirror the
//! application-layer fakes in `application/tests/authz.rs`.

#![allow(dead_code)]

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

use async_trait::async_trait;
use time::OffsetDateTime;
use uuid::Uuid;

use application::{
    events::EventBus,
    permissions::Permissions,
    service::{
        announcement::AnnouncementService, chat::ChatService, group::GroupService,
        notification::NotificationService, project::ProjectService, request::RequestService,
        ticket::TicketService, user::UserService,
    },
};
use domain::{
    error::{AuthzError, EventError, JobError, RepositoryError},
    ids::{
        ChannelId, GroupId, MessageId, NotificationId, ProjectCollaboratorId, ProjectId,
        ProjectInviteId, RequestId, TicketId, UserId,
    },
    model::{
        Announcement, AuditLog, Channel, ChannelKind, ChannelMembership, Group, Membership,
        Message, Notification, Project, ProjectCollaborator, ProjectInvite, Request,
        RequestAttachment, RequestStatus, Ticket, User, UserStatus,
    },
    ports::{
        authz_client::{AuthzClient, RelationTuple},
        event_publisher::EventPublisher,
        job_queue::JobQueue,
        presence::Presence,
        rate_limit::RateLimit,
    },
    repository::{
        AuditRepository, ChatRepository, GroupRepository, NotificationRepository,
        ProjectRepository, RequestRepository, TicketRepository, UserRepository,
    },
};
use infrastructure::local_storage::LocalStorage;

use server::{
    app::AppState, auth::TokenService, middleware::rate_limit::RateLimits, realtime::Realtime,
};

// --- repositories --------------------------------------------------------------

#[derive(Default)]
pub struct FakeUsers {
    pub users: Mutex<Vec<User>>,
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
    async fn find_by_email(&self, email: &str) -> Result<Option<User>, RepositoryError> {
        Ok(self
            .users
            .lock()
            .unwrap()
            .iter()
            .find(|u| u.email == email)
            .cloned())
    }
    async fn list_active(&self, _limit: u32, _offset: u32) -> Result<Vec<User>, RepositoryError> {
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
pub struct FakeGroups {
    pub groups: Mutex<Vec<Group>>,
    pub memberships: Mutex<Vec<Membership>>,
    pub it_group: Mutex<Option<Group>>,
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
        user_id: UserId,
    ) -> Result<Vec<Membership>, RepositoryError> {
        Ok(self
            .memberships
            .lock()
            .unwrap()
            .iter()
            .filter(|m| m.deactivated_at.is_none() && m.user_id == user_id)
            .cloned()
            .collect())
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

struct FakeProjects;

#[async_trait]
impl ProjectRepository for FakeProjects {
    async fn find_by_id(&self, _id: ProjectId) -> Result<Option<Project>, RepositoryError> {
        Ok(None)
    }
    async fn list_for_owner_group(&self, _g: GroupId) -> Result<Vec<Project>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn list_for_collaborator_group(
        &self,
        _g: GroupId,
    ) -> Result<Vec<Project>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn save_project(&self, _project: &Project) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn list_collaborators(
        &self,
        _id: ProjectId,
    ) -> Result<Vec<ProjectCollaborator>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn save_collaborator(&self, _c: &ProjectCollaborator) -> Result<(), RepositoryError> {
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
        _g: GroupId,
    ) -> Result<Vec<ProjectInvite>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn list_pending_invites_for_project(
        &self,
        _id: ProjectId,
    ) -> Result<Vec<ProjectInvite>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn save_invite(&self, _invite: &ProjectInvite) -> Result<(), RepositoryError> {
        Ok(())
    }
}

struct FakeRequests;

#[async_trait]
impl RequestRepository for FakeRequests {
    async fn find_by_id(&self, _id: RequestId) -> Result<Option<Request>, RepositoryError> {
        Ok(None)
    }
    async fn list_for_project(
        &self,
        _id: ProjectId,
        _status: Option<RequestStatus>,
    ) -> Result<Vec<Request>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn list_for_assignee(
        &self,
        _assignee: UserId,
        _status: Option<RequestStatus>,
    ) -> Result<Vec<Request>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn save(&self, _request: &Request) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn list_attachments(
        &self,
        _id: RequestId,
    ) -> Result<Vec<RequestAttachment>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn save_attachment(&self, _a: &RequestAttachment) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn list_all_attachment_keys(&self) -> Result<Vec<String>, RepositoryError> {
        Ok(Vec::new())
    }
}

struct FakeTickets;

#[async_trait]
impl TicketRepository for FakeTickets {
    async fn find_by_id(&self, _id: TicketId) -> Result<Option<Ticket>, RepositoryError> {
        Ok(None)
    }
    async fn list_open_for_triage(&self, _limit: u32) -> Result<Vec<Ticket>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn list_for_assignee(&self, _assignee: UserId) -> Result<Vec<Ticket>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn list_for_requester(&self, _requester: UserId) -> Result<Vec<Ticket>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn save(&self, _ticket: &Ticket) -> Result<(), RepositoryError> {
        Ok(())
    }
}

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
    async fn find_group_channel(&self, _g: GroupId) -> Result<Option<Channel>, RepositoryError> {
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
    async fn save_announcement(&self, _a: &Announcement) -> Result<(), RepositoryError> {
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

struct FakeNotifications;

#[async_trait]
impl NotificationRepository for FakeNotifications {
    async fn find_by_id(
        &self,
        _id: NotificationId,
    ) -> Result<Option<Notification>, RepositoryError> {
        Ok(None)
    }
    async fn list_for_user(
        &self,
        _user_id: UserId,
        _unread_only: bool,
        _limit: u32,
    ) -> Result<Vec<Notification>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn count_unread(&self, _user_id: UserId) -> Result<u64, RepositoryError> {
        Ok(0)
    }
    async fn save(&self, _notification: &Notification) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn mark_read(
        &self,
        _id: NotificationId,
        _at: OffsetDateTime,
    ) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn delete_read_before(&self, _cutoff: OffsetDateTime) -> Result<u64, RepositoryError> {
        Ok(0)
    }
}

struct FakeAudit;

#[async_trait]
impl AuditRepository for FakeAudit {
    async fn append(&self, _entry: &AuditLog) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn list_for_entity(
        &self,
        _entity_schema: &str,
        _entity_table: &str,
        _entity_id: Uuid,
        _limit: u32,
    ) -> Result<Vec<AuditLog>, RepositoryError> {
        Ok(Vec::new())
    }
}

// --- ports ---------------------------------------------------------------------

struct FakeAuthz;

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
    async fn write_tuple(&self, _s: &str, _r: &str, _o: &str) -> Result<(), AuthzError> {
        Ok(())
    }
    async fn delete_tuple(&self, _s: &str, _r: &str, _o: &str) -> Result<(), AuthzError> {
        Ok(())
    }
    async fn write_tuples(
        &self,
        _writes: &[RelationTuple],
        _deletes: &[RelationTuple],
    ) -> Result<(), AuthzError> {
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

struct FakePublisher;

#[async_trait]
impl EventPublisher for FakePublisher {
    async fn publish(&self, _topic: &str, _payload: &[u8]) -> Result<(), EventError> {
        Ok(())
    }
}

struct FakeJobs;

#[async_trait]
impl JobQueue for FakeJobs {
    async fn enqueue(&self, _queue: &str, _payload: &[u8]) -> Result<(), JobError> {
        Ok(())
    }
}

struct FakePresence;

#[async_trait]
impl Presence for FakePresence {
    async fn set_online(&self, _user: UserId, _ttl_secs: u64) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn is_online(&self, _user: UserId) -> Result<bool, RepositoryError> {
        Ok(false)
    }
}

/// Returns a monotonically increasing count (1, 2, 3, …) regardless of bucket, so
/// the rate-limit middleware trips deterministically once the count passes the
/// configured ceiling.
#[derive(Default)]
struct FakeRateLimit {
    count: AtomicU64,
}

#[async_trait]
impl RateLimit for FakeRateLimit {
    async fn incr(&self, _bucket: &str) -> Result<u64, RepositoryError> {
        Ok(self.count.fetch_add(1, Ordering::SeqCst) + 1)
    }
}

// --- model builders ------------------------------------------------------------

/// An `Active` user with the given id and email, no system role.
#[must_use]
pub fn active_user(id: UserId, email: &str) -> User {
    let now = OffsetDateTime::now_utc();
    User {
        id,
        email: email.to_owned(),
        password_hash: String::new(),
        full_name: "Test User".to_owned(),
        avatar_storage_key: None,
        phone: None,
        timezone: "UTC".to_owned(),
        status: UserStatus::Active,
        system_role: None,
        first_logged_in_at: Some(now),
        deactivated_at: None,
        created_at: now,
        updated_at: now,
    }
}

// --- AppState assembly ---------------------------------------------------------

/// Handles a test keeps after building state, to seed fakes the services read.
pub struct TestApp {
    pub state: AppState,
    pub users: Arc<FakeUsers>,
    pub groups: Arc<FakeGroups>,
}

/// Assembles a full [`AppState`] over in-memory fakes with the given rate-limit
/// ceilings. No network or filesystem is touched.
#[must_use]
pub fn test_app(rate_limits: RateLimits) -> TestApp {
    let users = Arc::new(FakeUsers::default());
    let groups = Arc::new(FakeGroups::default());
    let chats = Arc::new(FakeChats);
    let projects = Arc::new(FakeProjects);
    let requests = Arc::new(FakeRequests);

    let authz = Arc::new(FakeAuthz);
    let perms = Arc::new(Permissions::new(users.clone(), groups.clone(), authz));
    let events = Arc::new(EventBus::new(Arc::new(FakePublisher), Arc::new(FakeJobs)));

    let publisher: Arc<dyn EventPublisher> = Arc::new(FakePublisher);
    let realtime = Realtime::new(publisher, "redis://invalid.test");
    let storage = Arc::new(LocalStorage::new(
        std::env::temp_dir().join("portal-test-uploads"),
        "/files",
    ));
    let audit: Arc<dyn AuditRepository> = Arc::new(FakeAudit);
    let presence: Arc<dyn Presence> = Arc::new(FakePresence);
    let rate_limiter: Arc<dyn RateLimit> = Arc::new(FakeRateLimit::default());

    let state = AppState {
        user: Arc::new(UserService::new(
            users.clone(),
            groups.clone(),
            requests.clone(),
            chats.clone(),
            perms.clone(),
            events.clone(),
        )),
        group: Arc::new(GroupService::new(
            groups.clone(),
            projects.clone(),
            chats.clone(),
            perms.clone(),
            events.clone(),
        )),
        project: Arc::new(ProjectService::new(
            projects.clone(),
            requests.clone(),
            perms.clone(),
            events.clone(),
        )),
        request: Arc::new(RequestService::new(
            requests,
            projects,
            groups.clone(),
            storage.clone(),
            perms.clone(),
            events.clone(),
        )),
        ticket: Arc::new(TicketService::new(
            Arc::new(FakeTickets),
            perms.clone(),
            events.clone(),
        )),
        chat: Arc::new(ChatService::new(
            chats,
            users.clone(),
            perms.clone(),
            events.clone(),
        )),
        announcement: Arc::new(AnnouncementService::new(
            Arc::new(FakeChats),
            perms.clone(),
            events.clone(),
        )),
        notification: Arc::new(NotificationService::new(Arc::new(FakeNotifications), perms)),
        token: Arc::new(TokenService::new("test-secret", 3600, false)),
        realtime,
        audit,
        presence,
        rate_limiter,
        rate_limits,
        storage,
    };

    TestApp {
        state,
        users,
        groups,
    }
}

/// [`test_app`] with ceilings high enough that the limiter never trips.
#[must_use]
pub fn default_test_app() -> TestApp {
    test_app(RateLimits {
        auth: 1000,
        api: 1000,
    })
}
