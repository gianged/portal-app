use async_trait::async_trait;
use time::Date;

use crate::{
    error::RepositoryError,
    ids::{FlexHoursId, GroupId, UserId},
    model::FlexHours,
};

/// Loads and persists flexible-hours requests with their segments.
#[async_trait]
pub trait FlexHoursRepository: Send + Sync {
    async fn find_by_id(&self, id: FlexHoursId) -> Result<Option<FlexHours>, RepositoryError>;

    /// The user's request for a specific `work_date`, if any (unique per date).
    async fn find_by_user_date(
        &self,
        user: UserId,
        date: Date,
    ) -> Result<Option<FlexHours>, RepositoryError>;

    /// A user's requests whose `work_date` falls in the inclusive `[from, to]` range.
    async fn list_for_user(
        &self,
        user: UserId,
        from: Date,
        to: Date,
    ) -> Result<Vec<FlexHours>, RepositoryError>;

    /// Count of approved flex days the user has in the given calendar month.
    async fn approved_count_in_month(
        &self,
        user: UserId,
        year: i32,
        month: u8,
    ) -> Result<u32, RepositoryError>;

    /// Sum of approved flex hours (summed segment durations) for the user in the month.
    async fn approved_hours_in_month(
        &self,
        user: UserId,
        year: i32,
        month: u8,
    ) -> Result<f64, RepositoryError>;

    /// Pending requests from active members of `group` (leader review queue).
    async fn list_pending_for_leader(
        &self,
        group: GroupId,
    ) -> Result<Vec<FlexHours>, RepositoryError>;

    /// Distinct users with at least one approved flex day in the month (worker sweep).
    async fn users_with_approved_flex_in_month(
        &self,
        year: i32,
        month: u8,
    ) -> Result<Vec<UserId>, RepositoryError>;

    /// Upserts the request and replaces its segments transactionally.
    async fn save(&self, flex: &FlexHours) -> Result<(), RepositoryError>;
}
