use std::sync::Arc;

use domain::{
    ids::{ChannelId, DayOffId, GroupId, ProjectId, TicketId, UserId},
    model::{Channel, ChannelKind, DayOffStatus, Group, GroupChannel, Membership, UserStatus},
    ports::{
        job_queue::{JobQueue, QUEUE_REPAIR},
        token_revocation::TokenRevocation,
    },
    repository::{
        ChatRepository, DayOffRepository, GroupRepository, ProjectRepository, TicketRepository,
        UserRepository,
    },
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    error::{Error, Result},
    permissions::Permissions,
    service::LeaveBalanceService,
};

/// A post-commit obligation that failed inline. Every variant is an idempotent
/// reconcile: the handler re-derives the desired state from the DB.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RepairJob {
    BumpSessions {
        user_id: UserId,
    },
    SyncUserAccess {
        user_id: UserId,
    },
    SyncMembership {
        group_id: GroupId,
        user_id: UserId,
    },
    SyncGroupProvision {
        group_id: GroupId,
    },
    SyncProjectTuples {
        project_id: ProjectId,
    },
    SyncCollaboratorTuple {
        project_id: ProjectId,
        group_id: GroupId,
    },
    SyncTicketTuples {
        ticket_id: TicketId,
    },
    SyncDayOffBalance {
        dayoff_id: DayOffId,
    },
}

/// A created aggregate plus whether its authz tuples were provisioned inline.
/// `authz_pending = true` means a queued repair owns the grant and permissions
/// may lag briefly; responses surface it so clients can show a syncing state.
#[derive(Debug, Clone)]
pub struct Created<T> {
    pub entity: T,
    pub authz_pending: bool,
}

/// Queues repairs for obligations that failed after the commit point.
pub struct Repair {
    jobs: Arc<dyn JobQueue>,
}

impl Repair {
    #[must_use]
    pub fn new(jobs: Arc<dyn JobQueue>) -> Self {
        Self { jobs }
    }

    /// Logs the failed obligation distinctly and queues its repair.
    pub async fn queue(&self, job: RepairJob, cause: &Error) {
        tracing::error!(job = ?job, error = %cause,
            "post-commit obligation failed; queueing repair");
        let payload = serde_json::to_vec(&job).expect("RepairJob is serde-derivable");
        if let Err(e) = self.jobs.enqueue(QUEUE_REPAIR, &payload).await {
            tracing::error!(job = ?job, error = %e,
                "repair enqueue failed; state needs manual reconcile");
        }
    }

    /// Wraps an inline attempt: on failure, queue the repair and swallow.
    /// Only for obligations; never for business checks. Returns true when the
    /// obligation was fulfilled inline.
    pub async fn ensure(&self, attempt: Result<()>, job: RepairJob) -> bool {
        match attempt {
            Ok(()) => true,
            Err(e) => {
                self.queue(job, &e).await;
                false
            }
        }
    }
}

/// Worker-side reconciles for [`RepairJob`]s. Each handler re-derives the
/// desired state from the DB, so re-running is always safe.
pub struct RepairService {
    users: Arc<dyn UserRepository>,
    groups: Arc<dyn GroupRepository>,
    projects: Arc<dyn ProjectRepository>,
    tickets: Arc<dyn TicketRepository>,
    dayoffs: Arc<dyn DayOffRepository>,
    chats: Arc<dyn ChatRepository>,
    perms: Arc<Permissions>,
    leave: Arc<LeaveBalanceService>,
    revocation: Arc<dyn TokenRevocation>,
}

impl RepairService {
    #[must_use]
    pub fn new(
        users: Arc<dyn UserRepository>,
        groups: Arc<dyn GroupRepository>,
        projects: Arc<dyn ProjectRepository>,
        tickets: Arc<dyn TicketRepository>,
        dayoffs: Arc<dyn DayOffRepository>,
        chats: Arc<dyn ChatRepository>,
        perms: Arc<Permissions>,
        leave: Arc<LeaveBalanceService>,
        revocation: Arc<dyn TokenRevocation>,
    ) -> Self {
        Self {
            users,
            groups,
            projects,
            tickets,
            dayoffs,
            chats,
            perms,
            leave,
            revocation,
        }
    }

