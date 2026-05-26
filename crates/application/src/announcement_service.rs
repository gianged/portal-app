use std::sync::Arc;

use domain::{
    announcement::Announcement,
    chat::Message,
    ids::{ChannelId, MessageId, UserId},
    ports::chat_repository::ChatRepository,
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

    pub async fn post(
        &self,
        actor: UserId,
        cmd: PostAnnouncementCommand,
    ) -> Result<Announcement> {
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
