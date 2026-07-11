use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};

use crate::{
    error::TransitionError,
    ids::{ChannelId, MessageId, UserId},
};

/// Announcements are editable for the first 15 minutes after creation; past
/// that, they are considered published and immutable.
pub const EDIT_GRACE: Duration = Duration::minutes(15);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Announcement {
    pub id: MessageId,
    pub channel_id: ChannelId,
    pub sender_user_id: UserId,
    pub body: String,
    pub edited_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
}

impl Announcement {
    #[must_use]
    pub fn within_edit_grace(&self, now: OffsetDateTime) -> bool {
        let delta = now - self.created_at;
        delta >= Duration::ZERO && delta <= EDIT_GRACE
    }

    /// Replaces the body, only within the edit grace window.
    ///
    /// # Errors
    /// Returns [`TransitionError::EditGraceExpired`] past the window.
    pub fn edit(&mut self, body: String, now: OffsetDateTime) -> Result<(), TransitionError> {
        if !self.within_edit_grace(now) {
            return Err(TransitionError::EditGraceExpired {
                entity: "announcement",
            });
        }
        self.body = body;
        self.edited_at = Some(now);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn announcement(created_at: OffsetDateTime) -> Announcement {
        Announcement {
            id: MessageId(Uuid::nil()),
            channel_id: ChannelId(Uuid::nil()),
            sender_user_id: UserId(Uuid::nil()),
            body: "Town hall at 3pm".to_owned(),
            edited_at: None,
            created_at,
        }
    }

    #[test]
    fn within_grace_at_boundaries() {
        let created = OffsetDateTime::UNIX_EPOCH + Duration::days(1);
        let a = announcement(created);
        assert!(a.within_edit_grace(created), "delta 0 is within grace");
        assert!(
            a.within_edit_grace(created + EDIT_GRACE),
            "exactly 15m is within grace"
        );
        assert!(
            !a.within_edit_grace(created + EDIT_GRACE + Duration::seconds(1)),
            "15m + 1s is past grace"
        );
        assert!(
            !a.within_edit_grace(created - Duration::seconds(1)),
            "a negative delta (clock skew) is not within grace"
        );
    }

    #[test]
    fn edit_within_grace_updates_body() {
        let created = OffsetDateTime::UNIX_EPOCH + Duration::days(1);
        let mut a = announcement(created);
        a.edit("Moved to 4pm".to_owned(), created + Duration::minutes(2))
            .unwrap();
        assert_eq!(a.body, "Moved to 4pm");
        assert!(a.edited_at.is_some());
    }

    #[test]
    fn edit_after_grace_is_rejected() {
        let created = OffsetDateTime::UNIX_EPOCH + Duration::days(1);
        let mut a = announcement(created);
        let result = a.edit("too late".to_owned(), created + Duration::minutes(16));
        assert!(result.is_err());
        assert_eq!(
            a.body, "Town hall at 3pm",
            "body unchanged after failed edit"
        );
    }
}