    /// Dispatch a repair job. Errors return to apalis for redelivery.
    ///
    /// # Errors
    /// Returns a repository or authz error when a reconcile leg fails; the job
    /// is redelivered and re-runs idempotently.
    #[tracing::instrument(skip_all, fields(job = ?job))]
    pub async fn handle(&self, job: RepairJob) -> Result<()> {
        match job {
            RepairJob::BumpSessions { user_id } => self
                .revocation
                .bump_version(user_id)
                .await
                .map(|_| ())
                .map_err(Into::into),
            RepairJob::SyncUserAccess { user_id } => self.sync_user_access(user_id).await,
            RepairJob::SyncMembership { group_id, user_id } => {
                self.sync_membership(group_id, user_id).await
            }
            RepairJob::SyncGroupProvision { group_id } => self.sync_group_provision(group_id).await,
            RepairJob::SyncProjectTuples { project_id } => {
                self.sync_project_tuples(project_id).await
            }
            RepairJob::SyncCollaboratorTuple {
                project_id,
                group_id,
            } => self.sync_collaborator_tuple(project_id, group_id).await,
            RepairJob::SyncTicketTuples { ticket_id } => self.sync_ticket_tuples(ticket_id).await,
            RepairJob::SyncDayOffBalance { dayoff_id } => {
                self.sync_day_off_balance(dayoff_id).await
            }
        }
    }

    /// Company-role tuple, membership rows/tuples and general-channel
    /// subscription all follow the user's current status.
    async fn sync_user_access(&self, user_id: UserId) -> Result<()> {
        let Some(user) = self.users.find_by_id(user_id).await? else {
            return Ok(());
        };
        let active = user.status == UserStatus::Active;
        if let Some(role) = user.system_role {
            if active {
                self.perms.grant_company_role(user_id, role).await?;
            } else {
                self.perms.revoke_company_role(user_id, role).await?;
            }
        }
        let memberships = self
            .groups
            .list_active_memberships_for_user(user_id)
            .await?;
        if active {
            for m in &memberships {
                self.perms
                    .sync_group_role(user_id, m.group_id, Some(m.role))
                    .await?;
            }
        } else {
            let now = OffsetDateTime::now_utc();
            for mut m in memberships {
                m.deactivate(now);
                self.groups.save_membership(&m, &[]).await?;
                self.perms
                    .sync_group_role(user_id, m.group_id, None)
                    .await?;
            }
        }
        self.sync_general_subscription(user_id, active).await
    }

    /// Role tuple + group-channel subscription follow the membership row.
    async fn sync_membership(&self, group_id: GroupId, user_id: UserId) -> Result<()> {
        let desired = self
            .groups
            .find_membership(group_id, user_id)
            .await?
            .filter(Membership::is_active);
        self.perms
            .sync_group_role(user_id, group_id, desired.as_ref().map(|m| m.role))
            .await?;
        if let Some(channel) = self.chats.find_group_channel(group_id).await? {
            if desired.is_some() {
                self.chats
                    .subscribe_member(user_id, channel.id(), ChannelKind::Group)
                    .await?;
            } else {
                self.chats.unsubscribe_member(user_id, channel.id()).await?;
            }
        }
        Ok(())
    }

    /// Channel row + company/channel tuples follow the group row: an active
    /// group is provisioned, an archived one loses its org-wide tuples.
    async fn sync_group_provision(&self, group_id: GroupId) -> Result<()> {
        let Some(group) = self.groups.find_group(group_id).await? else {
            return Ok(());
        };
        if group.archived_at.is_some() {
            self.perms.revoke_group_created(group.id).await?;
            if let Some(channel) = self.chats.find_group_channel(group.id).await? {
                self.perms
                    .revoke_group_channel_created(group.id, channel.id())
                    .await?;
            }
            return Ok(());
        }
        let channel_id = self.ensure_group_channel(&group).await?;
        self.perms.grant_group_created(group.id).await?;
        self.perms
            .grant_group_channel_created(group.id, channel_id)
            .await
    }

    async fn sync_project_tuples(&self, project_id: ProjectId) -> Result<()> {
        let Some(project) = self.projects.find_by_id(project_id).await? else {
            return Ok(());
        };
        self.perms
            .grant_project_created(project.owner_group_id, project.id)
            .await
    }

