use async_trait::async_trait;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use domain::{
    error::RepositoryError,
    ids::{TicketId, UserId},
    model::Ticket,
    repository::TicketRepository,
};

use crate::postgres::{
    enums::{SqlTicketCategory, SqlTicketPriority, SqlTicketStatus},
    mappers::{like_pattern, map_pg_error},
};

pub struct PgTicketRepo {
    pool: PgPool,
}

impl PgTicketRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct TicketRow {
    id: Uuid,
    requester_user_id: Uuid,
    assignee_user_id: Option<Uuid>,
    title: String,
    description: String,
    status: SqlTicketStatus,
    priority: Option<SqlTicketPriority>,
    category: SqlTicketCategory,
    triaged_at: Option<OffsetDateTime>,
    resolved_at: Option<OffsetDateTime>,
    closed_at: Option<OffsetDateTime>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl From<TicketRow> for Ticket {
    fn from(r: TicketRow) -> Self {
        Self {
            id: TicketId(r.id),
            requester_user_id: UserId(r.requester_user_id),
            assignee_user_id: r.assignee_user_id.map(UserId),
            title: r.title,
            description: r.description,
            status: r.status.into(),
            priority: r.priority.map(Into::into),
            category: r.category.into(),
            triaged_at: r.triaged_at,
            resolved_at: r.resolved_at,
            closed_at: r.closed_at,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[async_trait]
impl TicketRepository for PgTicketRepo {
    async fn find_by_id(&self, id: TicketId) -> Result<Option<Ticket>, RepositoryError> {
        sqlx::query_as!(
            TicketRow,
            r#"SELECT
                 id,
                 requester_user_id,
                 assignee_user_id,
                 title,
                 description,
                 status   AS "status: SqlTicketStatus",
                 priority AS "priority: SqlTicketPriority",
                 category AS "category: SqlTicketCategory",
                 triaged_at,
                 resolved_at,
                 closed_at,
                 created_at,
                 updated_at
               FROM ticket.tickets
               WHERE id = $1"#,
            id.0,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_pg_error)
        .map(|opt| opt.map(Into::into))
    }

    async fn list_open_for_triage(
        &self,
        limit: u32,
        q: Option<&str>,
    ) -> Result<Vec<Ticket>, RepositoryError> {
        // Status set matches idx_tickets_status_priority_open. Priority NULLS LAST
        // pushes un-triaged tickets to the end so triaged-urgent surfaces first;
        // created_at breaks ties for stable ordering.
        let pattern: Option<String> = q.map(like_pattern);
        let rows = sqlx::query_as!(
            TicketRow,
            r#"SELECT
                 id,
                 requester_user_id,
                 assignee_user_id,
                 title,
                 description,
                 status   AS "status: SqlTicketStatus",
                 priority AS "priority: SqlTicketPriority",
                 category AS "category: SqlTicketCategory",
                 triaged_at,
                 resolved_at,
                 closed_at,
                 created_at,
                 updated_at
               FROM ticket.tickets
               WHERE status IN ('open', 'triaged', 'assigned', 'in_progress', 'reopened')
                 AND ($2::text IS NULL OR title ILIKE $2)
               ORDER BY priority NULLS LAST, created_at
               LIMIT $1"#,
            i64::from(limit),
            pattern,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn list_for_assignee(
        &self,
        assignee: UserId,
        q: Option<&str>,
    ) -> Result<Vec<Ticket>, RepositoryError> {
        // Matches idx_tickets_assignee_user_id (partial: assignee_user_id IS NOT NULL).
        let pattern: Option<String> = q.map(like_pattern);
        let rows = sqlx::query_as!(
            TicketRow,
            r#"SELECT
                 id,
                 requester_user_id,
                 assignee_user_id,
                 title,
                 description,
                 status   AS "status: SqlTicketStatus",
                 priority AS "priority: SqlTicketPriority",
                 category AS "category: SqlTicketCategory",
                 triaged_at,
                 resolved_at,
                 closed_at,
                 created_at,
                 updated_at
               FROM ticket.tickets
               WHERE assignee_user_id = $1
                 AND ($2::text IS NULL OR title ILIKE $2)
               ORDER BY created_at DESC"#,
            assignee.0,
            pattern,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn list_for_requester(
        &self,
        requester: UserId,
        q: Option<&str>,
    ) -> Result<Vec<Ticket>, RepositoryError> {
        let pattern: Option<String> = q.map(like_pattern);
        let rows = sqlx::query_as!(
            TicketRow,
            r#"SELECT
                 id,
                 requester_user_id,
                 assignee_user_id,
                 title,
                 description,
                 status   AS "status: SqlTicketStatus",
                 priority AS "priority: SqlTicketPriority",
                 category AS "category: SqlTicketCategory",
                 triaged_at,
                 resolved_at,
                 closed_at,
                 created_at,
                 updated_at
               FROM ticket.tickets
               WHERE requester_user_id = $1
                 AND ($2::text IS NULL OR title ILIKE $2)
               ORDER BY created_at DESC"#,
            requester.0,
            pattern,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn list_resolved_before(
        &self,
        cutoff: OffsetDateTime,
        limit: u32,
    ) -> Result<Vec<Ticket>, RepositoryError> {
        // Auto-close work list: resolved tickets whose reopen window has lapsed.
        let rows = sqlx::query_as!(
            TicketRow,
            r#"SELECT
                 id,
                 requester_user_id,
                 assignee_user_id,
                 title,
                 description,
                 status   AS "status: SqlTicketStatus",
                 priority AS "priority: SqlTicketPriority",
                 category AS "category: SqlTicketCategory",
                 triaged_at,
                 resolved_at,
                 closed_at,
                 created_at,
                 updated_at
               FROM ticket.tickets
               WHERE status = 'resolved'
                 AND resolved_at <= $1
               ORDER BY resolved_at
               LIMIT $2"#,
            cutoff,
            i64::from(limit),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn save(&self, ticket: &Ticket) -> Result<(), RepositoryError> {
        let status = SqlTicketStatus::from(ticket.status);
        let priority: Option<SqlTicketPriority> = ticket.priority.map(Into::into);
        let category = SqlTicketCategory::from(ticket.category);
        sqlx::query!(
            r#"INSERT INTO ticket.tickets
                 (id, requester_user_id, assignee_user_id, title, description,
                  status, priority, category, triaged_at, resolved_at, closed_at,
                  created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
               ON CONFLICT (id) DO UPDATE SET
                 requester_user_id = EXCLUDED.requester_user_id,
                 assignee_user_id  = EXCLUDED.assignee_user_id,
                 title             = EXCLUDED.title,
                 description       = EXCLUDED.description,
                 status            = EXCLUDED.status,
                 priority          = EXCLUDED.priority,
                 category          = EXCLUDED.category,
                 triaged_at        = EXCLUDED.triaged_at,
                 resolved_at       = EXCLUDED.resolved_at,
                 closed_at         = EXCLUDED.closed_at"#,
            ticket.id.0,
            ticket.requester_user_id.0,
            ticket.assignee_user_id.map(|u| u.0),
            ticket.title,
            ticket.description,
            status as SqlTicketStatus,
            priority as Option<SqlTicketPriority>,
            category as SqlTicketCategory,
            ticket.triaged_at,
            ticket.resolved_at,
            ticket.closed_at,
            ticket.created_at,
        )
        .execute(&self.pool)
        .await
        .map_err(map_pg_error)?;
        Ok(())
    }
}
