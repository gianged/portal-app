//! Behavior guard for the partial-failure hardening: post-commit obligations
//! that fail inline must queue an idempotent repair instead of losing state,
//! and the worker-side reconciles must re-derive the desired state from the DB.

mod support;

use std::sync::{Arc, atomic::Ordering};

use application::{
    ChatService, DayOffService, GroupService, LeaveBalanceService, Permissions, PolicyProvider,
    ProjectService, Repair, RepairJob, RepairService, UserService,
    commands::{day_off::DecideDayOffCommand, project::CreateProjectCommand},
};
use domain::{
    ids::{
        ChannelId, DayOffId, GroupId, LeaveGrantId, LeaveTransactionId, ProjectCollaboratorId,
        ProjectId, TicketId, UserId,
    },
    model::{
        AttendancePolicy, Channel, DayOff, DayOffKind, DayOffStatus, DirectChannel, GroupChannel,
        GroupKind, GroupRole, LeaveGrant, LeaveTransaction, LeaveTxnKind, Project,
        ProjectCollaborator, ProjectStatus, SystemRole, UserStatus,
    },
    ports::job_queue::QUEUE_REPAIR,
    repository::GroupRepository,
};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use support::{
    FakeAuthz, FakeChatAttachments, FakeChats, FakeDayOffs, FakeGroups, FakeHolidays, FakeLeave,
    FakeProjects, FakeRequests, FakeRevocation, FakeStorage, FakeTickets, FakeUsers, RecordingJobs,
    events, group, has, membership, user,
};

fn decode(payload: &[u8]) -> RepairJob {
    serde_json::from_slice(payload).expect("payload decodes as RepairJob")
}

fn policy() -> Arc<PolicyProvider> {
    Arc::new(PolicyProvider::new(AttendancePolicy::default()))
}

fn leave_service(
    leave: Arc<FakeLeave>,
    dayoffs: Arc<FakeDayOffs>,
    perms: Arc<Permissions>,
) -> Arc<LeaveBalanceService> {
    Arc::new(LeaveBalanceService::new(
        leave,
        Arc::new(FakeHolidays),
        dayoffs,
        policy(),
        perms,
        events(),
    ))
}

fn grant(user_id: UserId, remaining: f64) -> LeaveGrant {
    let now = OffsetDateTime::now_utc();
    LeaveGrant {
        id: LeaveGrantId(Uuid::now_v7()),
        user_id,
        grant_year: 2026,
        days_granted: 12.0,
        days_remaining: remaining,
        expires_on: (now + Duration::days(365)).date(),
        created_by: None,
        created_at: now,
        updated_at: now,
    }
}

fn day_off(user_id: UserId, status: DayOffStatus) -> DayOff {
    let now = OffsetDateTime::now_utc();
    DayOff {
        id: DayOffId(Uuid::now_v7()),
        requester_user_id: user_id,
        kind: DayOffKind::AnnualLeave,
        start_date: now.date(),
        end_date: now.date(),
        start_half: false,
        end_half: false,
        days: 1.0,
        reason: String::new(),
        status,
        leader_user_id: None,
        leader_decided_at: None,
        hr_user_id: None,
        hr_decided_at: None,
        decision_note: String::new(),
        created_at: now,
        updated_at: now,
    }
}

fn group_channel(group_id: GroupId) -> Channel {
    Channel::Group(GroupChannel {
        id: ChannelId(Uuid::now_v7()),
        group_id,
        name: "G".into(),
        created_at: OffsetDateTime::now_utc(),
    })
}

// --- T2: Permissions::sync_group_role ------------------------------------------

#[tokio::test]
async fn sync_group_role_rewrites_only_the_desired_tuple() {
    let member = user(None);
    let g = group(GroupKind::Standard);
    let authz = Arc::new(FakeAuthz::default());
    let perms = Permissions::new(
        Arc::new(FakeUsers::default()),
        Arc::new(FakeGroups::default()),
        authz.clone(),
    );

    perms
        .sync_group_role(member.id, g.id, Some(GroupRole::Leader))
        .await
        .unwrap();
    let subj = format!("user:{}", member.id.0);
    let obj = format!("group:{}", g.id.0);
    assert!(has(&authz.writes(), &subj, "leader", &obj));
    assert!(has(&authz.deletes(), &subj, "sub_leader", &obj));
    assert!(has(&authz.deletes(), &subj, "direct_member", &obj));

    // Desired None deletes all three roles and writes nothing.
    let authz = Arc::new(FakeAuthz::default());
    let perms = Permissions::new(
        Arc::new(FakeUsers::default()),
        Arc::new(FakeGroups::default()),
        authz.clone(),
    );
    perms.sync_group_role(member.id, g.id, None).await.unwrap();
    assert!(authz.writes().is_empty());
    assert!(has(&authz.deletes(), &subj, "leader", &obj));
    assert!(has(&authz.deletes(), &subj, "sub_leader", &obj));
    assert!(has(&authz.deletes(), &subj, "direct_member", &obj));
}

