use async_trait::async_trait;
use time::Date;

use crate::{
    error::RepositoryError,
    ids::{DailyReportId, GroupId, UserId},
    model::DailyReport,
};

/// Loads and persists daily reports and their entries.
#[async_trait]
pub trait DailyReportRepository: Send + Sync {
    async fn find_by_id(&self, id: DailyReportId) -> Result<Option<DailyReport>, RepositoryError>;

    /// The report a user filed for a given date, if any.
    async fn find_by_user_date(
        &self,
        user: UserId,
        date: Date,
    ) -> Result<Option<DailyReport>, RepositoryError>;

    /// A user's reports within the inclusive `[from, to]` date range.
    async fn list_for_user(
        &self,
        user: UserId,
        from: Date,
        to: Date,
    ) -> Result<Vec<DailyReport>, RepositoryError>;

    /// Reports of every active member of `group` within `[from, to]`.
    async fn list_for_group(
        &self,
        group: GroupId,
        from: Date,
        to: Date,
    ) -> Result<Vec<DailyReport>, RepositoryError>;

    /// Upserts the report and replaces its entries transactionally.
    async fn save(&self, report: &DailyReport) -> Result<(), RepositoryError>;
}
