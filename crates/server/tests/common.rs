//! In-memory test doubles and an [`AppState`] builder shared by the server's
//! integration tests. The fakes drive the real [`server::app::router`] without
//! standing up Postgres, Scylla, Redis, or `OpenFGA`.
//!
//! The fakes implement the `domain` traits with trivial in-memory behaviour;
//! untested-route methods return empty/`Ok` results. Repository shapes mirror the
//! application-layer fakes in `application/tests/authz.rs`.

#![allow(dead_code)]

use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use async_trait::async_trait;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use application::{
    events::EventBus,
    permissions::Permissions,
    resilience::HealthRegistry,
    service::{
        AnnouncementService, AuditService, ChatIngest, ChatIngestConfig, ChatService,
        CommentService, DailyReportService, DayOffService, FlexHoursService, GroupService,
        HolidayService, LeaveBalanceService, NotificationService, OvertimeService, PolicyProvider,
        PolicyService, ProjectService, ReportService, RequestService, TicketService, UserService,
    },
};
use domain::{
    error::{AuthzError, EventError, JobError, RenderError, RepositoryError},
    health::BackendId,
    ids::{
        ChannelId, CommentId, DailyReportId, DayOffId, FlexHoursId, GroupId, LeaveGrantId,
        MessageId, NotificationId, OvertimeId, ProjectCollaboratorId, ProjectId, ProjectInviteId,
        ReportId, RequestId, TicketId, UserId,
    },
    model::{
        Announcement, AttendancePolicy, AuditLog, Channel, ChannelKind, ChannelMembership,
        ChatAttachment, Comment, CommentEntity, CompanyStaffStats, DailyReport, DayOff, FlexHours,
        Group, GroupProjectStats, GroupRequestStats, GroupStaffStats, Holiday, LeaveGrant,
        LeaveTransaction, Membership, Message, MonthlyBucket, MonthlyReportData, Notification,
        Overtime, Period, Project, ProjectCollaborator, ProjectInvite, Report, ReportKind, Request,
        RequestAttachment, RequestStatus, StaffMonthlyStats, Ticket, TicketStats, User, UserStatus,
        YearlyReportData,
    },
    ports::{
        authz_client::{AuthzClient, RelationTuple},
        event_publisher::EventPublisher,
        job_queue::JobQueue,
        presence::Presence,
        rate_limit::RateLimit,
        report_renderer::ReportRenderer,
        token_revocation::TokenRevocation,
    },
    repository::{
        AuditRepository, ChatAttachmentRepository, ChatRepository, CommentRepository,
        DailyReportRepository, DayOffRepository, FlexHoursRepository, GroupRepository,
        HolidayRepository, LeaveBalanceRepository, NotificationRepository, OvertimeRepository,
        PolicyRepository, ProjectRepository, ReportArchiveRepository, ReportStatsRepository,
        RequestRepository, TicketRepository, UserRepository,
    },
};
use infrastructure::{local_storage::LocalStorage, signed_url::SignedUrl};

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
    async fn list_for_owner_group(
        &self,
        _g: GroupId,
        _q: Option<&str>,
    ) -> Result<Vec<Project>, RepositoryError> {
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

struct FakeComments;

#[async_trait]
impl CommentRepository for FakeComments {
    async fn find_by_id(
        &self,
        _entity: CommentEntity,
        _id: CommentId,
    ) -> Result<Option<Comment>, RepositoryError> {
        Ok(None)
    }
    async fn list_for_entity(
        &self,
        _entity: CommentEntity,
        _before: Option<CommentId>,
        _limit: u32,
    ) -> Result<Vec<Comment>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn save(&self, _comment: &Comment) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn delete(&self, _entity: CommentEntity, _id: CommentId) -> Result<(), RepositoryError> {
        Ok(())
    }
}

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
    async fn list_recent(
        &self,
        _limit: u32,
        _before: Option<OffsetDateTime>,
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

/// In-memory token revocation: a jti denylist plus per-user versions, so tests
/// can exercise logout replay and version-bump invalidation.
#[derive(Default)]
pub struct FakeRevocation {
    pub revoked: Mutex<HashSet<Uuid>>,
    pub versions: Mutex<HashMap<UserId, u64>>,
}

#[async_trait]
impl TokenRevocation for FakeRevocation {
    async fn revoke(&self, jti: Uuid, _ttl_secs: u64) -> Result<(), RepositoryError> {
        self.revoked.lock().unwrap().insert(jti);
        Ok(())
    }
    async fn is_revoked(&self, jti: Uuid) -> Result<bool, RepositoryError> {
        Ok(self.revoked.lock().unwrap().contains(&jti))
    }
    async fn version(&self, user: UserId) -> Result<u64, RepositoryError> {
        Ok(self
            .versions
            .lock()
            .unwrap()
            .get(&user)
            .copied()
            .unwrap_or(0))
    }
    async fn bump_version(&self, user: UserId) -> Result<u64, RepositoryError> {
        let mut versions = self.versions.lock().unwrap();
        let v = versions.entry(user).or_insert(0);
        *v += 1;
        Ok(*v)
    }
}

// --- reporting fakes -----------------------------------------------------------

struct FakeReportStats;

#[async_trait]
impl ReportStatsRepository for FakeReportStats {
    async fn project_stats_by_group(
        &self,
        _period: Period,
        _stuck_days: i32,
    ) -> Result<Vec<GroupProjectStats>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn request_stats_by_group(
        &self,
        _period: Period,
    ) -> Result<Vec<GroupRequestStats>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn ticket_stats(&self, _period: Period) -> Result<TicketStats, RepositoryError> {
        Ok(TicketStats {
            created_in_period: 0,
            resolved_in_period: 0,
            by_status: Vec::new(),
            by_category: Vec::new(),
            avg_resolve_hours: None,
        })
    }
    async fn staff_stats_by_group(
        &self,
        _period: Period,
    ) -> Result<Vec<GroupStaffStats>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn company_staff_stats(
        &self,
        _period: Period,
    ) -> Result<CompanyStaffStats, RepositoryError> {
        Ok(CompanyStaffStats {
            active_users: 0,
            new_active_users: 0,
            deactivated_users: 0,
        })
    }
    async fn monthly_growth(&self, _year: i32) -> Result<Vec<MonthlyBucket>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn staff_monthly_stats(
        &self,
        _user: UserId,
        _period: Period,
    ) -> Result<StaffMonthlyStats, RepositoryError> {
        Ok(StaffMonthlyStats {
            days_reported: 0,
            hours_request_work: 0.0,
            hours_learning: 0.0,
            hours_other: 0.0,
            leave_days_by_kind: Vec::new(),
            overtime_hours: 0.0,
            flex_days: 0,
            balance_expiring_soon: 0.0,
            requests_completed: 0,
            requests_open: 0,
            avg_request_progress: 0,
        })
    }
}

// --- attendance fakes ----------------------------------------------------------

struct FakePolicy;

#[async_trait]
impl PolicyRepository for FakePolicy {
    async fn load(&self) -> Result<AttendancePolicy, RepositoryError> {
        Ok(AttendancePolicy::default())
    }
    async fn save(
        &self,
        _policy: &AttendancePolicy,
        _updated_by: UserId,
    ) -> Result<(), RepositoryError> {
        Ok(())
    }
}

struct FakeDailyReports;

#[async_trait]
impl DailyReportRepository for FakeDailyReports {
    async fn find_by_id(&self, _id: DailyReportId) -> Result<Option<DailyReport>, RepositoryError> {
        Ok(None)
    }
    async fn find_by_user_date(
        &self,
        _user: UserId,
        _date: Date,
    ) -> Result<Option<DailyReport>, RepositoryError> {
        Ok(None)
    }
    async fn list_for_user(
        &self,
        _user: UserId,
        _from: Date,
        _to: Date,
    ) -> Result<Vec<DailyReport>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn list_for_group(
        &self,
        _group: GroupId,
        _from: Date,
        _to: Date,
    ) -> Result<Vec<DailyReport>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn save(&self, _report: &DailyReport) -> Result<(), RepositoryError> {
        Ok(())
    }
}

struct FakeHolidays;

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

struct FakeLeaveBalance;

#[async_trait]
impl LeaveBalanceRepository for FakeLeaveBalance {
    async fn list_grants(&self, _user: UserId) -> Result<Vec<LeaveGrant>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn available(&self, _user: UserId, _asof: Date) -> Result<f64, RepositoryError> {
        Ok(0.0)
    }
    async fn upsert_grant(&self, _grant: &LeaveGrant) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn apply(
        &self,
        _grant_deltas: &[(LeaveGrantId, f64)],
        _txns: &[LeaveTransaction],
    ) -> Result<(), RepositoryError> {
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
        _dayoff_id: DayOffId,
    ) -> Result<Vec<LeaveTransaction>, RepositoryError> {
        Ok(Vec::new())
    }
}

struct FakeDayOff;

#[async_trait]
impl DayOffRepository for FakeDayOff {
    async fn find_by_id(&self, _id: DayOffId) -> Result<Option<DayOff>, RepositoryError> {
        Ok(None)
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
        _month: u32,
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
    async fn save(&self, _day_off: &DayOff) -> Result<(), RepositoryError> {
        Ok(())
    }
}

struct FakeOvertime;

#[async_trait]
impl OvertimeRepository for FakeOvertime {
    async fn find_by_id(&self, _id: OvertimeId) -> Result<Option<Overtime>, RepositoryError> {
        Ok(None)
    }
    async fn list_for_user(
        &self,
        _user: UserId,
        _from: Date,
        _to: Date,
    ) -> Result<Vec<Overtime>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn approved_hours_in_month(
        &self,
        _user: UserId,
        _year: i32,
        _month: u32,
    ) -> Result<f64, RepositoryError> {
        Ok(0.0)
    }
    async fn list_pending_for_leader(
        &self,
        _group: GroupId,
    ) -> Result<Vec<Overtime>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn list_pending_for_hr(&self) -> Result<Vec<Overtime>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn save(&self, _overtime: &Overtime) -> Result<(), RepositoryError> {
        Ok(())
    }
}

struct FakeFlexHours;

#[async_trait]
impl FlexHoursRepository for FakeFlexHours {
    async fn find_by_id(&self, _id: FlexHoursId) -> Result<Option<FlexHours>, RepositoryError> {
        Ok(None)
    }
    async fn find_by_user_date(
        &self,
        _user: UserId,
        _date: Date,
    ) -> Result<Option<FlexHours>, RepositoryError> {
        Ok(None)
    }
    async fn list_for_user(
        &self,
        _user: UserId,
        _from: Date,
        _to: Date,
    ) -> Result<Vec<FlexHours>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn approved_count_in_month(
        &self,
        _user: UserId,
        _year: i32,
        _month: u32,
    ) -> Result<u32, RepositoryError> {
        Ok(0)
    }
    async fn approved_hours_in_month(
        &self,
        _user: UserId,
        _year: i32,
        _month: u32,
    ) -> Result<f64, RepositoryError> {
        Ok(0.0)
    }
    async fn list_pending_for_leader(
        &self,
        _group: GroupId,
    ) -> Result<Vec<FlexHours>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn users_with_approved_flex_in_month(
        &self,
        _year: i32,
        _month: u32,
    ) -> Result<Vec<UserId>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn save(&self, _flex: &FlexHours) -> Result<(), RepositoryError> {
        Ok(())
    }
}

struct FakeReportArchive;

#[async_trait]
impl ReportArchiveRepository for FakeReportArchive {
    async fn insert(&self, _report: &Report) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn list(&self, _limit: u32) -> Result<Vec<Report>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn find_by_id(&self, _id: ReportId) -> Result<Option<Report>, RepositoryError> {
        Ok(None)
    }
    async fn find_by_period(
        &self,
        _kind: ReportKind,
        _period_start: OffsetDateTime,
    ) -> Result<Option<Report>, RepositoryError> {
        Ok(None)
    }
    async fn list_all_storage_keys(&self) -> Result<Vec<String>, RepositoryError> {
        Ok(Vec::new())
    }
}

struct FakeRenderer;

impl ReportRenderer for FakeRenderer {
    fn render_monthly(&self, _data: &MonthlyReportData) -> Result<Vec<u8>, RenderError> {
        Ok(Vec::new())
    }
    fn render_yearly(&self, _data: &YearlyReportData) -> Result<Vec<u8>, RenderError> {
        Ok(Vec::new())
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
    pub revocation: Arc<FakeRevocation>,
}

/// Assembles a full [`AppState`] over in-memory fakes with the given rate-limit
/// ceilings. No network or filesystem is touched.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn test_app(rate_limits: RateLimits) -> TestApp {
    let users = Arc::new(FakeUsers::default());
    let groups = Arc::new(FakeGroups::default());
    let chats = Arc::new(FakeChats);
    let projects = Arc::new(FakeProjects);
    let requests = Arc::new(FakeRequests);

    let authz = Arc::new(FakeAuthz);
    let perms = Arc::new(Permissions::new(users.clone(), groups.clone(), authz));
    let events = Arc::new(EventBus::new(
        Arc::new(FakePublisher),
        Arc::new(FakeJobs),
        Arc::new(FakeJobs),
    ));

    let publisher: Arc<dyn EventPublisher> = Arc::new(FakePublisher);
    let realtime = Realtime::new(publisher, "redis://invalid.test");
    let signed_url = Arc::new(SignedUrl::new(b"test-secret"));
    let storage = Arc::new(LocalStorage::new(
        std::env::temp_dir().join("portal-test-uploads"),
        "/files",
        signed_url.clone(),
    ));
    let audit_repo: Arc<dyn AuditRepository> = Arc::new(FakeAudit);
    let audit_service = Arc::new(AuditService::new(audit_repo, perms.clone()));
    let presence: Arc<dyn Presence> = Arc::new(FakePresence);
    let rate_limiter: Arc<dyn RateLimit> = Arc::new(FakeRateLimit::default());
    let revocation = Arc::new(FakeRevocation::default());

    // Chat service + its ingest buffer. The drain loop is not spawned here: no
    // harness test drives the WS enqueue path, so the buffer only needs to exist.
    let chat = Arc::new(ChatService::new(
        chats.clone(),
        users.clone(),
        Arc::new(FakeChatAttachments),
        storage.clone(),
        perms.clone(),
        events.clone(),
    ));
    let (chat_ingest, _chat_ingest_rx) = ChatIngest::new(
        chat.clone(),
        chats.clone(),
        events.clone(),
        None,
        ChatIngestConfig::default(),
    );

    // Attendance policy: a fixed default snapshot; no repository load in tests.
    let policy_repo: Arc<dyn PolicyRepository> = Arc::new(FakePolicy);
    let policy_provider = Arc::new(PolicyProvider::new(AttendancePolicy::default()));

    // Hoisted so the daily-report service can reuse it.
    let request_service = Arc::new(RequestService::new(
        requests.clone(),
        projects.clone(),
        groups.clone(),
        storage.clone(),
        perms.clone(),
        events.clone(),
    ));
    let daily_report_repo: Arc<dyn DailyReportRepository> = Arc::new(FakeDailyReports);

    // Leave subsystem, wired like app::build: leave depends on the day-off repo,
    // day-off depends on the leave service.
    let holiday_repo: Arc<dyn HolidayRepository> = Arc::new(FakeHolidays);
    let leave_repo: Arc<dyn LeaveBalanceRepository> = Arc::new(FakeLeaveBalance);
    let day_off_repo: Arc<dyn DayOffRepository> = Arc::new(FakeDayOff);
    let holiday_service = Arc::new(HolidayService::new(holiday_repo.clone(), perms.clone()));
    let leave_service = Arc::new(LeaveBalanceService::new(
        leave_repo,
        holiday_repo.clone(),
        day_off_repo.clone(),
        policy_provider.clone(),
        perms.clone(),
        events.clone(),
    ));
    let day_off_service = Arc::new(DayOffService::new(
        day_off_repo,
        holiday_repo,
        leave_service.clone(),
        perms.clone(),
        events.clone(),
    ));

    let overtime_repo: Arc<dyn OvertimeRepository> = Arc::new(FakeOvertime);
    let overtime_service = Arc::new(OvertimeService::new(
        overtime_repo,
        policy_provider.clone(),
        perms.clone(),
        events.clone(),
    ));

    let flex_repo: Arc<dyn FlexHoursRepository> = Arc::new(FakeFlexHours);
    let flex_service = Arc::new(FlexHoursService::new(
        flex_repo,
        policy_provider.clone(),
        perms.clone(),
        events.clone(),
    ));

    let state = AppState {
        user: Arc::new(UserService::new(
            users.clone(),
            groups.clone(),
            requests.clone(),
            chats.clone(),
            perms.clone(),
            events.clone(),
            revocation.clone(),
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
        request: request_service.clone(),
        ticket: Arc::new(TicketService::new(
            Arc::new(FakeTickets),
            perms.clone(),
            events.clone(),
        )),
        chat,
        chat_ingest,
        comment: Arc::new(CommentService::new(
            Arc::new(FakeComments),
            requests.clone(),
            Arc::new(FakeTickets),
            perms.clone(),
            events.clone(),
        )),
        announcement: Arc::new(AnnouncementService::new(
            Arc::new(FakeChats),
            perms.clone(),
            events.clone(),
        )),
        notification: Arc::new(NotificationService::new(
            Arc::new(FakeNotifications),
            perms.clone(),
        )),
        report: Arc::new(ReportService::new(
            Arc::new(FakeReportStats),
            Arc::new(FakeReportArchive),
            Arc::new(FakeRenderer),
            storage.clone(),
            users.clone(),
            leave_service.clone(),
            flex_service.clone(),
            perms.clone(),
        )),
        policy: Arc::new(PolicyService::new(
            policy_repo.clone(),
            policy_provider.clone(),
            perms.clone(),
            events.clone(),
        )),
        daily_report: Arc::new(DailyReportService::new(
            daily_report_repo,
            groups.clone(),
            request_service.clone(),
            perms.clone(),
            events.clone(),
        )),
        holiday: holiday_service,
        leave: leave_service,
        day_off: day_off_service,
        overtime: overtime_service,
        flex: flex_service,
        perms,
        token: Arc::new(TokenService::new("test-secret", 3600, false)),
        revocation: revocation.clone(),
        realtime,
        audit_service,
        presence,
        rate_limiter,
        rate_limits,
        storage,
        signed_url,
        health: Arc::new(HealthRegistry::new(&BackendId::ALL)),
    };

    TestApp {
        state,
        users,
        groups,
        revocation,
    }
}

/// [`test_app`] with ceilings high enough that the limiter never trips.
#[must_use]
pub fn default_test_app() -> TestApp {
    test_app(RateLimits {
        auth: 1000,
        api: 1000,
        chat: 1000,
    })
}
