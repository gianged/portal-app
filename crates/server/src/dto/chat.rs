//! Domain <-> wire projections for chat channels, messages, and announcements.

use application::commands::chat::{PostAnnouncementCommand, PostMessageCommand};
use domain::{ids, model};
use shared::dto::{
    announcement::{AnnouncementDto, PostAnnouncementRequest},
    chat::{
        ChannelDto, ChannelKind as WireChannelKind, ChannelSummaryDto, ChatAttachmentDto,
        MessageDto, SendMessageRequest,
    },
    common::UserSummaryDto,
};
use time::OffsetDateTime;

use super::{channel_id, chat_attachment_id, group_id, message_id, unknown_user_summary};

#[must_use]
pub fn channel_kind_dto(kind: model::ChannelKind) -> WireChannelKind {
    match kind {
        model::ChannelKind::Group => WireChannelKind::Group,
        model::ChannelKind::General => WireChannelKind::General,
        model::ChannelKind::Direct => WireChannelKind::Direct,
    }
}

/// `other_user` is required for a direct channel (the participant who is not the
/// viewer, resolved by the caller); ignored for group/general channels.
#[must_use]
pub fn channel_dto(channel: &model::Channel, other_user: Option<UserSummaryDto>) -> ChannelDto {
    match channel {
        model::Channel::Group(c) => ChannelDto::Group {
            id: channel_id(c.id),
            group_id: group_id(c.group_id),
            name: c.name.clone(),
        },
        model::Channel::General(c) => ChannelDto::General {
            id: channel_id(c.id),
        },
        model::Channel::Direct(c) => ChannelDto::Direct {
            id: channel_id(c.id),
            other_user: other_user.unwrap_or_else(|| unknown_user_summary(c.user_high_id)),
        },
    }
}

#[must_use]
pub fn channel_summary_dto(
    membership: &model::ChannelMembership,
    title: String,
    unread: bool,
    last_message_at: Option<OffsetDateTime>,
) -> ChannelSummaryDto {
    ChannelSummaryDto {
        id: channel_id(membership.channel_id),
        kind: channel_kind_dto(membership.kind),
        title,
        unread,
        last_message_at,
    }
}

/// Recovers a message's creation time from its time-ordered (v7) id, mirroring
/// how the application layer derives it (the `Message` row stores no timestamp).
///
/// # Panics
///
/// Panics if `id` is not a UUIDv7 (no embedded timestamp); message ids always are.
#[must_use]
pub fn message_created_at(id: ids::MessageId) -> OffsetDateTime {
    let ts =
        id.0.get_timestamp()
            .expect("message ids are UUIDv7 and always carry a timestamp");
    let (secs, nanos) = ts.to_unix();
    let total = i128::from(secs) * 1_000_000_000 + i128::from(nanos);
    OffsetDateTime::from_unix_timestamp_nanos(total)
        .expect("a UUIDv7 timestamp is within OffsetDateTime's range")
}

/// `download_url` is the per-viewer presigned link the caller minted.
#[must_use]
pub fn chat_attachment_dto(a: &model::ChatAttachment, download_url: String) -> ChatAttachmentDto {
    ChatAttachmentDto {
        id: chat_attachment_id(a.id),
        storage_key: a.storage_key.clone(),
        filename: a.filename.clone(),
        content_type: a.content_type.clone(),
        size_bytes: a.size_bytes,
        download_url,
    }
}

/// `attachments` are the resolved + presigned DTOs for the message's keys
/// (empty for deleted messages; the caller decides).
#[must_use]
pub fn message_dto(
    message: &model::Message,
    sender: UserSummaryDto,
    mentions: Vec<UserSummaryDto>,
    attachments: Vec<ChatAttachmentDto>,
) -> MessageDto {
    MessageDto {
        id: message_id(message.id),
        channel_id: channel_id(message.channel_id),
        sender,
        body: message.body.clone(),
        mentions,
        attachments,
        is_announcement: message.is_announcement,
        edited_at: message.edited_at,
        deleted_at: message.deleted_at,
        created_at: message_created_at(message.id),
    }
}

#[must_use]
pub fn announcement_dto(
    announcement: &model::Announcement,
    sender: UserSummaryDto,
    now: OffsetDateTime,
) -> AnnouncementDto {
    AnnouncementDto {
        id: message_id(announcement.id),
        channel_id: channel_id(announcement.channel_id),
        sender,
        body: announcement.body.clone(),
        edited_at: announcement.edited_at,
        created_at: announcement.created_at,
        editable: announcement.within_edit_grace(now),
    }
}

#[must_use]
pub fn post_message_command(
    channel: ids::ChannelId,
    req: SendMessageRequest,
) -> PostMessageCommand {
    PostMessageCommand {
        channel_id: channel,
        body: req.body,
        mentions: req.mentions.into_iter().map(|u| ids::UserId(u.0)).collect(),
        attachment_keys: req.attachment_keys,
    }
}

#[must_use]
pub fn post_announcement_command(req: PostAnnouncementRequest) -> PostAnnouncementCommand {
    PostAnnouncementCommand {
        channel_id: ids::ChannelId(req.channel_id.0),
        body: req.body,
    }
}
