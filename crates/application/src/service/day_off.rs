use std::sync::Arc;

use domain::{
    ids::{DayOffId, GroupId, UserId},
    model::{self, DayOff, DayOffStatus},
    repository::{DayOffRepository, HolidayRepository},
};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use crate::{
    commands::day_off::{CreateDayOffCommand, DecideDayOffCommand},
    error::{ConflictCode, Error, Result},
    events::{DomainEvent, EventBus},
    permissions::Permissions,
    service::leave_balance::LeaveBalanceService,
};

/// Leave requests: staff file a day-off, a leader decides (and HR too, for annual
/// leave). Annual leave checks and consumes the leave balance.
pub struct DayOffService {
    dayoffs: Arc<dyn DayOffRepository>,
    holidays: Arc<dyn HolidayRepository>,
    leave: Arc<LeaveBalanceService>,
    perms: Arc<Permissions>,
    events: Arc<EventBus>,
}

impl DayOffService {
    #[must_use]
    pub fn new(
        dayoffs: Arc<dyn DayOffRepository>,
        holidays: Arc<dyn HolidayRepository>,
        leave: Arc<LeaveBalanceService>,
        perms: Arc<Permissions>,
        events: Arc<EventBus>,
    ) -> Self {
        Self {
            dayoffs,
            holidays,
            leave,
            perms,
            events,
        }
    }

    /// Files a new leave request. Rejects a past start date unless the kind allows
    /// backdating, computes `days` from the holiday calendar, and (for annual
    /// leave) requires enough balance. Emits `DayOffRequested`.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active, `Validation` for a disallowed past date, `Conflict` if annual-leave balance is insufficient, or a repository / event error if a backend is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor))]
    pub async fn create(&self, actor: UserId, cmd: CreateDayOffCommand) -> Result<DayOff> {
        self.perms.require_active(actor).await?;
        let now = OffsetDateTime::now_utc();
        let today = now.date();

        if cmd.start_date < today && !cmd.kind.allows_backdate() {
            return Err(Error::Validation(
                "start date cannot be in the past for this leave kind".into(),
            ));
        }

        let holiday_dates: Vec<Date> = self
            .holidays
            .list(cmd.start_date, cmd.end_date)
            .await?
            .into_iter()
            .map(|h| h.date)
            .collect();
        let days = model::working_days(
            cmd.start_date,
            cmd.end_date,
            cmd.start_half,
            cmd.end_half,
            &holiday_dates,
        );

        if cmd.kind.consumes_balance() {
            let available = self.leave.available(actor, today).await?;
            if available + f64::EPSILON < days {
                return Err(Error::Conflict(ConflictCode::InsufficientLeaveBalance));
            }
        }

        let day_off = DayOff {
            id: DayOffId(Uuid::now_v7()),
            requester_user_id: actor,
            kind: cmd.kind,
            start_date: cmd.start_date,
            end_date: cmd.end_date,
            start_half: cmd.start_half,
            end_half: cmd.end_half,
            days,
            reason: cmd.reason,
            status: DayOffStatus::Pending,
            leader_user_id: None,
            leader_decided_at: None,
            hr_user_id: None,
            hr_decided_at: None,
            decision_note: String::new(),
            created_at: now,
            updated_at: now,
        };
        self.dayoffs.save(&day_off).await?;
        self.events
            .emit(DomainEvent::DayOffRequested {
                dayoff_id: day_off.id,
                user_id: actor,
                actor,
                at: now,
            })
            .await?;
        Ok(day_off)
    }

    /// Leader's decision. Approving a leader-only kind finalizes it; approving an
    /// HR-gated kind advances it to await HR. Emits `DayOffDecided`.
    ///
    /// # Errors
    /// Returns `NotFound` if the request is missing, `Forbidden` if the actor does not lead the requester's group, `Transition` if the request is not pending, or a repository / event error if a backend is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, id = ?id))]
    pub async fn leader_decide(
        &self,
        actor: UserId,
        id: DayOffId,
        cmd: DecideDayOffCommand,
    ) -> Result<DayOff> {
        let mut day_off = self.load(id).await?;
        self.perms
            .require_leader_of_member(actor, day_off.requester_user_id)
            .await?;
        let now = OffsetDateTime::now_utc();
        if cmd.approve {
            if day_off.kind.requires_hr_approval() {
                day_off.leader_approve(actor, cmd.note, now)?;
            } else {
                day_off.approve(actor, cmd.note, now)?;
            }
        } else {
            day_off.reject(actor, cmd.note, now)?;
        }
        self.dayoffs.save(&day_off).await?;
        self.emit_decided(&day_off, cmd.approve, actor, now).await?;
        Ok(day_off)
    }

