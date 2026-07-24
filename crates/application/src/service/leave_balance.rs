use std::{collections::HashMap, sync::Arc};

use domain::{
    ids::{DayOffId, LeaveGrantId, LeaveTransactionId, UserId},
    model::{self, BalanceExpiryPolicy, LeaveGrant, LeaveTransaction, LeaveTxnKind},
    repository::{DayOffRepository, HolidayRepository, LeaveBalanceRepository},
};
use time::{Date, Month, OffsetDateTime};
use uuid::Uuid;

use crate::{
    commands::leave_balance::{AdjustBalanceCommand, SetLeaveGrantCommand},
    error::{ConflictCode, Error, Result},
    events::{DomainEvent, EventBus},
    permissions::Permissions,
    service::policy::PolicyProvider,
};

/// Leave balances: HR grants yearly entitlements that carry forward (FIFO) until
/// they expire. Day-off consumes and refunds balance here; the expiry sweep lapses
/// stale grants and (per policy) records the month's work percentage.
pub struct LeaveBalanceService {
    leave: Arc<dyn LeaveBalanceRepository>,
    holidays: Arc<dyn HolidayRepository>,
    dayoffs: Arc<dyn DayOffRepository>,
    policy: Arc<PolicyProvider>,
    perms: Arc<Permissions>,
    events: Arc<EventBus>,
}

impl LeaveBalanceService {
    #[must_use]
    pub fn new(
        leave: Arc<dyn LeaveBalanceRepository>,
        holidays: Arc<dyn HolidayRepository>,
        dayoffs: Arc<dyn DayOffRepository>,
        policy: Arc<PolicyProvider>,
        perms: Arc<Permissions>,
        events: Arc<EventBus>,
    ) -> Self {
        Self {
            leave,
            holidays,
            dayoffs,
            policy,
            perms,
            events,
        }
    }

    /// HR sets a user's entitlement for `grant_year`. A new grant starts fully
    /// remaining; editing an existing one keeps the already-consumed portion and
    /// clamps the remainder into the new ceiling. Emits `LeaveBalanceAdjusted`.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not HR, or a repository / event error if a backend is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor))]
    pub async fn set_grant(&self, actor: UserId, cmd: SetLeaveGrantCommand) -> Result<()> {
        self.perms.require_hr(actor).await?;
        let now = OffsetDateTime::now_utc();
        let carry = self.policy.current().balance_carry_years;
        let exp_year = i32::from(cmd.grant_year) + i32::from(carry);
        let expires_on = Date::from_calendar_date(exp_year, Month::December, 31)
            .map_err(|_| Error::Validation("grant year out of range".into()))?;

        let existing = self
            .leave
            .list_grants(cmd.user_id)
            .await?
            .into_iter()
            .find(|g| g.grant_year == cmd.grant_year);
        let (grant, prev_granted) = match existing {
            Some(g) => {
                let consumed = (g.days_granted - g.days_remaining).max(0.0);
                let remaining = (cmd.days_granted - consumed).max(0.0);
                (
                    LeaveGrant {
                        days_granted: cmd.days_granted,
                        days_remaining: remaining,
                        expires_on,
                        updated_at: now,
                        ..g
                    },
                    g.days_granted,
                )
            }
            None => (
                LeaveGrant {
                    id: LeaveGrantId(Uuid::now_v7()),
                    user_id: cmd.user_id,
                    grant_year: cmd.grant_year,
                    days_granted: cmd.days_granted,
                    days_remaining: cmd.days_granted,
                    expires_on,
                    created_by: Some(actor),
                    created_at: now,
                    updated_at: now,
                },
                0.0,
            ),
        };
        // Grant and its ledger entry commit in one transaction (absolute
        // remainder is already written; the ledger records the change).
        let delta = cmd.days_granted - prev_granted;
        let txn = (delta.abs() > f64::EPSILON).then(|| LeaveTransaction {
            id: LeaveTransactionId(Uuid::now_v7()),
            user_id: cmd.user_id,
            grant_id: grant.id,
            kind: LeaveTxnKind::Grant,
            delta,
            dayoff_id: None,
            work_pct: None,
            reason: String::new(),
            created_by: Some(actor),
            created_at: now,
        });
        self.leave
            .upsert_grant_with_txn(&grant, txn.as_ref())
            .await?;

        self.events
            .emit(DomainEvent::LeaveBalanceAdjusted {
                user_id: cmd.user_id,
                actor,
                at: now,
            })
            .await;
        Ok(())
    }

