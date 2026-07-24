use async_trait::async_trait;
use time::OffsetDateTime;

use crate::{
    error::RepositoryError,
    ids::{ChannelId, GroupId, MessageId, UserId},
    model::{Announcement, Channel, ChannelKind, ChannelMembership, Message},
};

#[async_trait]
pub trait ChatRepository: Send + Sync {
    async fn find_channel(&self, id: ChannelId) -> Result<Option<Channel>, RepositoryError>;

    /// Lookup using the `direct_channel_by_users` table; users may be passed in any
    /// order, the impl canonicalises via `DirectChannel::new`.
    async fn find_direct_channel(
        &self,
        a: UserId,
        b: UserId,
    ) -> Result<Option<Channel>, RepositoryError>;

    async fn save_channel(&self, channel: &Channel) -> Result<(), RepositoryError>;

    /// The group's channel (1:1 with the group), used to subscribe members when
    /// they join. Looked up via the `group_channel_by_group` table.
    async fn find_group_channel(
        &self,
        group_id: GroupId,
    ) -> Result<Option<Channel>, RepositoryError>;

    /// The single company-wide general channel, if it has been created.
    async fn find_general_channel(&self) -> Result<Option<Channel>, RepositoryError>;

    /// Add a `channels_by_user` row so `channel_id` appears in the user's channel
    /// list. Idempotent: re-subscribing an existing member is harmless.
    async fn subscribe_member(
        &self,
        user_id: UserId,
        channel_id: ChannelId,
        kind: ChannelKind,
    ) -> Result<(), RepositoryError>;

    /// Remove the user's `channels_by_user` row (e.g. on membership deactivation).
    async fn unsubscribe_member(
        &self,
        user_id: UserId,
        channel_id: ChannelId,
    ) -> Result<(), RepositoryError>;

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

    /// Persist a batch of messages. Default loops `save_message`; adapters override
    /// for true backend batching.
    async fn save_messages(&self, messages: &[Message]) -> Result<(), RepositoryError> {
        for message in messages {
            self.save_message(message).await?;
        }
        Ok(())
    }

    async fn find_announcement(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> Result<Option<Announcement>, RepositoryError>;

    async fn list_announcements(
        &self,
        channel_id: ChannelId,
        limit: u32,
    ) -> Result<Vec<Announcement>, RepositoryError>;

    /// Atomically persists the announcement rail row and its chat-timeline copy
    /// (one logged batch), so the pair cannot diverge.
    async fn save_announcement_with_message(
        &self,
        announcement: &Announcement,
        message: &Message,
    ) -> Result<(), RepositoryError>;

    /// Atomically removes the announcement rail row and its chat-timeline copy.
    async fn delete_announcement_with_message(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> Result<(), RepositoryError>;
}
