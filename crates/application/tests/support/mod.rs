//! In-memory test doubles shared by the application integration tests
//! (`authz.rs`, `repair.rs`). The authz client records every tuple write and
//! delete so tests can assert on them, and can simulate an FGA outage.

#![allow(dead_code)]

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use application::{EventBus, Repair};
use async_trait::async_trait;
use domain::{
    error::{
        AuthzError, EventError, JobError, RepositoryError, StorageError, TokenRevocationError,
    },
    ids::{
        ChannelId, DayOffId, GroupId, LeaveGrantId, MembershipId, MessageId, ProjectCollaboratorId,
        ProjectId, ProjectInviteId, RequestId, TicketId, UserId,
    },
    model::{
        Announcement, Channel, ChannelKind, ChannelMembership, ChatAttachment, DayOff, Group,
        GroupKind, GroupRole, Holiday, LeaveGrant, LeaveTransaction, Membership, Message, Project,
        ProjectCollaborator, ProjectInvite, Request, RequestAttachment, RequestStatus, SystemRole,
        Ticket, User, UserStatus,
    },
    ports::{
        authz_client::{AuthzClient, RelationTuple},
        event_publisher::EventPublisher,
        file_storage::{FileStorage, StorageObject},
        job_queue::JobQueue,
        token_revocation::TokenRevocation,
    },
    repository::{
        ChatAttachmentRepository, ChatRepository, DayOffRepository, GroupRepository,
        HolidayRepository, LeaveBalanceRepository, OutboxRecord, ProjectRepository,
        RequestRepository, TicketRepository, UserRepository,
    },
};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

// --- recording fake authz client ----------------------------------------------

pub type Tuple = (String, String, String);

/// Records writes and deletes; `fail_writes` simulates an FGA outage (every
/// mutation errors, checks still answer).
#[derive(Default)]
pub struct FakeAuthz {
    pub writes: Mutex<Vec<Tuple>>,
    pub deletes: Mutex<Vec<Tuple>>,
    pub fail_writes: AtomicBool,
}