    /// HR's decision on a leader-approved annual-leave request. Approving consumes
    /// the balance before finalizing. Emits `DayOffDecided`.
    ///
    /// # Errors
    /// Returns `NotFound` if the request is missing, `Forbidden` if the actor is not HR, `Conflict` if it is not awaiting HR or balance is insufficient, `Transition` for an illegal move, or a repository / event error if a backend is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, id = ?id))]
    pub async fn hr_decide(
        &self,
        actor: UserId,
        id: DayOffId,
        cmd: DecideDayOffCommand,
    ) -> Result<DayOff> {
        self.perms.require_hr(actor).await?;
        let mut day_off = self.load(id).await?;
        if day_off.status != DayOffStatus::LeaderApproved {
            return Err(Error::Conflict(ConflictCode::NotAwaitingHr));
        }
        let now = OffsetDateTime::now_utc();
        if cmd.approve {
            if day_off.kind.consumes_balance() {
                self.leave
                    .consume(day_off.requester_user_id, day_off.days, day_off.id)
                    .await?;
            }
            day_off.hr_approve(actor, cmd.note, now)?;
        } else {
            day_off.reject(actor, cmd.note, now)?;
        }
        self.dayoffs.save(&day_off).await?;
        self.emit_decided(&day_off, cmd.approve, actor, now).await?;
        Ok(day_off)
    }

    /// Requester cancels their own non-terminal request, refunding any consumed
    /// balance. Emits `DayOffCancelled`.
    ///
    /// # Errors
    /// Returns `NotFound` if the request is missing, `Forbidden` if the actor is not the requester, `Transition` if the request is terminal, or a repository / event error if a backend is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, id = ?id))]
    pub async fn cancel(&self, actor: UserId, id: DayOffId) -> Result<DayOff> {
        let mut day_off = self.load(id).await?;
        if day_off.requester_user_id != actor {
            return Err(Error::Forbidden);
        }
        let was_consumed =
            day_off.kind.consumes_balance() && day_off.status == DayOffStatus::Approved;
        let now = OffsetDateTime::now_utc();
        day_off.cancel(now)?;
        if was_consumed {
            self.leave.refund(day_off.id).await?;
        }
        self.dayoffs.save(&day_off).await?;
        self.events
            .emit(DomainEvent::DayOffCancelled {
                dayoff_id: day_off.id,
                user_id: day_off.requester_user_id,
                at: now,
            })
            .await?;
        Ok(day_off)
    }

    /// The actor's own requests overlapping `[from, to]`.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active, or a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor))]
    pub async fn list_mine(&self, actor: UserId, from: Date, to: Date) -> Result<Vec<DayOff>> {
        self.perms.require_active(actor).await?;
        Ok(self.dayoffs.list_for_user(actor, from, to).await?)
    }

    /// Pending requests in a group the actor leads (all kinds).
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not the group's leader, or a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, group = ?group))]
    pub async fn list_leader_queue(&self, actor: UserId, group: GroupId) -> Result<Vec<DayOff>> {
        self.perms.require_group_leader(actor, group).await?;
        Ok(self.dayoffs.list_pending_for_leader(group).await?)
    }

    /// Leader-approved annual-leave requests awaiting an HR decision.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not HR, or a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor))]
    pub async fn list_hr_queue(&self, actor: UserId) -> Result<Vec<DayOff>> {
        self.perms.require_hr(actor).await?;
        Ok(self.dayoffs.list_pending_for_hr().await?)
    }

    async fn emit_decided(
        &self,
        day_off: &DayOff,
        approved: bool,
        actor: UserId,
        now: OffsetDateTime,
    ) -> Result<()> {
        self.events
            .emit(DomainEvent::DayOffDecided {
                dayoff_id: day_off.id,
                user_id: day_off.requester_user_id,
                approved,
                actor,
                at: now,
            })
            .await?;
        Ok(())
    }

    async fn load(&self, id: DayOffId) -> Result<DayOff> {
        self.dayoffs
            .find_by_id(id)
            .await?
            .ok_or(Error::NotFound("day_off"))
    }
}
