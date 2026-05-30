use std::collections::HashSet;

use ::scylla::value::CqlTimeuuid;
use time::OffsetDateTime;
use uuid::Uuid;

use domain::{
    error::RepositoryError,
    ids::{ChannelId, GroupId, MessageId, UserId},
    model::{
        Announcement, Channel, ChannelKind, ChannelMembership, DirectChannel, GeneralChannel,
        GroupChannel, Message,
    },
};

const KIND_GROUP: &str = "group";
const KIND_GENERAL: &str = "general";
const KIND_DIRECT: &str = "direct";

/// Row tuple for the `channels` table.
///
/// Columns: `id`, `kind`, `name`, `group_id`, `user_a_id`, `user_b_id`, `created_at`.
pub(crate) type ChannelRow = (
    Uuid,
    String,
    Option<String>,
    Option<Uuid>,
    Option<Uuid>,
    Option<Uuid>,
    OffsetDateTime,
);

// `user_a_id` / `user_b_id` mirror the CQL column names; renaming would
// obscure the row layout.
#[allow(clippy::similar_names)]
pub(crate) fn row_to_channel(row: ChannelRow) -> Result<Channel, RepositoryError> {
    let (id, kind, name, group_id, user_a_id, user_b_id, created_at) = row;
    match kind.as_str() {
        KIND_GROUP => {
            let group_id = group_id
                .ok_or_else(|| RepositoryError::Backend("group channel missing group_id".into()))?;
            let name =
                name.ok_or_else(|| RepositoryError::Backend("group channel missing name".into()))?;
            Ok(Channel::Group(GroupChannel {
                id: ChannelId(id),
                group_id: GroupId(group_id),
                name,
                created_at,
            }))
        }
        KIND_GENERAL => Ok(Channel::General(GeneralChannel {
            id: ChannelId(id),
            created_at,
        })),
        KIND_DIRECT => {
            let a = user_a_id.ok_or_else(|| {
                RepositoryError::Backend("direct channel missing user_a_id".into())
            })?;
            let b = user_b_id.ok_or_else(|| {
                RepositoryError::Backend("direct channel missing user_b_id".into())
            })?;
            Ok(Channel::Direct(DirectChannel {
                id: ChannelId(id),
                user_low_id: UserId(a),
                user_high_id: UserId(b),
                created_at,
            }))
        }
        other => Err(RepositoryError::Backend(format!(
            "unknown channel kind: {other}"
        ))),
    }
}

/// Row tuple for the `messages_by_channel` table.
///
/// Columns: `message_id`, `sender_user_id`, `body`, `mentions`, `attachment_keys`,
/// `is_announcement`, `edited_at`, `deleted_at`. `channel_id` is part of the partition
/// key and is supplied by the caller — no need to round-trip it through the row.
pub(crate) type MessageRow = (
    CqlTimeuuid,
    Uuid,
    String,
    Option<HashSet<Uuid>>,
    Option<Vec<String>>,
    bool,
    Option<OffsetDateTime>,
    Option<OffsetDateTime>,
);

pub(crate) fn row_to_message(channel_id: ChannelId, row: MessageRow) -> Message {
    let (
        message_id,
        sender_user_id,
        body,
        mentions,
        attachment_keys,
        is_announcement,
        edited_at,
        deleted_at,
    ) = row;
    let mentions = mentions
        .unwrap_or_default()
        .into_iter()
        .map(UserId)
        .collect();
    Message {
        id: MessageId(timeuuid_to_uuid(message_id)),
        channel_id,
        sender_user_id: UserId(sender_user_id),
        body,
        mentions,
        attachment_keys: attachment_keys.unwrap_or_default(),
        is_announcement,
        edited_at,
        deleted_at,
    }
}

/// Row tuple for the `announcements_by_channel` table.
///
/// Columns: `message_id`, `sender_user_id`, `body`, `edited_at`. `created_at` is
/// derived from the embedded timestamp in the v7 `message_id` UUID since the
/// schema does not store it explicitly (TIMEUUID was meant to carry it).
pub(crate) type AnnouncementRow = (CqlTimeuuid, Uuid, String, Option<OffsetDateTime>);

pub(crate) fn row_to_announcement(
    channel_id: ChannelId,
    row: AnnouncementRow,
) -> Result<Announcement, RepositoryError> {
    let (message_id, sender_user_id, body, edited_at) = row;
    let id_uuid = timeuuid_to_uuid(message_id);
    let created_at = uuid_v7_timestamp(id_uuid)?;
    Ok(Announcement {
        id: MessageId(id_uuid),
        channel_id,
        sender_user_id: UserId(sender_user_id),
        body,
        edited_at,
        created_at,
    })
}

/// Row tuple for the `channels_by_user` table.
///
/// Columns: `channel_id`, `kind`, `last_read_at`. `user_id` is the partition key and
/// is supplied by the caller.
pub(crate) type ChannelMembershipRow = (Uuid, String, Option<OffsetDateTime>);

pub(crate) fn row_to_membership(
    user_id: UserId,
    row: ChannelMembershipRow,
) -> Result<ChannelMembership, RepositoryError> {
    let (channel_id, kind, last_read_at) = row;
    let kind = parse_channel_kind(&kind)?;
    Ok(ChannelMembership {
        user_id,
        channel_id: ChannelId(channel_id),
        kind,
        last_read_at,
    })
}

pub(crate) fn channel_kind_str(kind: ChannelKind) -> &'static str {
    match kind {
        ChannelKind::Group => KIND_GROUP,
        ChannelKind::General => KIND_GENERAL,
        ChannelKind::Direct => KIND_DIRECT,
    }
}

fn parse_channel_kind(s: &str) -> Result<ChannelKind, RepositoryError> {
    match s {
        KIND_GROUP => Ok(ChannelKind::Group),
        KIND_GENERAL => Ok(ChannelKind::General),
        KIND_DIRECT => Ok(ChannelKind::Direct),
        other => Err(RepositoryError::Backend(format!(
            "unknown channel kind: {other}"
        ))),
    }
}

pub(crate) fn timeuuid_to_uuid(t: CqlTimeuuid) -> Uuid {
    Uuid::from_bytes(*t.as_bytes())
}

pub(crate) fn uuid_to_timeuuid(u: Uuid) -> CqlTimeuuid {
    CqlTimeuuid::from(u)
}

/// Extracts the UUID v7 embedded millisecond timestamp as `OffsetDateTime`.
///
/// `MessageId`s are generated as v7 in the application layer; the `time::*_micros`
/// API works regardless of version provided the timestamp field exists.
fn uuid_v7_timestamp(id: Uuid) -> Result<OffsetDateTime, RepositoryError> {
    let ts = id
        .get_timestamp()
        .ok_or_else(|| RepositoryError::Backend("message id is not a versioned UUID".into()))?;
    let (secs, nanos) = ts.to_unix();
    let total_nanos = i128::from(secs) * 1_000_000_000 + i128::from(nanos);
    OffsetDateTime::from_unix_timestamp_nanos(total_nanos)
        .map_err(|e| RepositoryError::Backend(format!("invalid message timestamp: {e}")))
}
