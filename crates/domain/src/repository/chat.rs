use async_trait::async_trait;
use time::OffsetDateTime;

use crate::{
    error::RepositoryError,
    ids::{ChannelId, MessageId, UserId},
    model::{Announcement, Channel, ChannelMembership, Message},
};

#[async_trait]
pub trait ChatRepository: Send + Sync {
    async fn find_channel(&self, id: ChannelId) -> Result<Option<Channel>, RepositoryError>;

    /// Lookup using the `direct_channel_by_users` table; users should be passed
    /// in any order — the impl canonicalises via `DirectChannel::new`.
    async fn find_direct_channel(
        &self,
        a: UserId,
        b: UserId,
    ) -> Result<Option<Channel>, RepositoryError>;

    async fn save_channel(&self, channel: &Channel) -> Result<(), RepositoryError>;

    async fn list_channels_for_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<ChannelMembership>, RepositoryError>;

    async fn update_last_read(
        &self,
        user_id: UserId,
        channel_id: ChannelId,
        at: OffsetDateTime,
    ) -> Result<(), RepositoryError>;

    /// Reverse-chronological page of messages. `before` cursors at a known
    /// `MessageId` (exclusive) for pagination; `None` returns the latest page.
    async fn list_messages(
        &self,
        channel_id: ChannelId,
        before: Option<MessageId>,
        limit: u32,
    ) -> Result<Vec<Message>, RepositoryError>;

    /// Single-message lookup; needed for moderation (delete, edit-grace check).
    async fn find_message(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> Result<Option<Message>, RepositoryError>;

    async fn save_message(&self, message: &Message) -> Result<(), RepositoryError>;

    async fn find_announcement(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> Result<Option<Announcement>, RepositoryError>;

    async fn list_announcements(
        &self,
        channel_id: ChannelId,
    ) -> Result<Vec<Announcement>, RepositoryError>;

    async fn save_announcement(&self, announcement: &Announcement) -> Result<(), RepositoryError>;

    async fn delete_announcement(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> Result<(), RepositoryError>;
}
