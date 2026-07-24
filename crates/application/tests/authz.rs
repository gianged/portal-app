//! Behavior guard for `OpenFGA` tuple writes.
//!
//! The vocabulary unit test in `permissions.rs` proves the relation *names* line
//! up with the model; this proves the *services actually write the tuples* the
//! model needs to traverse (the second half of the original bug, where no
//! `owner_group` / `company` / `requester` tuples were ever written). Repositories and
//! the authz client are in-memory fakes; the fake authz client records every
//! tuple so the tests can assert on them.

mod support;

use std::sync::Arc;

use application::{
    ChatService, Error, GroupService, LeaveBalanceService, Permissions, PolicyProvider,
    ProjectService, TicketService,
    commands::{
        group::{AddMembershipCommand, CreateGroupCommand},
        project::CreateProjectCommand,
        ticket::RaiseTicketCommand,
    },
    error::ConflictCode,
};
use domain::{
    error::TransitionError,
    ids::{TicketId, UserId},
    model::{
        AttendancePolicy, GroupKind, GroupRole, SystemRole, Ticket, TicketCategory, TicketStatus,
        UserStatus,
    },
};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use support::{
    FakeAuthz, FakeChatAttachments, FakeChats, FakeDayOffs, FakeGroups, FakeHolidays, FakeLeave,
    FakeProjects, FakeRequests, FakeStorage, FakeTickets, FakeUsers, events, group, has,
    membership, repair, user,
};

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
        Arc::new(FakeProjects::default()),
        Arc::new(FakeRequests),
        perms,
        events(),
        repair(),
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
    let obj = format!("project:{}", project.entity.id.0);
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
    let svc = TicketService::new(Arc::new(FakeTickets::default()), perms, events(), repair());

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
    let obj = format!("ticket:{}", ticket.entity.id.0);
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
        Arc::new(FakeProjects::default()),
        Arc::new(FakeChats::default()),
        perms,
        events(),
        repair(),
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
    let group_obj = format!("group:{}", created.entity.id.0);
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
        Arc::new(FakeProjects::default()),
        Arc::new(FakeChats::default()),
        perms,
        events(),
        repair(),
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
        version: 0,
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
        Arc::new(FakeProjects::default()),
        Arc::new(FakeChats::default()),
        perms,
        events(),
        repair(),
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
        matches!(err, Error::Conflict(ConflictCode::GroupAlreadyHasLeader)),
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
        Arc::new(FakeProjects::default()),
        Arc::new(FakeChats::default()),
        perms,
        events(),
        repair(),
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
        matches!(err, Error::Conflict(ConflictCode::UserAlreadyMember)),
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
        Arc::new(FakeChats::default()),
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
        Arc::new(FakeChats::default()),
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
        matches!(
            inactive_err,
            Error::Conflict(ConflictCode::RecipientNotActive)
        ),
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

    let svc = TicketService::new(tickets.clone(), perms, events(), repair());
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
        matches!(err, Error::Transition(TransitionError::ReopenWindowExpired)),
        "an expired reopen window must be rejected, got {err:?}"
    );
}

#[tokio::test]
async fn leave_balance_views_gated_to_self_leader_or_hr() {
    let target = user(None);
    let leader = user(None);
    let hr = user(Some(SystemRole::Hr));
    let outsider = user(None);

    let users = Arc::new(FakeUsers::default());
    for u in [&target, &leader, &hr, &outsider] {
        users.users.lock().unwrap().push((*u).clone());
    }
    let g = group(GroupKind::Standard);
    let groups = Arc::new(FakeGroups::default());
    groups.groups.lock().unwrap().push(g.clone());
    {
        let mut m = groups.memberships.lock().unwrap();
        m.push(membership(g.id, target.id, GroupRole::Member));
        m.push(membership(g.id, leader.id, GroupRole::Leader));
    }

    let perms = Arc::new(Permissions::new(
        users,
        groups,
        Arc::new(FakeAuthz::default()),
    ));
    let svc = LeaveBalanceService::new(
        Arc::new(FakeLeave::default()),
        Arc::new(FakeHolidays),
        Arc::new(FakeDayOffs::default()),
        Arc::new(PolicyProvider::new(AttendancePolicy::default())),
        perms,
        events(),
    );

    let asof = OffsetDateTime::now_utc().date();
    assert!(svc.balance_of(target.id, target.id, asof).await.is_ok());
    assert!(svc.balance_of(leader.id, target.id, asof).await.is_ok());
    assert!(svc.balance_of(hr.id, target.id, asof).await.is_ok());
    assert!(svc.grants_of(hr.id, target.id).await.is_ok());

    let err = svc
        .balance_of(outsider.id, target.id, asof)
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::Forbidden),
        "an unrelated member must not view another user's balance, got {err:?}"
    );
    let err = svc.grants_of(outsider.id, target.id).await.unwrap_err();
    assert!(
        matches!(err, Error::Forbidden),
        "an unrelated member must not view another user's grants, got {err:?}"
    );
}