// --- T3: the original bug -------------------------------------------------------

#[tokio::test]
async fn create_project_queues_repair_when_grant_fails() {
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
    authz.fail_writes.store(true, Ordering::SeqCst);
    let perms = Arc::new(Permissions::new(users, groups, authz));
    let projects = Arc::new(FakeProjects::default());
    let repair_jobs = Arc::new(RecordingJobs::default());
    let svc = ProjectService::new(
        projects.clone(),
        Arc::new(FakeRequests),
        perms,
        events(),
        Arc::new(Repair::new(repair_jobs.clone())),
    );

    let created = svc
        .create_project(
            leader.id,
            CreateProjectCommand {
                owner_group_id: owner.id,
                name: "P".into(),
                description: String::new(),
            },
        )
        .await
        .expect("create still succeeds");

    // Row committed despite the FGA outage, and the response flags the lag.
    assert!(
        projects
            .projects
            .lock()
            .unwrap()
            .iter()
            .any(|p| p.id == created.entity.id)
    );
    assert!(created.authz_pending);
    let queued = repair_jobs.on(QUEUE_REPAIR);
    assert_eq!(queued.len(), 1);
    assert!(
        matches!(decode(&queued[0]), RepairJob::SyncProjectTuples { project_id } if project_id == created.entity.id)
    );
}

// --- T4: deactivate teardown ----------------------------------------------------

#[tokio::test]
async fn deactivate_user_queues_repairs_when_teardown_fails() {
    let hr = user(Some(SystemRole::Hr));
    let target = user(Some(SystemRole::Director));

    let users = Arc::new(FakeUsers::default());
    users
        .users
        .lock()
        .unwrap()
        .extend([hr.clone(), target.clone()]);
    let groups = Arc::new(FakeGroups::default());
    let authz = Arc::new(FakeAuthz::default());
    authz.fail_writes.store(true, Ordering::SeqCst);
    let perms = Arc::new(Permissions::new(users.clone(), groups.clone(), authz));
    let repair_jobs = Arc::new(RecordingJobs::default());
    let revocation = Arc::new(FakeRevocation::default());
    revocation.fail.store(true, Ordering::SeqCst);
    let svc = UserService::new(
        users.clone(),
        groups,
        Arc::new(FakeRequests),
        Arc::new(FakeChats::default()),
        perms,
        events(),
        Arc::new(Repair::new(repair_jobs.clone())),
        revocation,
    );

    svc.deactivate_user(hr.id, target.id)
        .await
        .expect("deactivation commits despite failing obligations");

    let saved = users
        .users
        .lock()
        .unwrap()
        .iter()
        .find(|u| u.id == target.id)
        .cloned()
        .unwrap();
    assert_eq!(saved.status, UserStatus::Deactivated);
    let queued: Vec<RepairJob> = repair_jobs
        .on(QUEUE_REPAIR)
        .iter()
        .map(|p| decode(p))
        .collect();
    assert!(
        queued
            .iter()
            .any(|j| matches!(j, RepairJob::BumpSessions { user_id } if *user_id == target.id))
    );
    assert!(
        queued
            .iter()
            .any(|j| matches!(j, RepairJob::SyncUserAccess { user_id } if *user_id == target.id))
    );
}

// --- T5: collaborator removal never silently keeps access -----------------------

