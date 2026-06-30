use std::sync::Arc;

use arc_swap::ArcSwap;
use domain::{ids::UserId, model::AttendancePolicy, repository::PolicyRepository};
use time::OffsetDateTime;

use crate::{
    commands::policy::UpdatePolicyCommand,
    error::{Error, Result},
    events::{DomainEvent, EventBus},
    permissions::Permissions,
};

/// Lock-free cache of the current [`AttendancePolicy`], loaded once at boot and
/// swapped atomically on update. Services that read limits depend on this rather
/// than hitting the database per check.
pub struct PolicyProvider {
    cache: ArcSwap<AttendancePolicy>,
}

impl PolicyProvider {
    #[must_use]
    pub fn new(initial: AttendancePolicy) -> Self {
        Self {
            cache: ArcSwap::from_pointee(initial),
        }
    }

    /// Current policy snapshot; cheap (one atomic load + `Arc` clone).
    #[must_use]
    pub fn current(&self) -> Arc<AttendancePolicy> {
        self.cache.load_full()
    }

    /// Replaces the cached policy.
    pub fn store(&self, policy: AttendancePolicy) {
        self.cache.store(Arc::new(policy));
    }
}

/// Reads and updates the attendance policy. HR / Director only; on update it
/// validates, persists, refreshes the cache, and emits `AttendancePolicyUpdated`.
pub struct PolicyService {
    repo: Arc<dyn PolicyRepository>,
    provider: Arc<PolicyProvider>,
    perms: Arc<Permissions>,
    events: Arc<EventBus>,
}

impl PolicyService {
    #[must_use]
    pub fn new(
        repo: Arc<dyn PolicyRepository>,
        provider: Arc<PolicyProvider>,
        perms: Arc<Permissions>,
        events: Arc<EventBus>,
    ) -> Self {
        Self {
            repo,
            provider,
            perms,
            events,
        }
    }

    /// Current cached policy. Readable by any caller; the limits drive the UI.
    #[must_use]
    pub fn current(&self) -> Arc<AttendancePolicy> {
        self.provider.current()
    }

    /// Validates and applies a policy change, persists it, refreshes the cache, and
    /// emits `AttendancePolicyUpdated`.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not an admin, `Validation` if the policy
    /// is invalid, or a repository / event error if a backend is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor))]
    pub async fn update(
        &self,
        actor: UserId,
        cmd: UpdatePolicyCommand,
    ) -> Result<AttendancePolicy> {
        self.perms.require_admin(actor).await?;
        let now = OffsetDateTime::now_utc();
        let policy = AttendancePolicy {
            workday_start: cmd.workday_start,
            work_hours_per_day: cmd.work_hours_per_day,
            flex_core_start: cmd.flex_core_start,
            flex_core_end: cmd.flex_core_end,
            flex_daily_min: cmd.flex_daily_min,
            flex_daily_max: cmd.flex_daily_max,
            flex_earliest_start: cmd.flex_earliest_start,
            flex_latest_end: cmd.flex_latest_end,
            flex_max_segments: cmd.flex_max_segments,
            flex_max_per_month: cmd.flex_max_per_month,
            overtime_max_hours_per_month: cmd.overtime_max_hours_per_month,
            balance_carry_years: cmd.balance_carry_years,
            balance_expiry_policy: cmd.balance_expiry_policy,
            balance_expiry_warn_days: cmd.balance_expiry_warn_days,
            updated_by_user_id: Some(actor),
            updated_at: now,
        };
        policy
            .validate()
            .map_err(|e| Error::Validation(e.to_string()))?;
        self.repo.save(&policy, actor).await?;
        self.provider.store(policy.clone());
        self.events
            .emit(DomainEvent::AttendancePolicyUpdated { actor, at: now })
            .await?;
        Ok(policy)
    }
}
