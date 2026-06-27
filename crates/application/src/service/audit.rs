mod projector;

use std::sync::Arc;

use domain::{ids::UserId, model::AuditLog, repository::AuditRepository};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{error::Result, permissions::Permissions};

pub use projector::AuditProjector;

/// Read side of the audit log. Every method is admin-gated (Director or HR) via
/// [`Permissions::require_admin`]; the write side is the system-level
/// [`AuditProjector`], which performs no permission checks.
pub struct AuditService {
    audit: Arc<dyn AuditRepository>,
    perms: Arc<Permissions>,
}

impl AuditService {
    #[must_use]
    pub fn new(audit: Arc<dyn AuditRepository>, perms: Arc<Permissions>) -> Self {
        Self { audit, perms }
    }

    /// Lists the most recent audit entries across all entities. `before` pages
    /// backwards by `occurred_at`.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not an admin, `NotFound` if the actor
    /// does not exist, or a repository error if the datastore is unavailable.
    pub async fn list_recent(
        &self,
        actor: UserId,
        limit: u32,
        before: Option<OffsetDateTime>,
    ) -> Result<Vec<AuditLog>> {
        self.perms.require_admin(actor).await?;
        Ok(self.audit.list_recent(limit, before).await?)
    }

    /// Lists audit entries for one entity, newest first.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not an admin, `NotFound` if the actor
    /// does not exist, or a repository error if the datastore is unavailable.
    pub async fn list_for_entity(
        &self,
        actor: UserId,
        entity_schema: &str,
        entity_table: &str,
        entity_id: Uuid,
        limit: u32,
    ) -> Result<Vec<AuditLog>> {
        self.perms.require_admin(actor).await?;
        Ok(self
            .audit
            .list_for_entity(entity_schema, entity_table, entity_id, limit)
            .await?)
    }
}
