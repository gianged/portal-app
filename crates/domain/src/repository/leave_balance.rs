use async_trait::async_trait;
use time::Date;

use crate::{
    error::RepositoryError,
    ids::{DayOffId, LeaveGrantId, UserId},
    model::{LeaveGrant, LeaveTransaction},
};

/// Loads and persists leave grants and the immutable balance ledger.
#[async_trait]
pub trait LeaveBalanceRepository: Send + Sync {
    /// Every grant for `user`, newest year first.
    async fn list_grants(&self, user: UserId) -> Result<Vec<LeaveGrant>, RepositoryError>;

    /// Sum of `days_remaining` across the user's non-expired grants as of `asof`.
    async fn available(&self, user: UserId, asof: Date) -> Result<f64, RepositoryError>;

    /// Inserts or updates a year grant (keyed on `user_id, grant_year`).
    async fn upsert_grant(&self, grant: &LeaveGrant) -> Result<(), RepositoryError>;

    /// Atomically applies `grant_deltas` (added to each grant's `days_remaining`)
    /// and inserts every transaction in `txns`, in one database transaction.
    async fn apply(
        &self,
        grant_deltas: &[(LeaveGrantId, f64)],
        txns: &[LeaveTransaction],
    ) -> Result<(), RepositoryError>;

    /// Grants expiring within `within_days` of `asof` that still have a remainder.
    async fn list_expiring(
        &self,
        asof: Date,
        within_days: i64,
    ) -> Result<Vec<LeaveGrant>, RepositoryError>;

    /// The user's ledger entries created within the inclusive `[from, to]` range.
    async fn list_transactions(
        &self,
        user: UserId,
        from: Date,
        to: Date,
    ) -> Result<Vec<LeaveTransaction>, RepositoryError>;

    /// Ledger entries linked to a given leave request (drives refund reversal).
    async fn transactions_for_dayoff(
        &self,
        dayoff_id: DayOffId,
    ) -> Result<Vec<LeaveTransaction>, RepositoryError>;
}