#[tokio::test]
async fn remove_collaborator_queues_repair_when_revoke_fails() {
    let leader = user(None);
    let owner = group(GroupKind::Standard);
    let collab_group = group(GroupKind::Standard);
    let now = OffsetDateTime::now_utc();

    let users = Arc::new(FakeUsers::default());
    let groups = Arc::new(FakeGroups::default());
    groups
        .memberships
        .lock()
        .unwrap()
        .push(membership(owner.id, leader.id, GroupRole::Leader));

    let projects = Arc::new(FakeProjects::default());
    let project = Project {
        id: ProjectId(Uuid::now_v7()),
        owner_group_id: owner.id,
        created_by_user_id: leader.id,
        name: "P".into(),
        description: String::new(),
        status: ProjectStatus::Active,
        progress: 0,
        completed_at: None,
        version: 0,
        created_at: now,
        updated_at: now,
    };
    projects.projects.lock().unwrap().push(project.clone());
    projects
        .collaborators
        .lock()
        .unwrap()
        .push(ProjectCollaborator {
            id: ProjectCollaboratorId(Uuid::now_v7()),
            project_id: project.id,
            group_id: collab_group.id,
            created_at: now,
            updated_at: now,
        });

    let authz = Arc::new(FakeAuthz::default());
    authz.fail_writes.store(true, Ordering::SeqCst);
    let perms = Arc::new(Permissions::new(users, groups, authz));
    let repair_jobs = Arc::new(RecordingJobs::default());
    let svc = ProjectService::new(
        projects.clone(),
        Arc::new(FakeRequests),
        perms,
        events(),
        Arc::new(Repair::new(repair_jobs.clone())),
    );

    svc.remove_collaborator(leader.id, project.id, collab_group.id)
        .await
        .expect("removal commits despite failing revoke");

    assert!(projects.collaborators.lock().unwrap().is_empty());
    let queued = repair_jobs.on(QUEUE_REPAIR);
    assert_eq!(queued.len(), 1);
    assert!(matches!(
        decode(&queued[0]),
        RepairJob::SyncCollaboratorTuple { project_id, group_id }
            if project_id == project.id && group_id == collab_group.id
    ));
}

// --- T6: day-off save failure after the balance debit ---------------------------

#[tokio::test]
async fn hr_decide_save_failure_queues_balance_reconcile() {
    let hr = user(Some(SystemRole::Hr));
    let requester = user(None);

    let users = Arc::new(FakeUsers::default());
    users
        .users
        .lock()
        .unwrap()
        .extend([hr.clone(), requester.clone()]);
    let perms = Arc::new(Permissions::new(
        users,
        Arc::new(FakeGroups::default()),
        Arc::new(FakeAuthz::default()),
    ));

    let dayoffs = Arc::new(FakeDayOffs::default());
    let pending = day_off(requester.id, DayOffStatus::LeaderApproved);
    *dayoffs.dayoff.lock().unwrap() = Some(pending.clone());
    dayoffs.fail_save.store(true, Ordering::SeqCst);

    let leave_repo = Arc::new(FakeLeave::default());
    leave_repo
        .grants
        .lock()
        .unwrap()
        .push(grant(requester.id, 5.0));

    let repair_jobs = Arc::new(RecordingJobs::default());
    let svc = DayOffService::new(
        dayoffs.clone(),
        Arc::new(FakeHolidays),
        leave_service(leave_repo.clone(), dayoffs.clone(), perms.clone()),
        perms,
        events(),
        Arc::new(Repair::new(repair_jobs.clone())),
    );

    let err = svc
        .hr_decide(
            hr.id,
            pending.id,
            DecideDayOffCommand {
                approve: true,
                note: String::new(),
            },
        )
        .await;
    assert!(err.is_err(), "save failure must surface");

    // The debit ran before the failed save...
    assert!(
        leave_repo
            .applies
            .lock()
            .unwrap()
            .iter()
            .flatten()
            .any(|t| t.kind == LeaveTxnKind::Consume)
    );
    // ...and the reconcile that will refund it is queued.
    let queued = repair_jobs.on(QUEUE_REPAIR);
    assert_eq!(queued.len(), 1);
    assert!(matches!(
        decode(&queued[0]),
        RepairJob::SyncDayOffBalance { dayoff_id } if dayoff_id == pending.id
    ));
}

// --- T7: worker-side reconciles -------------------------------------------------

fn repair_service(
    users: Arc<FakeUsers>,
    groups: Arc<FakeGroups>,
    chats: Arc<FakeChats>,
    dayoffs: Arc<FakeDayOffs>,
    leave: Arc<FakeLeave>,
    authz: Arc<FakeAuthz>,
) -> RepairService {
    let perms = Arc::new(Permissions::new(users.clone(), groups.clone(), authz));
    RepairService::new(
        users,
        groups,
        Arc::new(FakeProjects::default()),
        Arc::new(FakeTickets::default()),
        dayoffs.clone(),
        chats,
        perms.clone(),
        leave_service(leave, dayoffs, perms),
        Arc::new(FakeRevocation::default()),
    )
}