    /// HR posts a manual correction against the user's most-recent grant. Emits
    /// `LeaveBalanceAdjusted`.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not HR, `Conflict` if the user has no grant to adjust, or a repository / event error if a backend is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor))]
    pub async fn adjust(&self, actor: UserId, cmd: AdjustBalanceCommand) -> Result<()> {
        self.perms.require_hr(actor).await?;
        let now = OffsetDateTime::now_utc();
        let mut grant = self
            .leave
            .list_grants(cmd.user_id)
            .await?
            .into_iter()
            .next()
            .ok_or(Error::Conflict(ConflictCode::NoLeaveGrant))?;

        let prev_remaining = grant.days_remaining;
        grant.days_remaining = (grant.days_remaining + cmd.delta).max(0.0);
        if grant.days_remaining > grant.days_granted {
            grant.days_granted = grant.days_remaining;
        }
        grant.updated_at = now;
        // Ledger the delta actually applied (the zero clamp may shrink it),
        // committed atomically with the grant.
        let txn = LeaveTransaction {
            id: LeaveTransactionId(Uuid::now_v7()),
            user_id: cmd.user_id,
            grant_id: grant.id,
            kind: LeaveTxnKind::Adjust,
            delta: grant.days_remaining - prev_remaining,
            dayoff_id: None,
            work_pct: None,
            reason: cmd.reason,
            created_by: Some(actor),
            created_at: now,
        };
        self.leave.upsert_grant_with_txn(&grant, Some(&txn)).await?;

        self.events
            .emit(DomainEvent::LeaveBalanceAdjusted {
                user_id: cmd.user_id,
                actor,
                at: now,
            })
            .await;
        Ok(())
    }

    /// Days available to the user as of `asof` (sum of non-expired remainders).
    /// Ungated; callers authorize first.
    ///
    /// # Errors
    /// Returns a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(user = ?user))]
    pub async fn available(&self, user: UserId, asof: Date) -> Result<f64> {
        Ok(self.leave.available(user, asof).await?)
    }

    /// Days available to `target` as of `asof`, gated to self, a leader of the
    /// member, or HR.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor may not view the target's balance, or a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, target = ?target))]
    pub async fn balance_of(&self, actor: UserId, target: UserId, asof: Date) -> Result<f64> {
        self.require_can_view(actor, target).await?;
        Ok(self.leave.available(target, asof).await?)
    }

    /// `target`'s grants, newest year first, gated like [`Self::balance_of`].
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor may not view the target's balance, or a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, target = ?target))]
    pub async fn grants_of(&self, actor: UserId, target: UserId) -> Result<Vec<LeaveGrant>> {
        self.require_can_view(actor, target).await?;
        Ok(self.leave.list_grants(target).await?)
    }

    /// Grants plus the ledger entries in `[from, to]`. Ungated; callers authorize.
    ///
    /// # Errors
    /// Returns a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(user = ?user))]
    pub async fn statement(
        &self,
        user: UserId,
        from: Date,
        to: Date,
    ) -> Result<(Vec<LeaveGrant>, Vec<LeaveTransaction>)> {
        let grants = self.leave.list_grants(user).await?;
        let txns = self.leave.list_transactions(user, from, to).await?;
        Ok((grants, txns))
    }

    /// Work percentage for a calendar month: present working days over expected
    /// working days, where expected excludes weekends and holidays and present
    /// subtracts approved leave. Returns 0 when the month has no working days.
    ///
    /// # Errors
    /// Returns `Validation` if `month` is out of range, or a repository error if a backend is unavailable.
    #[tracing::instrument(skip_all, fields(user = ?user, year, month))]
    pub async fn work_percentage(&self, user: UserId, year: i32, month: u8) -> Result<f64> {
        let m = Month::try_from(month).map_err(|_| Error::Validation("invalid month".into()))?;
        let first = Date::from_calendar_date(year, m, 1)
            .map_err(|_| Error::Validation("invalid month".into()))?;
        let last = Date::from_calendar_date(year, m, m.length(year))
            .map_err(|_| Error::Validation("invalid month".into()))?;

        let holiday_dates: Vec<Date> = self
            .holidays
            .list(first, last)
            .await?
            .into_iter()
            .map(|h| h.date)
            .collect();
        let working = model::working_days(first, last, false, false, &holiday_dates);
        if working <= 0.0 {
            return Ok(0.0);
        }
        let approved = self
            .dayoffs
            .approved_days_in_month(user, year, month)
            .await?;
        let present = (working - approved).max(0.0);
        Ok(100.0 * present / working)
    }

    /// Draws `days` from the user's grants FIFO and records `Consume` ledger
    /// entries linked to `dayoff_id`. Called when annual leave is HR-approved.
    ///
    /// # Errors
    /// Returns `Conflict` when the balance is insufficient, or a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(user = ?user, dayoff = ?dayoff_id))]
    pub async fn consume(&self, user: UserId, days: f64, dayoff_id: DayOffId) -> Result<()> {
        let now = OffsetDateTime::now_utc();
        // Idempotent on retry: skip if this dayoff already has a Consume entry.
        let existing = self.leave.transactions_for_dayoff(dayoff_id).await?;
        if existing.iter().any(|t| t.kind == LeaveTxnKind::Consume) {
            return Ok(());
        }
        let grants = self.leave.list_grants(user).await?;
        let deltas = model::allocate_fifo(&grants, days, now.date())
            .map_err(|_| Error::Conflict(ConflictCode::InsufficientLeaveBalance))?;
        if deltas.is_empty() {
            return Ok(());
        }
        let txns: Vec<LeaveTransaction> = deltas
            .iter()
            .map(|(grant_id, delta)| LeaveTransaction {
                id: LeaveTransactionId(Uuid::now_v7()),
                user_id: user,
                grant_id: *grant_id,
                kind: LeaveTxnKind::Consume,
                delta: *delta,
                dayoff_id: Some(dayoff_id),
                work_pct: None,
                reason: String::new(),
                created_by: None,
                created_at: now,
            })
            .collect();
        self.leave.apply(&deltas, &txns).await?;
        Ok(())
    }

