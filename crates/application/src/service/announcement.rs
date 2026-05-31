use std::sync::Arc;

use domain::{
    ids::{ChannelId, MessageId, UserId},
    model::{Announcement, Message},
    repository::ChatRepository,
};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    commands::chat::PostAnnouncementCommand,
    error::{Error, Result},
    events::{DomainEvent, EventBus},
    permissions::Permissions,
};

pub struct AnnouncementService {
    chats: Arc<dyn ChatRepository>,
    perms: Arc<Permissions>,
    events: Arc<EventBus>,
}

impl AnnouncementService {
    #[must_use]
    pub fn new(
        chats: Arc<dyn ChatRepository>,
        perms: Arc<Permissions>,
        events: Arc<EventBus>,
    ) -> Self {
        Self {
            chats,
            perms,
            events,
        }
    }

    /// Posts an announcement to a channel the actor may announce in.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active or cannot announce in the
    /// channel, `NotFound` if the channel does not exist, a repository error if
    /// the datastore is unavailable, or an event error if the event bus fails.
    pub async fn post(&self, actor: UserId, cmd: PostAnnouncementCommand) -> Result<Announcement> {
        self.perms.require_active(actor).await?;
        let channel = self
            .chats
            .find_channel(cmd.channel_id)
            .await?
            .ok_or(Error::NotFound("channel"))?;
        self.perms
            .require_can_announce_in_channel(actor, &channel)
            .await?;

        let now = OffsetDateTime::now_utc();
        let id = MessageId(Uuid::now_v7());

        let message = Message {
            id,
            channel_id: cmd.channel_id,
            sender_user_id: actor,
            body: cmd.body.clone(),
            mentions: Vec::new(),
            attachment_keys: Vec::new(),
            is_announcement: true,
            edited_at: None,
            deleted_at: None,
        };
        let announcement = Announcement {
            id,
            channel_id: cmd.channel_id,
            sender_user_id: actor,
            body: cmd.body,
            edited_at: None,
            created_at: now,
        };

        self.chats.save_message(&message).await?;
        self.chats.save_announcement(&announcement).await?;
        self.events
            .emit(DomainEvent::AnnouncementPosted {
                announcement_id: id,
                channel_id: cmd.channel_id,
                sender: actor,
                at: now,
                after: announcement.clone(),
            })
            .await?;
        Ok(announcement)
    }

    /// Edits an announcement the actor authored, within the edit grace period.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active or is not the author,
    /// `NotFound` if the announcement does not exist, `Transition` if the edit
    /// grace period has elapsed, a repository error if the datastore is
    /// unavailable, or an event error if the event bus fails.
    pub async fn edit(
        &self,
        actor: UserId,
        channel_id: ChannelId,
        announcement_id: MessageId,
        body: String,
    ) -> Result<Announcement> {
        self.perms.require_active(actor).await?;
        let mut announcement = self
            .chats
            .find_announcement(channel_id, announcement_id)
            .await?
            .ok_or(Error::NotFound("announcement"))?;
        if announcement.sender_user_id != actor {
            return Err(Error::Forbidden);
        }
        let now = OffsetDateTime::now_utc();
        announcement.edit(body, now)?;
        self.chats.save_announcement(&announcement).await?;
        self.events
            .emit(DomainEvent::AnnouncementEdited {
                announcement_id,
                channel_id,
                actor,
                at: now,
                after: announcement.clone(),
            })
            .await?;
        Ok(announcement)
    }

    /// Deletes an announcement. The author or any HR user may delete it.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active or is neither the author
    /// nor HR, `NotFound` if the announcement does not exist, a repository error
    /// if the datastore or authz backend is unavailable, or an event error if the
    /// event bus fails.
    pub async fn delete(
        &self,
        actor: UserId,
        channel_id: ChannelId,
        announcement_id: MessageId,
    ) -> Result<()> {
        self.perms.require_active(actor).await?;
        let announcement = self
            .chats
            .find_announcement(channel_id, announcement_id)
            .await?
            .ok_or(Error::NotFound("announcement"))?;
        if announcement.sender_user_id != actor && !self.perms.is_hr(actor).await? {
            return Err(Error::Forbidden);
        }
        let now = OffsetDateTime::now_utc();
        self.chats
            .delete_announcement(channel_id, announcement_id)
            .await?;
        self.events
            .emit(DomainEvent::AnnouncementDeleted {
                announcement_id,
                channel_id,
                actor,
                at: now,
            })
            .await?;
        Ok(())
    }

    /// Lists announcements in a channel the actor may view.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active or cannot view the channel,
    /// `NotFound` if the channel does not exist, or a repository error if the
    /// datastore is unavailable.
    pub async fn list_for_channel(
        &self,
        actor: UserId,
        channel_id: ChannelId,
    ) -> Result<Vec<Announcement>> {
        self.perms.require_active(actor).await?;
        let channel = self
            .chats
            .find_channel(channel_id)
            .await?
            .ok_or(Error::NotFound("channel"))?;
        self.perms.require_can_view_channel(actor, &channel).await?;
        Ok(self.chats.list_announcements(channel_id).await?)
    }
}