#[tokio::test]
async fn sync_membership_reconciles_role_and_subscription() {
    let member = user(None);
    let g = group(GroupKind::Standard);
    let groups = Arc::new(FakeGroups::default());
    groups
        .memberships
        .lock()
        .unwrap()
        .push(membership(g.id, member.id, GroupRole::Leader));
    let chats = Arc::new(FakeChats::default());
    let channel = group_channel(g.id);
    let channel_id = channel.id();
    *chats.group_channel.lock().unwrap() = Some(channel);
    let authz = Arc::new(FakeAuthz::default());

    let svc = repair_service(
        Arc::new(FakeUsers::default()),
        groups.clone(),
        chats.clone(),
        Arc::new(FakeDayOffs::default()),
        Arc::new(FakeLeave::default()),
        authz.clone(),
    );
    svc.handle(RepairJob::SyncMembership {
        group_id: g.id,
        user_id: member.id,
    })
    .await
    .unwrap();

    let subj = format!("user:{}", member.id.0);
    let obj = format!("group:{}", g.id.0);
    assert!(has(&authz.writes(), &subj, "leader", &obj));
    assert!(has(&authz.deletes(), &subj, "sub_leader", &obj));
    assert!(has(&authz.deletes(), &subj, "direct_member", &obj));
    assert!(
        chats
            .subscribes()
            .iter()
            .any(|(u, c, _)| *u == member.id && *c == channel_id)
    );

    // No membership row: all three roles deleted, member unsubscribed.
    groups.memberships.lock().unwrap().clear();
    let authz2 = Arc::new(FakeAuthz::default());
    let chats2 = Arc::new(FakeChats::default());
    *chats2.group_channel.lock().unwrap() = Some(group_channel(g.id));
    let svc = repair_service(
        Arc::new(FakeUsers::default()),
        groups,
        chats2.clone(),
        Arc::new(FakeDayOffs::default()),
        Arc::new(FakeLeave::default()),
        authz2.clone(),
    );
    svc.handle(RepairJob::SyncMembership {
        group_id: g.id,
        user_id: member.id,
    })
    .await
    .unwrap();
    assert!(authz2.writes().is_empty());
    assert!(has(&authz2.deletes(), &subj, "leader", &obj));
    assert!(chats2.unsubscribes().iter().any(|(u, _)| *u == member.id));
}

#[tokio::test]
async fn sync_day_off_balance_consumes_or_refunds_from_db_state() {
    let requester = user(None);

    // Approved annual leave with no Consume entry yet: reconcile consumes.
    let dayoffs = Arc::new(FakeDayOffs::default());
    let approved = day_off(requester.id, DayOffStatus::Approved);
    *dayoffs.dayoff.lock().unwrap() = Some(approved.clone());
    let leave = Arc::new(FakeLeave::default());
    leave.grants.lock().unwrap().push(grant(requester.id, 5.0));
    let svc = repair_service(
        Arc::new(FakeUsers::default()),
        Arc::new(FakeGroups::default()),
        Arc::new(FakeChats::default()),
        dayoffs,
        leave.clone(),
        Arc::new(FakeAuthz::default()),
    );
    svc.handle(RepairJob::SyncDayOffBalance {
        dayoff_id: approved.id,
    })
    .await
    .unwrap();
    assert!(
        leave
            .applies
            .lock()
            .unwrap()
            .iter()
            .flatten()
            .any(|t| t.kind == LeaveTxnKind::Consume)
    );

    // Cancelled after a consume: reconcile refunds the net amount.
    let dayoffs = Arc::new(FakeDayOffs::default());
    let cancelled = day_off(requester.id, DayOffStatus::Cancelled);
    *dayoffs.dayoff.lock().unwrap() = Some(cancelled.clone());
    let leave = Arc::new(FakeLeave::default());
    let g = grant(requester.id, 4.0);
    leave.grants.lock().unwrap().push(g.clone());
    leave.txns.lock().unwrap().push(LeaveTransaction {
        id: LeaveTransactionId(Uuid::now_v7()),
        user_id: requester.id,
        grant_id: g.id,
        kind: LeaveTxnKind::Consume,
        delta: -1.0,
        dayoff_id: Some(cancelled.id),
        work_pct: None,
        reason: String::new(),
        created_by: None,
        created_at: OffsetDateTime::now_utc(),
    });
    let svc = repair_service(
        Arc::new(FakeUsers::default()),
        Arc::new(FakeGroups::default()),
        Arc::new(FakeChats::default()),
        dayoffs,
        leave.clone(),
        Arc::new(FakeAuthz::default()),
    );
    svc.handle(RepairJob::SyncDayOffBalance {
        dayoff_id: cancelled.id,
    })
    .await
    .unwrap();
    assert!(
        leave
            .applies
            .lock()
            .unwrap()
            .iter()
            .flatten()
            .any(|t| t.kind == LeaveTxnKind::Refund && (t.delta - 1.0).abs() < 1e-9)
    );
}

