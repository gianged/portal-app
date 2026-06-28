use std::sync::Arc;

use domain::{
    ids::{AuditLogId, UserId},
    model::{AuditAction, AuditLog, CommentEntity},
    repository::AuditRepository,
};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{error::Result, events::DomainEvent};

/// System-level handler that projects audited [`DomainEvent`]s into immutable
/// [`AuditLog`] rows. Runs in the worker with no permission checks; records only
/// who/what/which/when, leaving `payload_*` empty to avoid serialising domain
/// structs that carry secrets (e.g. `User.password_hash`).
pub struct AuditProjector {
    audit: Arc<dyn AuditRepository>,
}

impl AuditProjector {
    #[must_use]
    pub fn new(audit: Arc<dyn AuditRepository>) -> Self {
        Self { audit }
    }

    /// Dispatch one event. Events that do not map to a Postgres-backed entity
    /// (chat / announcements live in Scylla) are a logged no-op.
    ///
    /// # Errors
    /// Returns a repository error if appending the audit row fails.
    #[tracing::instrument(skip_all)]
    pub async fn handle(&self, event: &DomainEvent) -> Result<()> {
        use AuditAction::{Assign, Create, Delete, StatusChange, Transfer, Update};
        use DomainEvent as E;

        let entry = match event {
            // --- users: auth.users ---
            E::UserCreated {
                user_id, actor, at, ..
            } => row(Some(*actor), Create, "auth", "users", user_id.0, *at),
            // First login is self-service; no acting admin.
            E::UserActivated { user_id, at, .. } => {
                row(None, StatusChange, "auth", "users", user_id.0, *at)
            }
            E::UserDeactivated {
                user_id, actor, at, ..
            } => row(Some(*actor), StatusChange, "auth", "users", user_id.0, *at),
            E::UserReactivated {
                user_id, actor, at, ..
            } => row(Some(*actor), StatusChange, "auth", "users", user_id.0, *at),
            E::UserProfileUpdated {
                user_id, actor, at, ..
            } => row(Some(*actor), Update, "auth", "users", user_id.0, *at),
            E::UserPasswordChanged { user_id, at } => {
                row(Some(*user_id), Update, "auth", "users", user_id.0, *at)
            }
            E::UserPasswordReset {
                user_id, actor, at, ..
            } => row(Some(*actor), Update, "auth", "users", user_id.0, *at),

            // --- groups: org.groups ---
            E::GroupCreated {
                group_id,
                actor,
                at,
                ..
            } => row(Some(*actor), Create, "org", "groups", group_id.0, *at),
            E::GroupDeleted {
                group_id,
                actor,
                at,
                ..
            } => row(Some(*actor), Delete, "org", "groups", group_id.0, *at),
            E::GroupMetadataUpdated {
                group_id,
                actor,
                at,
                ..
            } => row(Some(*actor), Update, "org", "groups", group_id.0, *at),
            E::LeadershipTransferred {
                group_id,
                actor,
                at,
                ..
            } => row(Some(*actor), Transfer, "org", "groups", group_id.0, *at),

            // --- memberships: org.memberships ---
            E::MembershipAdded {
                membership_id,
                actor,
                at,
                ..
            } => row(
                Some(*actor),
                Create,
                "org",
                "memberships",
                membership_id.0,
                *at,
            ),
            E::MembershipRoleChanged {
                membership_id,
                actor,
                at,
                ..
            } => row(
                Some(*actor),
                Update,
                "org",
                "memberships",
                membership_id.0,
                *at,
            ),
            E::MembershipDeactivated {
                membership_id,
                actor,
                at,
                ..
            } => row(
                Some(*actor),
                Delete,
                "org",
                "memberships",
                membership_id.0,
                *at,
            ),

            // --- projects: project.projects ---
            E::ProjectCreated {
                project_id,
                actor,
                at,
                ..
            } => row(
                Some(*actor),
                Create,
                "project",
                "projects",
                project_id.0,
                *at,
            ),
            E::ProjectMetadataUpdated {
                project_id,
                actor,
                at,
                ..
            } => row(
                Some(*actor),
                Update,
                "project",
                "projects",
                project_id.0,
                *at,
            ),
            E::ProjectStatusChanged {
                project_id,
                actor,
                at,
                ..
            } => row(
                Some(*actor),
                StatusChange,
                "project",
                "projects",
                project_id.0,
                *at,
            ),
            E::ProjectCollaboratorRemoved {
                project_id,
                actor,
                at,
                ..
            } => row(
                Some(*actor),
                Delete,
                "project",
                "projects",
                project_id.0,
                *at,
            ),

            // --- project invites: project.project_invites ---
            E::ProjectInviteSent {
                invite_id,
                actor,
                at,
                ..
            } => row(
                Some(*actor),
                Create,
                "project",
                "project_invites",
                invite_id.0,
                *at,
            ),
            E::ProjectInviteResponded {
                invite_id,
                actor,
                at,
                ..
            } => row(
                Some(*actor),
                StatusChange,
                "project",
                "project_invites",
                invite_id.0,
                *at,
            ),

            // --- requests: project.requests ---
            E::RequestCreated {
                request_id,
                actor,
                at,
                ..
            } => row(
                Some(*actor),
                Create,
                "project",
                "requests",
                request_id.0,
                *at,
            ),
            E::RequestMetadataUpdated {
                request_id,
                actor,
                at,
                ..
            } => row(
                Some(*actor),
                Update,
                "project",
                "requests",
                request_id.0,
                *at,
            ),
            E::RequestAssigned {
                request_id,
                actor,
                at,
                ..
            } => row(
                Some(*actor),
                Assign,
                "project",
                "requests",
                request_id.0,
                *at,
            ),
            E::RequestStatusChanged {
                request_id,
                actor,
                at,
                ..
            } => row(
                Some(*actor),
                StatusChange,
                "project",
                "requests",
                request_id.0,
                *at,
            ),

            // --- tickets: ticket.tickets ---
            E::TicketRaised {
                ticket_id,
                requester,
                at,
                ..
            } => row(
                Some(*requester),
                Create,
                "ticket",
                "tickets",
                ticket_id.0,
                *at,
            ),
            E::TicketTriaged {
                ticket_id,
                actor,
                at,
                ..
            } => row(Some(*actor), Update, "ticket", "tickets", ticket_id.0, *at),
            E::TicketAssigned {
                ticket_id,
                actor,
                at,
                ..
            } => row(Some(*actor), Assign, "ticket", "tickets", ticket_id.0, *at),
            E::TicketStatusChanged {
                ticket_id,
                actor,
                at,
                ..
            } => row(
                Some(*actor),
                StatusChange,
                "ticket",
                "tickets",
                ticket_id.0,
                *at,
            ),
            // System action, no actor (precedent: UserActivated).
            E::TicketAutoClosed { ticket_id, at } => {
                row(None, StatusChange, "ticket", "tickets", ticket_id.0, *at)
            }

            // --- comments: project.request_comments / ticket.ticket_comments ---
            E::CommentAdded {
                comment_id,
                entity,
                actor,
                at,
                ..
            } => comment_row(*entity, Some(*actor), Create, comment_id.0, *at),
            E::CommentEdited {
                comment_id,
                entity,
                actor,
                at,
                ..
            } => comment_row(*entity, Some(*actor), Update, comment_id.0, *at),
            E::CommentDeleted {
                comment_id,
                entity,
                actor,
                at,
            } => comment_row(*entity, Some(*actor), Delete, comment_id.0, *at),

            // Chat / announcements live in Scylla, not the Postgres audit log.
            other => {
                tracing::debug!(topic = other.topic(), "audit: event not projected");
                return Ok(());
            }
        };

        self.audit.append(&entry).await?;
        Ok(())
    }
}

/// [`row`] with the schema/table derived from the comment's parent entity.
fn comment_row(
    entity: CommentEntity,
    actor: Option<UserId>,
    action: AuditAction,
    entity_id: Uuid,
    occurred_at: OffsetDateTime,
) -> AuditLog {
    let (schema, table) = match entity {
        CommentEntity::Request { .. } => ("project", "request_comments"),
        CommentEntity::Ticket { .. } => ("ticket", "ticket_comments"),
    };
    row(actor, action, schema, table, entity_id, occurred_at)
}

/// Builds an immutable audit row. `payload_*` stay `None` (see the type doc).
fn row(
    actor: Option<UserId>,
    action: AuditAction,
    schema: &str,
    table: &str,
    entity_id: Uuid,
    occurred_at: OffsetDateTime,
) -> AuditLog {
    AuditLog {
        id: AuditLogId(Uuid::now_v7()),
        actor_user_id: actor,
        action,
        entity_schema: schema.to_owned(),
        entity_table: table.to_owned(),
        entity_id,
        payload_before: None,
        payload_after: None,
        occurred_at,
    }
}