    /// Collaborator row exists -> grant; absent -> revoke.
    async fn sync_collaborator_tuple(
        &self,
        project_id: ProjectId,
        group_id: GroupId,
    ) -> Result<()> {
        let exists = self
            .projects
            .list_collaborators(project_id)
            .await?
            .iter()
            .any(|c| c.group_id == group_id);
        if exists {
            self.perms
                .grant_project_collaborator(group_id, project_id)
                .await
        } else {
            self.perms
                .revoke_project_collaborator(group_id, project_id)
                .await
        }
    }

    async fn sync_ticket_tuples(&self, ticket_id: TicketId) -> Result<()> {
        let Some(ticket) = self.tickets.find_by_id(ticket_id).await? else {
            return Ok(());
        };
        self.perms
            .grant_ticket_created(ticket.requester_user_id, ticket.id)
            .await?;
        if let Some(assignee) = ticket.assignee_user_id {
            self.perms
                .grant_ticket_assignee(assignee, ticket.id)
                .await?;
        }
        Ok(())
    }

    /// Balance follows the day-off state; consume/refund are idempotent.
    async fn sync_day_off_balance(&self, id: DayOffId) -> Result<()> {
        let Some(dayoff) = self.dayoffs.find_by_id(id).await? else {
            return Ok(());
        };
        if dayoff.kind.consumes_balance() && dayoff.status == DayOffStatus::Approved {
            self.leave
                .consume(dayoff.requester_user_id, dayoff.days, dayoff.id)
                .await
        } else {
            self.leave.refund(dayoff.id).await
        }
    }

    async fn sync_general_subscription(&self, user_id: UserId, active: bool) -> Result<()> {
        if let Some(channel) = self.chats.find_general_channel().await? {
            if active {
                self.chats
                    .subscribe_member(user_id, channel.id(), ChannelKind::General)
                    .await?;
            } else {
                self.chats.unsubscribe_member(user_id, channel.id()).await?;
            }
        }
        Ok(())
    }

    async fn ensure_group_channel(&self, group: &Group) -> Result<ChannelId> {
        if let Some(channel) = self.chats.find_group_channel(group.id).await? {
            return Ok(channel.id());
        }
        let id = ChannelId(Uuid::now_v7());
        let channel = Channel::Group(GroupChannel {
            id,
            group_id: group.id,
            name: group.name.clone(),
            created_at: OffsetDateTime::now_utc(),
        });
        self.chats.save_channel(&channel).await?;
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use domain::error::JobError;

    use super::*;

    #[derive(Default)]
    struct RecordingJobs {
        jobs: Mutex<Vec<(String, Vec<u8>)>>,
        fail: bool,
    }

    #[async_trait]
    impl JobQueue for RecordingJobs {
        async fn enqueue(&self, queue: &str, payload: &[u8]) -> std::result::Result<(), JobError> {
            if self.fail {
                return Err(JobError::Backend("queue down".into()));
            }
            self.jobs
                .lock()
                .unwrap()
                .push((queue.to_owned(), payload.to_vec()));
            Ok(())
        }
    }

    fn job() -> RepairJob {
        RepairJob::BumpSessions {
            user_id: UserId(Uuid::now_v7()),
        }
    }

    #[tokio::test]
    async fn ensure_queues_job_on_failure() {
        let jobs = Arc::new(RecordingJobs::default());
        let repair = Repair::new(jobs.clone());

        assert!(repair.ensure(Ok(()), job()).await);
        assert!(jobs.jobs.lock().unwrap().is_empty());

        let expected = job();
        assert!(!repair.ensure(Err(Error::Forbidden), expected.clone()).await);
        let queued = jobs.jobs.lock().unwrap();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].0, QUEUE_REPAIR);
        let decoded: RepairJob = serde_json::from_slice(&queued[0].1).unwrap();
        assert!(
            matches!((decoded, expected), (RepairJob::BumpSessions { user_id: a }, RepairJob::BumpSessions { user_id: b }) if a == b)
        );
    }

    #[tokio::test]
    async fn enqueue_failure_does_not_panic() {
        let repair = Repair::new(Arc::new(RecordingJobs {
            jobs: Mutex::new(Vec::new()),
            fail: true,
        }));
        assert!(!repair.ensure(Err(Error::Forbidden), job()).await);
    }
}