    /// Reverses a request's net consumption with `Refund` ledger entries. Safe to
    /// call when nothing was consumed (no-op) or already refunded.
    ///
    /// # Errors
    /// Returns a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(dayoff = ?dayoff_id))]
    pub async fn refund(&self, dayoff_id: DayOffId) -> Result<()> {
        let now = OffsetDateTime::now_utc();
        let txns = self.leave.transactions_for_dayoff(dayoff_id).await?;
        let mut net: HashMap<LeaveGrantId, (UserId, f64)> = HashMap::new();
        for t in &txns {
            if matches!(t.kind, LeaveTxnKind::Consume | LeaveTxnKind::Refund) {
                net.entry(t.grant_id).or_insert((t.user_id, 0.0)).1 += t.delta;
            }
        }
        let mut deltas = Vec::new();
        let mut refunds = Vec::new();
        for (grant_id, (user, sum)) in net {
            if sum < -f64::EPSILON {
                let amount = -sum;
                deltas.push((grant_id, amount));
                refunds.push(LeaveTransaction {
                    id: LeaveTransactionId(Uuid::now_v7()),
                    user_id: user,
                    grant_id,
                    kind: LeaveTxnKind::Refund,
                    delta: amount,
                    dayoff_id: Some(dayoff_id),
                    work_pct: None,
                    reason: String::new(),
                    created_by: None,
                    created_at: now,
                });
            }
        }
        if !deltas.is_empty() {
            self.leave.apply(&deltas, &refunds).await?;
        }
        Ok(())
    }

    /// Daily expiry sweep: warns on grants nearing expiry, then lapses grants whose
    /// expiry has passed (zeroing their remainder via an `Expire` ledger entry).
    /// Records the expiry month's work percentage when the policy says so.
    ///
    /// # Errors
    /// Returns a repository / event error if a backend is unavailable.
    #[tracing::instrument(skip_all, fields(asof = ?asof))]
    pub async fn run_expiry(&self, asof: Date) -> Result<()> {
        let policy = self.policy.current();
        let warn_days = i64::from(policy.balance_expiry_warn_days);
        let grants = self.leave.list_expiring(asof, warn_days).await?;
        let now = OffsetDateTime::now_utc();
        let record_pct = policy.balance_expiry_policy == BalanceExpiryPolicy::RecordWorkPct;

        let mut deltas = Vec::new();
        let mut txns = Vec::new();
        for g in grants {
            if g.days_remaining <= 0.0 {
                continue;
            }
            if g.expires_on <= asof {
                let work_pct = if record_pct {
                    let year = g.expires_on.year();
                    let month = u8::from(g.expires_on.month());
                    // A read failure skips this grant instead of aborting the
                    // sweep; the next run retries it.
                    match self.work_percentage(g.user_id, year, month).await {
                        Ok(pct) => Some(pct),
                        Err(e) => {
                            tracing::warn!(user = ?g.user_id, grant = ?g.id, error = %e,
                                "work percentage read failed; skipping grant this sweep");
                            continue;
                        }
                    }
                } else {
                    None
                };
                deltas.push((g.id, -g.days_remaining));
                txns.push(LeaveTransaction {
                    id: LeaveTransactionId(Uuid::now_v7()),
                    user_id: g.user_id,
                    grant_id: g.id,
                    kind: LeaveTxnKind::Expire,
                    delta: -g.days_remaining,
                    dayoff_id: None,
                    work_pct,
                    reason: String::new(),
                    created_by: None,
                    created_at: now,
                });
            } else {
                self.events
                    .emit(DomainEvent::LeaveBalanceExpiring {
                        user_id: g.user_id,
                        grant_id: g.id,
                        at: now,
                    })
                    .await;
            }
        }
        if !deltas.is_empty() {
            self.leave.apply(&deltas, &txns).await?;
        }
        Ok(())
    }

    /// Self always; otherwise a leader of the member or HR.
    async fn require_can_view(&self, actor: UserId, target: UserId) -> Result<()> {
        if actor == target {
            return Ok(());
        }
        if self.perms.is_leader_of_member(actor, target).await? || self.perms.is_hr(actor).await? {
            return Ok(());
        }
        Err(Error::Forbidden)
    }
}
