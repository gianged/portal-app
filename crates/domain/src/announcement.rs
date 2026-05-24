use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};

use crate::{
    error::TransitionError,
    ids::{ChannelId, MessageId, UserId},
};

/// Edit grace window per CLAUDE.md invariant 6: announcements are immutable
/// after 15 minutes from creation.
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

    pub fn edit(
        &mut self,
        body: String,
        now: OffsetDateTime,
    ) -> Result<(), TransitionError> {
        if !self.within_edit_grace(now) {
            return Err(TransitionError::invalid("announcement", "edit_after_grace"));
        }
        self.body = body;
        self.edited_at = Some(now);
        Ok(())
    }
}