impl FakeAuthz {
    pub fn writes(&self) -> Vec<Tuple> {
        self.writes.lock().unwrap().clone()
    }
    pub fn deletes(&self) -> Vec<Tuple> {
        self.deletes.lock().unwrap().clone()
    }
    fn record(&self, log: &Mutex<Vec<Tuple>>, subject: &str, relation: &str, object: &str) {
        log.lock()
            .unwrap()
            .push((subject.into(), relation.into(), object.into()));
    }
    fn gate(&self) -> Result<(), AuthzError> {
        if self.fail_writes.load(Ordering::SeqCst) {
            Err(AuthzError::Backend("fga down".into()))
        } else {
            Ok(())
        }
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
    async fn check_subject(
        &self,
        _subject: &str,
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
        self.gate()?;
        self.record(&self.writes, subject, relation, object);
        Ok(())
    }
    async fn delete_tuple(
        &self,
        subject: &str,
        relation: &str,
        object: &str,
    ) -> Result<(), AuthzError> {
        self.gate()?;
        self.record(&self.deletes, subject, relation, object);
        Ok(())
    }
    async fn write_tuples(
        &self,
        writes: &[RelationTuple],
        deletes: &[RelationTuple],
    ) -> Result<(), AuthzError> {
        self.gate()?;
        for t in writes {
            self.record(&self.writes, &t.subject, &t.relation, &t.object);
        }
        for t in deletes {
            self.record(&self.deletes, &t.subject, &t.relation, &t.object);
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

// --- fake repositories (only the exercised methods hold state) ------------------

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
    async fn find_by_ids(&self, ids: &[UserId]) -> Result<Vec<User>, RepositoryError> {
        Ok(self
            .users
            .lock()
            .unwrap()
            .iter()
            .filter(|u| ids.contains(&u.id))
            .cloned()
            .collect())
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
    async fn save(&self, user: &User, _outbox: &[OutboxRecord]) -> Result<(), RepositoryError> {
        let mut v = self.users.lock().unwrap();
        if let Some(existing) = v.iter_mut().find(|u| u.id == user.id) {
            *existing = user.clone();
        } else {
            v.push(user.clone());
        }
        Ok(())
    }
    async fn list_avatar_keys(&self) -> Result<Vec<String>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn list_with_system_role(&self) -> Result<Vec<User>, RepositoryError> {
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
    async fn find_by_ids(&self, ids: &[GroupId]) -> Result<Vec<Group>, RepositoryError> {
        Ok(self
            .groups
            .lock()
            .unwrap()
            .iter()
            .filter(|g| ids.contains(&g.id))
            .cloned()
            .collect())
    }
    async fn list_all(&self) -> Result<Vec<Group>, RepositoryError> {
        // Mirrors the Postgres repo: archived groups are hidden.
        Ok(self
            .groups
            .lock()
            .unwrap()
            .iter()
            .filter(|g| g.archived_at.is_none())
            .cloned()
            .collect())
    }
    async fn find_it_group(&self) -> Result<Option<Group>, RepositoryError> {
        Ok(self.it_group.lock().unwrap().clone())
    }
    async fn save_group(
        &self,
        group: &Group,
        _outbox: &[OutboxRecord],
    ) -> Result<(), RepositoryError> {
        let mut v = self.groups.lock().unwrap();
        if let Some(existing) = v.iter_mut().find(|g| g.id == group.id) {
            *existing = group.clone();
        } else {
            v.push(group.clone());
        }
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
    async fn save_membership(
        &self,
        membership: &Membership,
        _outbox: &[OutboxRecord],
    ) -> Result<(), RepositoryError> {
        let mut v = self.memberships.lock().unwrap();
        if let Some(existing) = v.iter_mut().find(|m| m.id == membership.id) {
            *existing = membership.clone();
        } else {
            v.push(membership.clone());
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct FakeProjects {
    pub projects: Mutex<Vec<Project>>,
    pub collaborators: Mutex<Vec<ProjectCollaborator>>,
    pub invites: Mutex<Vec<ProjectInvite>>,
    pub deleted_collaborators: Mutex<Vec<ProjectCollaboratorId>>,
}

#[async_trait]
impl ProjectRepository for FakeProjects {
    async fn find_by_id(&self, id: ProjectId) -> Result<Option<Project>, RepositoryError> {
        Ok(self
            .projects
            .lock()
            .unwrap()
            .iter()
            .find(|p| p.id == id)
            .cloned())
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
    async fn list_page(
        &self,
        _after: Option<ProjectId>,
        _limit: u32,
    ) -> Result<Vec<Project>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn save_project(
        &self,
        project: &Project,
        _outbox: &[OutboxRecord],
    ) -> Result<(), RepositoryError> {
        self.projects.lock().unwrap().push(project.clone());
        Ok(())
    }
    async fn list_collaborators(
        &self,
        project_id: ProjectId,
    ) -> Result<Vec<ProjectCollaborator>, RepositoryError> {
        Ok(self
            .collaborators
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.project_id == project_id)
            .cloned()
            .collect())
    }
    async fn save_collaborator(
        &self,
        collaborator: &ProjectCollaborator,
    ) -> Result<(), RepositoryError> {
        self.collaborators
            .lock()
            .unwrap()
            .push(collaborator.clone());
        Ok(())
    }
    async fn delete_collaborator(
        &self,
        id: ProjectCollaboratorId,
        _outbox: &[OutboxRecord],
    ) -> Result<(), RepositoryError> {
        self.collaborators.lock().unwrap().retain(|c| c.id != id);
        self.deleted_collaborators.lock().unwrap().push(id);
        Ok(())
    }
    async fn find_invite(
        &self,
        id: ProjectInviteId,
    ) -> Result<Option<ProjectInvite>, RepositoryError> {
        Ok(self
            .invites
            .lock()
            .unwrap()
            .iter()
            .find(|i| i.id == id)
            .cloned())
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
    async fn save_invite(
        &self,
        _invite: &ProjectInvite,
        _outbox: &[OutboxRecord],
    ) -> Result<(), RepositoryError> {
        Ok(())
    }
}

#[derive(Default)]
pub struct FakeRequests;

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
    async fn list_page(
        &self,
        _project: Option<ProjectId>,
        _after: Option<RequestId>,
        _limit: u32,
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
    async fn save(
        &self,
        _request: &Request,
        _outbox: &[OutboxRecord],
    ) -> Result<(), RepositoryError> {
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
pub struct FakeTickets {
    pub tickets: Mutex<Vec<Ticket>>,
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
    async fn save(&self, ticket: &Ticket, _outbox: &[OutboxRecord]) -> Result<(), RepositoryError> {
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
pub struct FakeChatAttachments;

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
pub struct FakeStorage;

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

/// Chat repo with just enough state for provisioning/subscription assertions.
#[derive(Default)]
pub struct FakeChats {
    pub group_channel: Mutex<Option<Channel>>,
    pub general_channel: Mutex<Option<Channel>>,
    pub direct_channel: Mutex<Option<Channel>>,
    pub subscribes: Mutex<Vec<(UserId, ChannelId, ChannelKind)>>,
    pub unsubscribes: Mutex<Vec<(UserId, ChannelId)>>,
}

impl FakeChats {
    pub fn subscribes(&self) -> Vec<(UserId, ChannelId, ChannelKind)> {
        self.subscribes.lock().unwrap().clone()
    }
    pub fn unsubscribes(&self) -> Vec<(UserId, ChannelId)> {
        self.unsubscribes.lock().unwrap().clone()
    }
}

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
        Ok(self.direct_channel.lock().unwrap().clone())
    }
    async fn save_channel(&self, _channel: &Channel) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn find_group_channel(
        &self,
        _group_id: GroupId,
    ) -> Result<Option<Channel>, RepositoryError> {
        Ok(self.group_channel.lock().unwrap().clone())
    }
    async fn find_general_channel(&self) -> Result<Option<Channel>, RepositoryError> {
        Ok(self.general_channel.lock().unwrap().clone())
    }
    async fn subscribe_member(
        &self,
        user_id: UserId,
        channel_id: ChannelId,
        kind: ChannelKind,
    ) -> Result<(), RepositoryError> {
        self.subscribes
            .lock()
            .unwrap()
            .push((user_id, channel_id, kind));
        Ok(())
    }
    async fn unsubscribe_member(
        &self,
        user_id: UserId,
        channel_id: ChannelId,
    ) -> Result<(), RepositoryError> {
        self.unsubscribes
            .lock()
            .unwrap()
            .push((user_id, channel_id));
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
        _limit: u32,
    ) -> Result<Vec<Announcement>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn save_announcement_with_message(
        &self,
        _announcement: &Announcement,
        _message: &Message,
    ) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn delete_announcement_with_message(
        &self,
        _channel_id: ChannelId,
        _message_id: MessageId,
    ) -> Result<(), RepositoryError> {
        Ok(())
    }
}

#[derive(Default)]
pub struct FakePublisher;

#[async_trait]
impl EventPublisher for FakePublisher {
    async fn publish(&self, _topic: &str, _payload: &[u8]) -> Result<(), EventError> {
        Ok(())
    }
}

#[derive(Default)]
pub struct FakeJobs;

#[async_trait]
impl JobQueue for FakeJobs {
    async fn enqueue(&self, _queue: &str, _payload: &[u8]) -> Result<(), JobError> {
        Ok(())
    }
}

/// Job queue that records every enqueue, for repair-path assertions.
#[derive(Default)]
pub struct RecordingJobs {
    pub jobs: Mutex<Vec<(String, Vec<u8>)>>,
}

impl RecordingJobs {
    /// Payloads enqueued on `queue`.
    pub fn on(&self, queue: &str) -> Vec<Vec<u8>> {
        self.jobs
            .lock()
            .unwrap()
            .iter()
            .filter(|(q, _)| q == queue)
            .map(|(_, p)| p.clone())
            .collect()
    }
}

#[async_trait]
impl JobQueue for RecordingJobs {
    async fn enqueue(&self, queue: &str, payload: &[u8]) -> Result<(), JobError> {
        self.jobs
            .lock()
            .unwrap()
            .push((queue.to_owned(), payload.to_vec()));
        Ok(())
    }
}

/// Token-revocation fake with an outage switch and a bump counter.
#[derive(Default)]
pub struct FakeRevocation {
    pub fail: AtomicBool,
    pub bumps: AtomicU64,
}

#[async_trait]
impl TokenRevocation for FakeRevocation {
    async fn revoke(
        &self,
        _jti: Uuid,
        _ttl: std::time::Duration,
    ) -> Result<(), TokenRevocationError> {
        Ok(())
    }
    async fn is_revoked(&self, _jti: Uuid) -> Result<bool, TokenRevocationError> {
        Ok(false)
    }
    async fn version(&self, _user: UserId) -> Result<u64, TokenRevocationError> {
        Ok(0)
    }
    async fn bump_version(&self, _user: UserId) -> Result<u64, TokenRevocationError> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(TokenRevocationError::Backend("redis down".into()));
        }
        Ok(self.bumps.fetch_add(1, Ordering::SeqCst) + 1)
    }
}

/// Leave-balance repo holding grants and a ledger; `apply` records each call so
/// tests can assert consume/refund behaviour.
#[derive(Default)]
pub struct FakeLeave {
    pub grants: Mutex<Vec<LeaveGrant>>,
    pub txns: Mutex<Vec<LeaveTransaction>>,
    pub applies: Mutex<Vec<Vec<LeaveTransaction>>>,
}

#[async_trait]
impl LeaveBalanceRepository for FakeLeave {
    async fn list_grants(&self, _user: UserId) -> Result<Vec<LeaveGrant>, RepositoryError> {
        Ok(self.grants.lock().unwrap().clone())
    }
    async fn available(&self, _user: UserId, _asof: Date) -> Result<f64, RepositoryError> {
        Ok(self
            .grants
            .lock()
            .unwrap()
            .iter()
            .map(|g| g.days_remaining)
            .sum())
    }
    async fn upsert_grant_with_txn(
        &self,
        _grant: &LeaveGrant,
        _txn: Option<&LeaveTransaction>,
    ) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn apply(
        &self,
        _grant_deltas: &[(LeaveGrantId, f64)],
        txns: &[LeaveTransaction],
    ) -> Result<(), RepositoryError> {
        self.applies.lock().unwrap().push(txns.to_vec());
        self.txns.lock().unwrap().extend_from_slice(txns);
        Ok(())
    }
    async fn list_expiring(
        &self,
        _asof: Date,
        _within_days: i64,
    ) -> Result<Vec<LeaveGrant>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn list_transactions(
        &self,
        _user: UserId,
        _from: Date,
        _to: Date,
    ) -> Result<Vec<LeaveTransaction>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn transactions_for_dayoff(
        &self,
        dayoff_id: DayOffId,
    ) -> Result<Vec<LeaveTransaction>, RepositoryError> {
        Ok(self
            .txns
            .lock()
            .unwrap()
            .iter()
            .filter(|t| t.dayoff_id == Some(dayoff_id))
            .cloned()
            .collect())
    }
}

pub struct FakeHolidays;

#[async_trait]
impl HolidayRepository for FakeHolidays {
    async fn list(&self, _from: Date, _to: Date) -> Result<Vec<Holiday>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn upsert(&self, _date: Date, _name: &str) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn delete(&self, _date: Date) -> Result<(), RepositoryError> {
        Ok(())
    }
}

/// Day-off repo holding one request; `fail_save` simulates a datastore fault.
#[derive(Default)]
pub struct FakeDayOffs {
    pub dayoff: Mutex<Option<DayOff>>,
    pub fail_save: AtomicBool,
}

#[async_trait]
impl DayOffRepository for FakeDayOffs {
    async fn find_by_id(&self, id: DayOffId) -> Result<Option<DayOff>, RepositoryError> {
        Ok(self.dayoff.lock().unwrap().clone().filter(|d| d.id == id))
    }
    async fn list_for_user(
        &self,
        _user: UserId,
        _from: Date,
        _to: Date,
    ) -> Result<Vec<DayOff>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn approved_days_in_month(
        &self,
        _user: UserId,
        _year: i32,
        _month: u8,
    ) -> Result<f64, RepositoryError> {
        Ok(0.0)
    }
    async fn list_pending_for_leader(
        &self,
        _group: GroupId,
    ) -> Result<Vec<DayOff>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn list_pending_for_hr(&self) -> Result<Vec<DayOff>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn save(&self, day_off: &DayOff) -> Result<(), RepositoryError> {
        if self.fail_save.load(Ordering::SeqCst) {
            return Err(RepositoryError::Backend("pg down".into()));
        }
        *self.dayoff.lock().unwrap() = Some(day_off.clone());
        Ok(())
    }
}

// --- builders + assertions -----------------------------------------------------

pub fn user(system_role: Option<SystemRole>) -> User {
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
        email_notifications: true,
        first_logged_in_at: Some(now),
        deactivated_at: None,
        version: 0,
        created_at: now,
        updated_at: now,
    }
}

pub fn group(kind: GroupKind) -> Group {
    let now = OffsetDateTime::now_utc();
    Group {
        id: GroupId(Uuid::now_v7()),
        name: "G".into(),
        description: String::new(),
        kind,
        archived_at: None,
        version: 0,
        created_at: now,
        updated_at: now,
    }
}

pub fn membership(group_id: GroupId, user_id: UserId, role: GroupRole) -> Membership {
    let now = OffsetDateTime::now_utc();
    Membership {
        id: MembershipId(Uuid::now_v7()),
        group_id,
        user_id,
        role,
        joined_at: now,
        deactivated_at: None,
        version: 0,
        created_at: now,
        updated_at: now,
    }
}

pub fn events() -> Arc<EventBus> {
    Arc::new(EventBus::new(Arc::new(FakePublisher), Arc::new(FakeJobs)))
}

pub fn repair() -> Arc<Repair> {
    Arc::new(Repair::new(Arc::new(FakeJobs)))
}

pub fn has(writes: &[Tuple], subject: &str, relation: &str, object: &str) -> bool {
    writes
        .iter()
        .any(|(s, r, o)| s == subject && r == relation && o == object)
}
