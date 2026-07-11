use std::sync::Arc;

use domain::{
    ids::{GroupId, OvertimeId, UserId},
    model::{Overtime, OvertimeStatus},
    repository::OvertimeRepository,
};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use crate::{
    commands::overtime::{CreateOvertimeCommand, DecideOvertimeCommand},
    error::{ConflictCode, Error, Result},
    events::{DomainEvent, EventBus},
    permissions::Permissions,
    service::policy::PolicyProvider,
};

/// Overtime requests: staff file extra hours, a leader approves, then HR approves.
/// The monthly legal cap is read from the cached attendance policy.
pub struct OvertimeService {
    overtimes: Arc<dyn OvertimeRepository>,
    policy: Arc<PolicyProvider>,
    perms: Arc<Permissions>,
    events: Arc<EventBus>,
}

impl OvertimeService {
    #[must_use]
    pub fn new(
        overtimes: Arc<dyn OvertimeRepository>,
        policy: Arc<PolicyProvider>,
        perms: Arc<Permissions>,
        events: Arc<EventBus>,
    ) -> Self {
        Self {
            overtimes,
            policy,
            perms,
            events,
        }
    }

    /// Files a new overtime request, rejecting it if the month's approved hours
    /// plus these would exceed the policy cap. Emits `OvertimeRequested`.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active, `Conflict` if the monthly cap would be exceeded, or a repository / event error if a backend is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor))]
    pub async fn create(&self, actor: UserId, cmd: CreateOvertimeCommand) -> Result<Overtime> {
        self.perms.require_active(actor).await?;
        let now = OffsetDateTime::now_utc();

        let cap = self.policy.current().overtime_max_hours_per_month;
        let year = cmd.work_date.year();
        let month = u8::from(cmd.work_date.month());
        let used = self
            .overtimes
            .approved_hours_in_month(actor, year, month)
            .await?;
        if used + cmd.hours > cap + f64::EPSILON {
            return Err(Error::Conflict(ConflictCode::OvertimeMonthlyCapExceeded));
        }

        let overtime = Overtime {
            id: OvertimeId(Uuid::now_v7()),
            requester_user_id: actor,
            work_date: cmd.work_date,
            hours: cmd.hours,
            reason: cmd.reason,
            status: OvertimeStatus::Pending,
            leader_user_id: None,
            leader_decided_at: None,
            hr_user_id: None,
            hr_decided_at: None,
            decision_note: String::new(),
            created_at: now,
            updated_at: now,
        };
        self.overtimes.save(&overtime).await?;
        self.events
            .emit(DomainEvent::OvertimeRequested {
                overtime_id: overtime.id,
                requester: actor,
                at: now,
            })
            .await?;
        Ok(overtime)
    }

    /// Leader's decision. Approving advances the request to await HR. Emits
    /// `OvertimeDecided`.
    ///
    /// # Errors
    /// Returns `NotFound` if the request is missing, `Forbidden` if the actor does not lead the requester's group, `Transition` if the request is not pending, or a repository / event error if a backend is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, id = ?id))]
    pub async fn leader_decide(
        &self,
        actor: UserId,
        id: OvertimeId,
        cmd: DecideOvertimeCommand,
    ) -> Result<Overtime> {
        let mut overtime = self.load(id).await?;
        self.perms
            .require_leader_of_member(actor, overtime.requester_user_id)
            .await?;
        let now = OffsetDateTime::now_utc();
        if cmd.approve {
            overtime.leader_approve(actor, cmd.note, now)?;
        } else {
            overtime.reject(actor, cmd.note, now)?;
        }
        self.overtimes.save(&overtime).await?;
        self.emit_decided(&overtime, actor, now).await?;
        Ok(overtime)
    }

    /// HR's decision on a leader-approved request. Emits `OvertimeDecided`.
    ///
    /// # Errors
    /// Returns `NotFound` if the request is missing, `Forbidden` if the actor is not HR, `Conflict` if it is not awaiting HR, `Transition` for an illegal move, or a repository / event error if a backend is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, id = ?id))]
    pub async fn hr_decide(
        &self,
        actor: UserId,
        id: OvertimeId,
        cmd: DecideOvertimeCommand,
    ) -> Result<Overtime> {
        self.perms.require_hr(actor).await?;
        let mut overtime = self.load(id).await?;
        if overtime.status != OvertimeStatus::LeaderApproved {
            return Err(Error::Conflict(ConflictCode::NotAwaitingHr));
        }
        let now = OffsetDateTime::now_utc();
        if cmd.approve {
            overtime.hr_approve(actor, cmd.note, now)?;
        } else {
            overtime.reject(actor, cmd.note, now)?;
        }
        self.overtimes.save(&overtime).await?;
        self.emit_decided(&overtime, actor, now).await?;
        Ok(overtime)
    }

    /// Requester cancels their own non-terminal request. Emits `OvertimeCancelled`.
    ///
    /// # Errors
    /// Returns `NotFound` if the request is missing, `Forbidden` if the actor is not the requester, `Transition` if the request is terminal, or a repository / event error if a backend is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, id = ?id))]
    pub async fn cancel(&self, actor: UserId, id: OvertimeId) -> Result<Overtime> {
        let mut overtime = self.load(id).await?;
        if overtime.requester_user_id != actor {
            return Err(Error::Forbidden);
        }
        let now = OffsetDateTime::now_utc();
        overtime.cancel(now)?;
        self.overtimes.save(&overtime).await?;
        self.events
            .emit(DomainEvent::OvertimeCancelled {
                overtime_id: overtime.id,
                requester: overtime.requester_user_id,
                at: now,
            })
            .await?;
        Ok(overtime)
    }

    /// The actor's own requests with a `work_date` in `[from, to]`.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active, or a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor))]
    pub async fn list_mine(&self, actor: UserId, from: Date, to: Date) -> Result<Vec<Overtime>> {
        self.perms.require_active(actor).await?;
        Ok(self.overtimes.list_for_user(actor, from, to).await?)
    }

    /// Pending requests in a group the actor leads.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not the group's leader, or a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, group = ?group))]
    pub async fn list_leader_queue(&self, actor: UserId, group: GroupId) -> Result<Vec<Overtime>> {
        self.perms.require_group_leader(actor, group).await?;
        Ok(self.overtimes.list_pending_for_leader(group).await?)
    }

    /// Leader-approved requests awaiting an HR decision.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not HR, or a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor))]
    pub async fn list_hr_queue(&self, actor: UserId) -> Result<Vec<Overtime>> {
        self.perms.require_hr(actor).await?;
        Ok(self.overtimes.list_pending_for_hr().await?)
    }

    async fn emit_decided(
        &self,
        overtime: &Overtime,
        actor: UserId,
        now: OffsetDateTime,
    ) -> Result<()> {
        self.events
            .emit(DomainEvent::OvertimeDecided {
                overtime_id: overtime.id,
                requester: overtime.requester_user_id,
                status: overtime.status,
                actor,
                at: now,
            })
            .await?;
        Ok(())
    }

    async fn load(&self, id: OvertimeId) -> Result<Overtime> {
        self.overtimes
            .find_by_id(id)
            .await?
            .ok_or(Error::NotFound("overtime"))
    }
}
