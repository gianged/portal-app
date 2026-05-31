use std::sync::Arc;

use domain::{
    ids::{ChannelId, MessageId, UserId},
    model::{Channel, ChannelKind, ChannelMembership, DirectChannel, Message, UserStatus},
    repository::{ChatRepository, UserRepository},
};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::{
    commands::chat::PostMessageCommand,
    error::{Error, Result},
    events::{DomainEvent, EventBus},
    permissions::Permissions,
};

/// Members can delete only their own messages within this window after posting.
/// Group-channel leaders bypass this grace and can delete anytime.
const MESSAGE_DELETE_GRACE: Duration = Duration::minutes(15);

/// A channel-list row enriched with its latest-activity timestamp and an unread
/// flag (latest message newer than the member's `last_read_at`).
#[derive(Debug, Clone)]
pub struct ChannelOverview {
    pub membership: ChannelMembership,
    pub last_message_at: Option<OffsetDateTime>,
    pub unread: bool,
}

pub struct ChatService {
    chats: Arc<dyn ChatRepository>,
    users: Arc<dyn UserRepository>,
    perms: Arc<Permissions>,
    events: Arc<EventBus>,
}

impl ChatService {
    #[must_use]
    pub fn new(
        chats: Arc<dyn ChatRepository>,
        users: Arc<dyn UserRepository>,
        perms: Arc<Permissions>,
        events: Arc<EventBus>,
    ) -> Self {
        Self {
            chats,
            users,
            perms,
            events,
        }
    }

    /// Looks up a channel by id, returning `None` if it does not exist.
    ///
    /// # Errors
    /// Returns a repository error if the datastore is unavailable.
    pub async fn find_channel(&self, id: ChannelId) -> Result<Option<Channel>> {
        Ok(self.chats.find_channel(id).await?)
    }

    /// Idempotent: returns an existing direct channel between the two users if
    /// one exists, otherwise creates one. The pair is canonicalised by
    /// `DirectChannel::new` so order doesn't matter.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active, `Validation` if the actor
    /// targets themselves, `NotFound` if the other user does not exist, `Conflict`
    /// if the recipient is not active, or a repository error if the datastore is
    /// unavailable.
    pub async fn open_direct_channel(&self, actor: UserId, other_user: UserId) -> Result<Channel> {
        self.perms.require_active(actor).await?;
        if actor == other_user {
            return Err(Error::Validation("cannot_dm_self".into()));
        }
        let other = self
            .users
            .find_by_id(other_user)
            .await?
            .ok_or(Error::NotFound("user"))?;
        if other.status != UserStatus::Active {
            return Err(Error::Conflict("recipient_not_active".into()));
        }

        if let Some(existing) = self.chats.find_direct_channel(actor, other_user).await? {
            return Ok(existing);
        }

        let now = OffsetDateTime::now_utc();
        let id = ChannelId(Uuid::now_v7());
        let direct = DirectChannel::new(id, actor, other_user, now);
        let channel = Channel::Direct(direct);
        self.chats.save_channel(&channel).await?;
        // Subscribe both participants so the channel shows up in their lists.
        // Direct-channel read access is enforced by identity (see
        // `Permissions::require_can_view_channel`), not OpenFGA, so there is no
        // participant tuple to write — and thus no Director backdoor.
        self.chats
            .subscribe_member(actor, id, ChannelKind::Direct)
            .await?;
        self.chats
            .subscribe_member(other_user, id, ChannelKind::Direct)
            .await?;
        Ok(channel)
    }

    /// Lists the channels the actor is subscribed to.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active, or a repository error if
    /// the datastore is unavailable.
    pub async fn list_channels(&self, actor: UserId) -> Result<Vec<ChannelMembership>> {
        self.perms.require_active(actor).await?;
        Ok(self.chats.list_channels_for_user(actor).await?)
    }

    /// Channel list with per-channel unread state and last-activity time. Reads
    /// the single latest message per channel (`list_messages(_, None, 1)`); an
    /// N-read pass that is acceptable at this scale.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active, or a repository error if
    /// the datastore is unavailable.
    pub async fn channel_overviews(&self, actor: UserId) -> Result<Vec<ChannelOverview>> {
        let memberships = self.list_channels(actor).await?;
        let mut out = Vec::with_capacity(memberships.len());
        for membership in memberships {
            let latest = self
                .chats
                .list_messages(membership.channel_id, None, 1)
                .await?
                .into_iter()
                .next();
            let last_message_at = latest.as_ref().map(|m| uuid_v7_created_at(m.id.0));
            let unread = match (last_message_at, membership.last_read_at) {
                (Some(at), Some(read)) => at > read,
                (Some(_), None) => true,
                (None, _) => false,
            };
            out.push(ChannelOverview {
                membership,
                last_message_at,
                unread,
            });
        }
        Ok(out)
    }

    /// Lists messages in a channel the actor may view, newest first.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active or cannot view the channel,
    /// `NotFound` if the channel does not exist, or a repository error if the
    /// datastore is unavailable.
    pub async fn list_messages(
        &self,
        actor: UserId,
        channel_id: ChannelId,
        before: Option<MessageId>,
        limit: u32,
    ) -> Result<Vec<Message>> {
        self.perms.require_active(actor).await?;
        let channel = self
            .chats
            .find_channel(channel_id)
            .await?
            .ok_or(Error::NotFound("channel"))?;
        self.perms.require_can_view_channel(actor, &channel).await?;
        Ok(self.chats.list_messages(channel_id, before, limit).await?)
    }