// --- T8: emit keeps the notify leg alive when broadcast dies --------------------

#[tokio::test]
async fn emit_broadcast_failure_still_enqueues_notify() {
    use application::{DomainEvent, EventBus};
    use domain::error::EventError;
    use domain::ports::event_publisher::EventPublisher;
    use domain::ports::job_queue::QUEUE_NOTIFICATIONS;

    struct FailingPublisher;
    #[async_trait::async_trait]
    impl EventPublisher for FailingPublisher {
        async fn publish(&self, _topic: &str, _payload: &[u8]) -> Result<(), EventError> {
            Err(EventError::Backend("redis down".into()))
        }
    }

    let jobs = Arc::new(RecordingJobs::default());
    let bus = EventBus::new(Arc::new(FailingPublisher), jobs.clone());
    bus.emit(DomainEvent::TicketAutoClosed {
        ticket_id: TicketId(Uuid::now_v7()),
        at: OffsetDateTime::now_utc(),
    })
    .await;
    assert_eq!(jobs.on(QUEUE_NOTIFICATIONS).len(), 1);
}

// --- T9: direct-channel self-heal ----------------------------------------------

#[tokio::test]
async fn open_direct_channel_heals_missing_membership() {
    let a = user(None);
    let b = user(None);
    let users = Arc::new(FakeUsers::default());
    users.users.lock().unwrap().extend([a.clone(), b.clone()]);

    let chats = Arc::new(FakeChats::default());
    let existing = Channel::Direct(DirectChannel::new(
        ChannelId(Uuid::now_v7()),
        a.id,
        b.id,
        OffsetDateTime::now_utc(),
    ));
    let channel_id = existing.id();
    *chats.direct_channel.lock().unwrap() = Some(existing);

    let perms = Arc::new(Permissions::new(
        users.clone(),
        Arc::new(FakeGroups::default()),
        Arc::new(FakeAuthz::default()),
    ));
    let svc = ChatService::new(
        chats.clone(),
        users,
        Arc::new(FakeChatAttachments),
        Arc::new(FakeStorage),
        perms,
        events(),
    );

    let opened = svc.open_direct_channel(a.id, b.id).await.unwrap();
    assert_eq!(opened.id(), channel_id);
    let subs = chats.subscribes();
    assert!(subs.iter().any(|(u, c, _)| *u == a.id && *c == channel_id));
    assert!(subs.iter().any(|(u, c, _)| *u == b.id && *c == channel_id));
}

// --- T10: group archive ---------------------------------------------------------

#[tokio::test]
async fn delete_group_archives_and_queues_purge_on_fga_failure() {
    let hr = user(Some(SystemRole::Hr));
    let g = group(GroupKind::Standard);

    let users = Arc::new(FakeUsers::default());
    users.users.lock().unwrap().push(hr.clone());
    let groups = Arc::new(FakeGroups::default());
    groups.groups.lock().unwrap().push(g.clone());

    let authz = Arc::new(FakeAuthz::default());
    authz.fail_writes.store(true, Ordering::SeqCst);
    let perms = Arc::new(Permissions::new(users, groups.clone(), authz));
    let repair_jobs = Arc::new(RecordingJobs::default());
    let svc = GroupService::new(
        groups.clone(),
        Arc::new(FakeProjects::default()),
        Arc::new(FakeChats::default()),
        perms,
        events(),
        Arc::new(Repair::new(repair_jobs.clone())),
    );

    svc.delete_group(hr.id, g.id)
        .await
        .expect("archive commits despite failing purge");

    let stored = groups
        .groups
        .lock()
        .unwrap()
        .iter()
        .find(|x| x.id == g.id)
        .cloned()
        .unwrap();
    assert!(stored.archived_at.is_some());
    // Archived groups vanish from the active directory.
    assert!(groups.list_all().await.unwrap().is_empty());
    let queued = repair_jobs.on(QUEUE_REPAIR);
    assert_eq!(queued.len(), 1);
    assert!(matches!(
        decode(&queued[0]),
        RepairJob::SyncGroupProvision { group_id } if group_id == g.id
    ));
}
