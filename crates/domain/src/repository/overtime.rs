use async_trait::async_trait;
use time::Date;

use crate::{
    error::RepositoryError,
    ids::{GroupId, OvertimeId, UserId},
    model::Overtime,
};

/// Loads and persists overtime requests.
#[async_trait]
pub trait OvertimeRepository: Send + Sync {
    async fn find_by_id(&self, id: OvertimeId) -> Result<Option<Overtime>, RepositoryError>;

    /// A user's requests whose `work_date` falls in the inclusive `[from, to]` range.
    async fn list_for_user(
        &self,
        user: UserId,
        from: Date,
        to: Date,
    ) -> Result<Vec<Overtime>, RepositoryError>;

    /// Sum of approved overtime hours the user has in the given calendar month.
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
    ) -> Result<Vec<Overtime>, RepositoryError>;

    /// Leader-approved requests awaiting an HR decision.
    async fn list_pending_for_hr(&self) -> Result<Vec<Overtime>, RepositoryError>;

    /// Upserts the request.
    async fn save(&self, overtime: &Overtime) -> Result<(), RepositoryError>;
}
