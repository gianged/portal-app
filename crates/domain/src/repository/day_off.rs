use async_trait::async_trait;
use time::Date;

use crate::{
    error::RepositoryError,
    ids::{DayOffId, GroupId, UserId},
    model::DayOff,
};

/// Loads and persists leave requests.
#[async_trait]
pub trait DayOffRepository: Send + Sync {
    async fn find_by_id(&self, id: DayOffId) -> Result<Option<DayOff>, RepositoryError>;

    /// A user's requests overlapping the inclusive `[from, to]` range.
    async fn list_for_user(
        &self,
        user: UserId,
        from: Date,
        to: Date,
    ) -> Result<Vec<DayOff>, RepositoryError>;

    /// Approved leave days the user took in the given calendar month.
    async fn approved_days_in_month(
        &self,
        user: UserId,
        year: i32,
        month: u32,
    ) -> Result<f64, RepositoryError>;

    /// Pending requests from active members of `group` (leader review queue).
    async fn list_pending_for_leader(&self, group: GroupId)
    -> Result<Vec<DayOff>, RepositoryError>;

    /// Leader-approved annual-leave requests awaiting an HR decision.
    async fn list_pending_for_hr(&self) -> Result<Vec<DayOff>, RepositoryError>;

    /// Upserts the request.
    async fn save(&self, day_off: &DayOff) -> Result<(), RepositoryError>;
}
