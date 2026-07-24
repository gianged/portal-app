use std::sync::Arc;

use domain::{
    ids::{FlexHoursId, FlexSegmentId, GroupId, UserId},
    model::{FlexHours, FlexSegment, FlexStatus},
    repository::FlexHoursRepository,
};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use crate::{
    commands::flex_hours::{DecideFlexCommand, RequestFlexCommand},
    error::{ConflictCode, Error, Result},
    events::{DomainEvent, EventBus},
    permissions::Permissions,
    service::policy::PolicyProvider,
};

/// Tolerance in hours for the month-end reconciliation; absorbs float
/// accumulation error in summed segment hours.
const RECONCILE_TOLERANCE_HOURS: f64 = 1e-6;

/// Flexible hours: staff file a per-day custom schedule, a leader approves, and
/// the month is expected to net to the standard total. Per-day shape and the
/// monthly cap are read from the cached attendance policy.
pub struct FlexHoursService {
    flex: Arc<dyn FlexHoursRepository>,
    policy: Arc<PolicyProvider>,
    perms: Arc<Permissions>,
    events: Arc<EventBus>,
}

impl FlexHoursService {
    #[must_use]
    pub fn new(
        flex: Arc<dyn FlexHoursRepository>,
        policy: Arc<PolicyProvider>,
        perms: Arc<Permissions>,
        events: Arc<EventBus>,
    ) -> Self {
        Self {
            flex,
            policy,
            perms,
            events,
        }
    }

    /// Files a new flex request after validating the day's shape against policy
    /// and enforcing the monthly cap and the one-per-date rule. Emits `FlexRequested`.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active, `Validation` if the day's
    /// shape is invalid, `Conflict` if the monthly cap is reached or a request
    /// already exists for the date, or a repository / event error if a backend is
    /// unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor))]
    pub async fn request(&self, actor: UserId, cmd: RequestFlexCommand) -> Result<FlexHours> {
        self.perms.require_active(actor).await?;
        let now = OffsetDateTime::now_utc();

        let flex_id = FlexHoursId(Uuid::now_v7());
        let segments = cmd
            .segments
            .into_iter()
            .enumerate()
            .map(|(i, (start, end))| FlexSegment {
                id: FlexSegmentId(Uuid::now_v7()),
                flex_id,
                seq: u16::try_from(i).unwrap_or(u16::MAX),
                start,
                end,
            })
            .collect();
        let flex = FlexHours {
            id: flex_id,
            user_id: actor,
            work_date: cmd.work_date,
            segments,
            status: FlexStatus::Pending,
            leader_user_id: None,
            decided_at: None,
            decision_note: String::new(),
            created_at: now,
            updated_at: now,
        };

        let policy = self.policy.current();
        flex.validate_day(&policy)
            .map_err(|e| Error::Validation(e.to_string()))?;

        let (year, month) = year_month(cmd.work_date);
        let used = self
            .flex
            .approved_count_in_month(actor, year, month)
            .await?;
        if used >= u32::from(policy.flex_max_per_month) {
            return Err(Error::Conflict(ConflictCode::FlexMonthlyCapReached));
        }
        if self
            .flex
            .find_by_user_date(actor, cmd.work_date)
            .await?
            .is_some()
        {
            return Err(Error::Conflict(ConflictCode::FlexAlreadyExistsForDate));
        }

        self.flex.save(&flex).await?;
        self.events
            .emit(DomainEvent::FlexRequested {
                flex_id,
                user_id: actor,
                at: now,
            })
            .await;
        Ok(flex)
    }

    /// Leader's decision on a pending request. Approving re-checks the monthly cap.
    /// Emits `FlexDecided`.
    ///
    /// # Errors
    /// Returns `NotFound` if the request is missing, `Forbidden` if the actor does
    /// not lead the owner's group, `Conflict` if approving would exceed the cap,
    /// `Transition` if the request is not pending, or a repository / event error.
    #[tracing::instrument(skip_all, fields(actor = ?actor, id = ?id))]
    pub async fn decide(
        &self,
        actor: UserId,
        id: FlexHoursId,
        cmd: DecideFlexCommand,
    ) -> Result<FlexHours> {
        let mut flex = self.load(id).await?;
        self.perms
            .require_leader_of_member(actor, flex.user_id)
            .await?;
        let now = OffsetDateTime::now_utc();
        if cmd.approve {
            let (year, month) = year_month(flex.work_date);
            let used = self
                .flex
                .approved_count_in_month(flex.user_id, year, month)
                .await?;
            if used >= u32::from(self.policy.current().flex_max_per_month) {
                return Err(Error::Conflict(ConflictCode::FlexMonthlyCapReached));
            }
            flex.approve(actor, cmd.note, now)?;
        } else {
            flex.reject(actor, cmd.note, now)?;
        }
        self.flex.save(&flex).await?;
        self.events
            .emit(DomainEvent::FlexDecided {
                flex_id: flex.id,
                user_id: flex.user_id,
                status: flex.status,
                actor,
                at: now,
            })
            .await;
        Ok(flex)
    }

