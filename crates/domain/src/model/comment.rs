use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};

use crate::{
    error::TransitionError,
    ids::{CommentId, RequestId, TicketId, UserId},
    model::EDIT_GRACE,
};

/// The work item a comment hangs off. Two physical tables back this enum; the
/// repository switches on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CommentEntity {
    Request { request_id: RequestId },
    Ticket { ticket_id: TicketId },
}

/// A discussion comment on a request or ticket. Author-editable and deletable within
/// the shared [`EDIT_GRACE`] window, immutable after, so the timeline stays audit-worthy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub id: CommentId,
    pub entity: CommentEntity,
    pub author_user_id: UserId,
    pub body: String,
    pub edited_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
}

impl Comment {
    #[must_use]
    pub fn within_edit_grace(&self, now: OffsetDateTime) -> bool {
        let delta = now - self.created_at;
        delta >= Duration::ZERO && delta <= EDIT_GRACE
    }

    pub fn edit(&mut self, body: String, now: OffsetDateTime) -> Result<(), TransitionError> {
        if !self.within_edit_grace(now) {
            return Err(TransitionError::invalid("comment", "edit_after_grace"));
        }
        self.body = body;
        self.edited_at = Some(now);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Duration;
    use uuid::Uuid;

    fn comment(created_at: OffsetDateTime) -> Comment {
        Comment {
            id: CommentId(Uuid::nil()),
            entity: CommentEntity::Request {
                request_id: RequestId(Uuid::nil()),
            },
            author_user_id: UserId(Uuid::nil()),
            body: "Looks good so far".to_owned(),
            edited_at: None,
            created_at,
        }
    }

    #[test]
    fn within_grace_at_boundaries() {
        let created = OffsetDateTime::UNIX_EPOCH + Duration::days(1);
        let c = comment(created);
        assert!(c.within_edit_grace(created), "delta 0 is within grace");
        assert!(
            c.within_edit_grace(created + EDIT_GRACE),
            "exactly 15m is within grace"
        );
        assert!(
            !c.within_edit_grace(created + EDIT_GRACE + Duration::seconds(1)),
            "15m + 1s is past grace"
        );
        assert!(
            !c.within_edit_grace(created - Duration::seconds(1)),
            "a negative delta (clock skew) is not within grace"
        );
    }

    #[test]
    fn edit_within_grace_updates_body() {
        let created = OffsetDateTime::UNIX_EPOCH + Duration::days(1);
        let mut c = comment(created);
        c.edit("Revised note".to_owned(), created + Duration::minutes(2))
            .unwrap();
        assert_eq!(c.body, "Revised note");
        assert!(c.edited_at.is_some());
    }

    #[test]
    fn edit_after_grace_is_rejected() {
        let created = OffsetDateTime::UNIX_EPOCH + Duration::days(1);
        let mut c = comment(created);
        assert!(
            c.edit("too late".to_owned(), created + Duration::minutes(16))
                .is_err()
        );
        assert_eq!(c.body, "Looks good so far");
    }
}
