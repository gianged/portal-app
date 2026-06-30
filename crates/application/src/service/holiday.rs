use std::sync::Arc;

use domain::{ids::UserId, model::Holiday, repository::HolidayRepository};
use time::Date;

use crate::{error::Result, permissions::Permissions};

/// The public-holiday calendar. Any active user can read it; HR maintains it.
pub struct HolidayService {
    holidays: Arc<dyn HolidayRepository>,
    perms: Arc<Permissions>,
}

impl HolidayService {
    #[must_use]
    pub fn new(holidays: Arc<dyn HolidayRepository>, perms: Arc<Permissions>) -> Self {
        Self { holidays, perms }
    }

    /// Holidays within the inclusive `[from, to]` range. Readable by any active user.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active, `NotFound` if the actor does not exist, or a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor))]
    pub async fn list(&self, actor: UserId, from: Date, to: Date) -> Result<Vec<Holiday>> {
        self.perms.require_active(actor).await?;
        Ok(self.holidays.list(from, to).await?)
    }

    /// Adds or renames the holiday on `date`. HR only.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not HR, or a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, date = ?date))]
    pub async fn set(&self, actor: UserId, date: Date, name: String) -> Result<()> {
        self.perms.require_hr(actor).await?;
        self.holidays.upsert(date, &name).await?;
        Ok(())
    }

    /// Removes the holiday on `date`. HR only.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not HR, or a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, date = ?date))]
    pub async fn remove(&self, actor: UserId, date: Date) -> Result<()> {
        self.perms.require_hr(actor).await?;
        self.holidays.delete(date).await?;
        Ok(())
    }
}