    /// Owner cancels their own pending request. Emits `FlexCancelled`.
    ///
    /// # Errors
    /// Returns `NotFound` if missing, `Forbidden` if the actor is not the owner,
    /// `Transition` if it is not pending, or a repository / event error.
    #[tracing::instrument(skip_all, fields(actor = ?actor, id = ?id))]
    pub async fn cancel(&self, actor: UserId, id: FlexHoursId) -> Result<FlexHours> {
        let mut flex = self.load(id).await?;
        if flex.user_id != actor {
            return Err(Error::Forbidden);
        }
        let now = OffsetDateTime::now_utc();
        flex.cancel(now)?;
        self.flex.save(&flex).await?;
        self.events
            .emit(DomainEvent::FlexCancelled {
                flex_id: flex.id,
                user_id: flex.user_id,
                at: now,
            })
            .await;
        Ok(flex)
    }

    /// The actor's own requests with a `work_date` in `[from, to]`.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active, or a repository error.
    #[tracing::instrument(skip_all, fields(actor = ?actor))]
    pub async fn list_mine(&self, actor: UserId, from: Date, to: Date) -> Result<Vec<FlexHours>> {
        self.perms.require_active(actor).await?;
        Ok(self.flex.list_for_user(actor, from, to).await?)
    }

    /// Pending requests in a group the actor leads.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not the group's leader, or a repository error.
    #[tracing::instrument(skip_all, fields(actor = ?actor, group = ?group))]
    pub async fn list_leader_queue(&self, actor: UserId, group: GroupId) -> Result<Vec<FlexHours>> {
        self.perms.require_group_leader(actor, group).await?;
        Ok(self.flex.list_pending_for_leader(group).await?)
    }

    /// Net deviation of the user's approved flex hours from the expected monthly
    /// total: `approved_hours - approved_days * work_hours_per_day`. Zero means the
    /// month is reconciled. Caller owns authorization.
    ///
    /// # Errors
    /// Returns a repository error if a backend is unavailable.
    #[tracing::instrument(skip_all, fields(user = ?user, year, month))]
    pub async fn month_delta(&self, user: UserId, year: i32, month: u8) -> Result<f64> {
        let hours = self.flex.approved_hours_in_month(user, year, month).await?;
        let days = self.flex.approved_count_in_month(user, year, month).await?;
        let expected = f64::from(days) * self.policy.current().work_hours_per_day;
        Ok(hours - expected)
    }

    /// Emits `FlexMonthUnreconciled` for every user whose approved flex hours in the
    /// month do not net to the expected total. Driven by the month-end worker sweep.
    ///
    /// # Errors
    /// Returns a repository / event error if a backend is unavailable.
    #[tracing::instrument(skip_all, fields(year, month))]
    pub async fn emit_unreconciled(&self, year: i32, month: u8) -> Result<()> {
        let users = self
            .flex
            .users_with_approved_flex_in_month(year, month)
            .await?;
        let now = OffsetDateTime::now_utc();
        for user in users {
            // A read failure skips the user instead of aborting the sweep.
            let delta = match self.month_delta(user, year, month).await {
                Ok(d) => d,
                Err(e) => {
                    tracing::warn!(user = ?user, error = %e,
                        "unreconciled check failed; skipping user");
                    continue;
                }
            };
            if delta.abs() > RECONCILE_TOLERANCE_HOURS {
                self.events
                    .emit(DomainEvent::FlexMonthUnreconciled {
                        user_id: user,
                        year,
                        month,
                        at: now,
                    })
                    .await;
            }
        }
        Ok(())
    }

    async fn load(&self, id: FlexHoursId) -> Result<FlexHours> {
        self.flex
            .find_by_id(id)
            .await?
            .ok_or(Error::NotFound("flex_hours"))
    }
}

/// Calendar `(year, month)` of a date, with `month` as 1..=12.
fn year_month(date: Date) -> (i32, u8) {
    (date.year(), u8::from(date.month()))
}
