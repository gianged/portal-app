use std::sync::Arc;

use domain::{
    ids::{CommentId, UserId},
    model::{Comment, CommentEntity},
    repository::{CommentRepository, RequestRepository, TicketRepository},
};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    error::{Error, Result},
    events::{DomainEvent, EventBus},
    permissions::Permissions,
};

/// Discussion comments on requests and tickets; one service for both parents,
/// author-editable within the domain's shared 15-minute grace window and immutable
/// afterwards. Only the view gate differs ([`Self::require_can_view`]).
pub struct CommentService {
    comments: Arc<dyn CommentRepository>,
    requests: Arc<dyn RequestRepository>,
    tickets: Arc<dyn TicketRepository>,
    perms: Arc<Permissions>,
    events: Arc<EventBus>,
}

impl CommentService {
    #[must_use]
    pub fn new(
        comments: Arc<dyn CommentRepository>,
        requests: Arc<dyn RequestRepository>,
        tickets: Arc<dyn TicketRepository>,
        perms: Arc<Permissions>,
        events: Arc<EventBus>,
    ) -> Self {
        Self {
            comments,
            requests,
            tickets,
            perms,
            events,
        }
    }

    /// Commenting rights == viewing rights: project view for a request's comments,
    /// ticket view for a ticket's.
    async fn require_can_view(&self, actor: UserId, entity: CommentEntity) -> Result<()> {
        self.perms.require_active(actor).await?;
        match entity {
            CommentEntity::Request { request_id } => {
                let request = self
                    .requests
                    .find_by_id(request_id)
                    .await?
                    .ok_or(Error::NotFound("request"))?;
                self.perms
                    .require_can_view_project(actor, request.project_id)
                    .await
            }
            CommentEntity::Ticket { ticket_id } => {
                self.tickets
                    .find_by_id(ticket_id)
                    .await?
                    .ok_or(Error::NotFound("ticket"))?;
                self.perms.require_can_view_ticket(actor, ticket_id).await
            }
        }
    }

    /// Adds a comment to a request/ticket the actor may view.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor cannot view the parent, `NotFound` if
    /// the parent does not exist, or a repository/event error.
    pub async fn add(&self, actor: UserId, entity: CommentEntity, body: String) -> Result<Comment> {
        self.require_can_view(actor, entity).await?;
        let now = OffsetDateTime::now_utc();
        let comment = Comment {
            id: CommentId(Uuid::now_v7()),
            entity,
            author_user_id: actor,
            body,
            edited_at: None,
            created_at: now,
        };
        self.comments.save(&comment).await?;
        self.events
            .emit(DomainEvent::CommentAdded {
                comment_id: comment.id,
                entity,
                actor,
                at: now,
                after: comment.clone(),
            })
            .await?;
        Ok(comment)
    }

    /// Newest-first comment page for a request/ticket the actor may view.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor cannot view the parent, `NotFound` if
    /// the parent does not exist, or a repository error.
    pub async fn list(
        &self,
        actor: UserId,
        entity: CommentEntity,
        before: Option<CommentId>,
        limit: u32,
    ) -> Result<Vec<Comment>> {
        self.require_can_view(actor, entity).await?;
        Ok(self.comments.list_for_entity(entity, before, limit).await?)
    }

    /// Author-only edit, within the grace window (enforced in the domain).
    ///
    /// # Errors
    /// Returns `NotFound` if the comment is missing, `Forbidden` for a
    /// non-author, `Transition` past the grace window, or a repository/event
    /// error.
    pub async fn edit(
        &self,
        actor: UserId,
        entity: CommentEntity,
        comment_id: CommentId,
        body: String,
    ) -> Result<Comment> {
        self.require_can_view(actor, entity).await?;
        let mut comment = self
            .comments
            .find_by_id(entity, comment_id)
            .await?
            .ok_or(Error::NotFound("comment"))?;
        if comment.author_user_id != actor {
            return Err(Error::Forbidden);
        }
        let now = OffsetDateTime::now_utc();
        comment.edit(body, now)?;
        self.comments.save(&comment).await?;
        self.events
            .emit(DomainEvent::CommentEdited {
                comment_id,
                entity,
                actor,
                at: now,
                after: comment.clone(),
            })
            .await?;
        Ok(comment)
    }

    /// Author-only hard delete, within the same grace window as edit
    /// (announcements precedent); the audit log keeps the trail.
    ///
    /// # Errors
    /// Returns `NotFound` if the comment is missing, `Forbidden` for a
    /// non-author or past the grace window, or a repository/event error.
    pub async fn remove(
        &self,
        actor: UserId,
        entity: CommentEntity,
        comment_id: CommentId,
    ) -> Result<()> {
        self.require_can_view(actor, entity).await?;
        let comment = self
            .comments
            .find_by_id(entity, comment_id)
            .await?
            .ok_or(Error::NotFound("comment"))?;
        let now = OffsetDateTime::now_utc();
        if comment.author_user_id != actor || !comment.within_edit_grace(now) {
            return Err(Error::Forbidden);
        }
        self.comments.delete(entity, comment_id).await?;
        self.events
            .emit(DomainEvent::CommentDeleted {
                comment_id,
                entity,
                actor,
                at: now,
            })
            .await?;
        Ok(())
    }
}
