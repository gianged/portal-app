use async_trait::async_trait;
use time::Date;

use crate::{error::RepositoryError, model::Holiday};

/// Loads and maintains the public-holiday calendar.
#[async_trait]
pub trait HolidayRepository: Send + Sync {
    /// Holidays within the inclusive `[from, to]` date range, ascending by date.
    async fn list(&self, from: Date, to: Date) -> Result<Vec<Holiday>, RepositoryError>;

    /// Inserts or renames the holiday on `date`.
    async fn upsert(&self, date: Date, name: &str) -> Result<(), RepositoryError>;

    /// Removes the holiday on `date` (no-op if none exists).
    async fn delete(&self, date: Date) -> Result<(), RepositoryError>;
}