    /// Posts a message to a channel the actor may post in.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active or cannot post in the
    /// channel, `NotFound` if the channel does not exist, a repository error if
    /// the datastore is unavailable, or an event error if the event bus fails.
    pub async fn post_message(&self, actor: UserId, cmd: PostMessageCommand) -> Result<Message> {
        self.perms.require_active(actor).await?;
        let channel = self
            .chats
            .find_channel(cmd.channel_id)
            .await?
            .ok_or(Error::NotFound("channel"))?;
        self.perms
            .require_can_post_in_channel(actor, &channel)
            .await?;

        let now = OffsetDateTime::now_utc();
        let message = Message {
            id: MessageId(Uuid::now_v7()),
            channel_id: cmd.channel_id,
            sender_user_id: actor,
            body: cmd.body,
            mentions: cmd.mentions,
            attachment_keys: cmd.attachment_keys,
            is_announcement: false,
            edited_at: None,
            deleted_at: None,
        };
        self.chats.save_message(&message).await?;
        self.events
            .emit(DomainEvent::MessagePosted {
                message_id: message.id,
                channel_id: message.channel_id,
                sender: actor,
                mentions: message.mentions.clone(),
                at: now,
                after: message.clone(),
            })
            .await?;
        Ok(message)
    }

    /// Deletes a message. The sender may delete within the grace window;
    /// channel moderators may delete at any time.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active or is neither the sender
    /// within the grace window nor a channel moderator, `NotFound` if the channel
    /// or message does not exist, a repository error if the datastore is
    /// unavailable, or an event error if the event bus fails.
    pub async fn delete_message(
        &self,
        actor: UserId,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> Result<()> {
        self.perms.require_active(actor).await?;
        let channel = self
            .chats
            .find_channel(channel_id)
            .await?
            .ok_or(Error::NotFound("channel"))?;

        let mut message = self
            .chats
            .find_message(channel_id, message_id)
            .await?
            .ok_or(Error::NotFound("message"))?;
        if message.is_deleted() {
            return Ok(());
        }

        let now = OffsetDateTime::now_utc();
        let is_sender = message.sender_user_id == actor;
        let within_grace = now - uuid_v7_created_at(message.id.0) <= MESSAGE_DELETE_GRACE;
        let is_moderator = self
            .perms
            .user_is_channel_moderator(actor, &channel)
            .await?;

        if !((is_sender && within_grace) || is_moderator) {
            return Err(Error::Forbidden);
        }

        message.delete(now);
        self.chats.save_message(&message).await?;
        self.events
            .emit(DomainEvent::MessageDeleted {
                message_id,
                channel_id,
                actor,
                at: now,
            })
            .await?;
        Ok(())
    }

    /// Edit a regular message. Sender-only (moderators delete, they don't
    /// rewrite others' words) and only within the post grace window. Announcements
    /// have their own edit path and are rejected here.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active or is not the sender within
    /// the grace window, `NotFound` if the message does not exist, `Conflict` if
    /// the message is deleted or is an announcement, a repository error if the
    /// datastore is unavailable, or an event error if the event bus fails.
    pub async fn edit_message(
        &self,
        actor: UserId,
        channel_id: ChannelId,
        message_id: MessageId,
        body: String,
    ) -> Result<Message> {
        self.perms.require_active(actor).await?;
        let mut message = self
            .chats
            .find_message(channel_id, message_id)
            .await?
            .ok_or(Error::NotFound("message"))?;
        if message.is_deleted() {
            return Err(Error::Conflict("message_deleted".into()));
        }
        if message.is_announcement {
            return Err(Error::Conflict("cannot_edit_announcement".into()));
        }

        let now = OffsetDateTime::now_utc();
        let within_grace = now - uuid_v7_created_at(message.id.0) <= MESSAGE_DELETE_GRACE;
        if message.sender_user_id != actor || !within_grace {
            return Err(Error::Forbidden);
        }

        message.edit(body, now);
        self.chats.save_message(&message).await?;
        self.events
            .emit(DomainEvent::MessageEdited {
                message_id,
                channel_id,
                actor,
                at: now,
                after: message.clone(),
            })
            .await?;
        Ok(message)
    }

    /// Marks the channel read for the actor up to the current time.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active or cannot view the channel,
    /// `NotFound` if the channel does not exist, or a repository error if the
    /// datastore is unavailable.
    pub async fn mark_read(&self, actor: UserId, channel_id: ChannelId) -> Result<()> {
        self.perms.require_active(actor).await?;
        let channel = self
            .chats
            .find_channel(channel_id)
            .await?
            .ok_or(Error::NotFound("channel"))?;
        self.perms.require_can_view_channel(actor, &channel).await?;
        let now = OffsetDateTime::now_utc();
        self.chats.update_last_read(actor, channel_id, now).await?;
        Ok(())
    }
}

/// Recover the creation timestamp embedded in a `UUIDv7` ID. Used for the
/// 15-minute message-delete grace window since `Message` itself doesn't store
/// `created_at` — the time is implicit in the id.
fn uuid_v7_created_at(id: Uuid) -> OffsetDateTime {
    let ts = id
        .get_timestamp()
        .expect("UUIDv7 ID always carries an embedded timestamp");
    let (secs, nanos) = ts.to_unix();
    let total_nanos = i128::from(secs) * 1_000_000_000 + i128::from(nanos);
    OffsetDateTime::from_unix_timestamp_nanos(total_nanos)
        .expect("UUIDv7 timestamp falls within OffsetDateTime range")
}
