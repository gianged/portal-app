use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use async_trait::async_trait;
use futures::{StreamExt, TryStreamExt, stream};
// `::scylla` names the driver crate, not this crate's own `scylla` module.
use ::scylla::{
    client::session::Session,
    statement::{
        batch::{Batch, BatchType},
        prepared::PreparedStatement,
    },
};
use time::OffsetDateTime;
use uuid::Uuid;

use domain::{
    error::RepositoryError,
    ids::{ChannelId, GroupId, MessageId, UserId},
    model::{Announcement, Channel, ChannelKind, ChannelMembership, Message},
    repository::ChatRepository,
};

use crate::scylla::mappers::{
    AnnouncementRow, ChannelMembershipRow, ChannelRow, MessageRow, channel_kind_str,
    row_to_announcement, row_to_channel, row_to_membership, row_to_message,
};

/// Prepared statements held for the repository's lifetime; prepared once at startup to avoid per-call round-trips.
struct Statements {
    // Partition: id - single-row channel lookup.
    find_channel: PreparedStatement,
    // Partition: (user_low_id, user_high_id) - direct-channel lookup by canonical pair.
    find_direct_channel_id: PreparedStatement,
    // Partition: id - write paths split by channel kind so each `INSERT` is shape-correct.
    insert_group_channel: PreparedStatement,
    insert_general_channel: PreparedStatement,
    insert_direct_channel: PreparedStatement,
    insert_direct_channel_lookup: PreparedStatement,
    // Partition: group_id - group->channel lookup (1:1) for member subscription.
    insert_group_channel_lookup: PreparedStatement,
    find_group_channel_id: PreparedStatement,
    // Partition: scope - org-wide singleton channel lookup ('general').
    insert_singleton_channel: PreparedStatement,
    find_general_channel_id: PreparedStatement,
    insert_channel_by_user: PreparedStatement,
    delete_channel_by_user: PreparedStatement,
    // Partition: user_id - list a user's joined channels with read marker.
    list_channels_for_user: PreparedStatement,
    update_last_read: PreparedStatement,
    // Partition: channel_id, clustering: message_id DESC - reverse-chrono.
    list_messages_latest: PreparedStatement,
    list_messages_before: PreparedStatement,
    find_message: PreparedStatement,
    save_message: PreparedStatement,
    // Partition: channel_id, clustering: message_id DESC - denormalised announcement rail.
    find_announcement: PreparedStatement,
    list_announcements: PreparedStatement,
    save_announcement: PreparedStatement,
    delete_announcement_row: PreparedStatement,
    delete_announcement_message: PreparedStatement,
}

const CHANNEL_COLS: &str = "id, kind, name, group_id, user_a_id, user_b_id, created_at";
const MESSAGE_COLS: &str = "message_id, sender_user_id, body, mentions, attachment_keys, is_announcement, edited_at, deleted_at";
const ANNOUNCEMENT_COLS: &str = "message_id, sender_user_id, body, edited_at";

/// Scylla-backed implementation of `ChatRepository`.
pub struct ScyllaChatRepo {
    session: Arc<Session>,
    stmts: Statements,
}

impl ScyllaChatRepo {
    #[allow(clippy::too_many_lines)]
    pub async fn new(session: Arc<Session>) -> Result<Self, RepositoryError> {
        let stmts = Statements {
            find_channel: prepare(
                &session,
                format!("SELECT {CHANNEL_COLS} FROM portal_chat.channels WHERE id = ?"),
            )
            .await?,
            find_direct_channel_id: prepare(
                &session,
                "SELECT channel_id FROM portal_chat.direct_channel_by_users \
                 WHERE user_low_id = ? AND user_high_id = ?",
            )
            .await?,
            insert_group_channel: prepare(
                &session,
                "INSERT INTO portal_chat.channels \
                 (id, kind, name, group_id, created_at) VALUES (?, 'group', ?, ?, ?)",
            )
            .await?,
            insert_general_channel: prepare(
                &session,
                "INSERT INTO portal_chat.channels (id, kind, created_at) \
                 VALUES (?, 'general', ?)",
            )
            .await?,
            insert_direct_channel: prepare(
                &session,
                "INSERT INTO portal_chat.channels \
                 (id, kind, user_a_id, user_b_id, created_at) VALUES (?, 'direct', ?, ?, ?)",
            )
            .await?,
            insert_direct_channel_lookup: prepare(
                &session,
                "INSERT INTO portal_chat.direct_channel_by_users \
                 (user_low_id, user_high_id, channel_id) VALUES (?, ?, ?)",
            )
            .await?,
            insert_group_channel_lookup: prepare(
                &session,
                "INSERT INTO portal_chat.group_channel_by_group \
                 (group_id, channel_id) VALUES (?, ?)",
            )
            .await?,
            find_group_channel_id: prepare(
                &session,
                "SELECT channel_id FROM portal_chat.group_channel_by_group WHERE group_id = ?",
            )
            .await?,
            insert_singleton_channel: prepare(
                &session,
                "INSERT INTO portal_chat.singleton_channel (scope, channel_id) VALUES (?, ?)",
            )
            .await?,
            find_general_channel_id: prepare(
                &session,
                "SELECT channel_id FROM portal_chat.singleton_channel WHERE scope = 'general'",
            )
            .await?,
            insert_channel_by_user: prepare(
                &session,
                "INSERT INTO portal_chat.channels_by_user \
                 (user_id, channel_id, kind, last_read_at) VALUES (?, ?, ?, ?)",
            )
            .await?,
            delete_channel_by_user: prepare(
                &session,
                "DELETE FROM portal_chat.channels_by_user WHERE user_id = ? AND channel_id = ?",
            )
            .await?,
            list_channels_for_user: prepare(
                &session,
                "SELECT channel_id, kind, last_read_at FROM portal_chat.channels_by_user \
                 WHERE user_id = ?",
            )
            .await?,
            update_last_read: prepare(
                &session,
                "UPDATE portal_chat.channels_by_user SET last_read_at = ? \
                 WHERE user_id = ? AND channel_id = ?",
            )
            .await?,
            list_messages_latest: prepare(
                &session,
                format!(
                    "SELECT {MESSAGE_COLS} FROM portal_chat.messages_by_channel \
                     WHERE channel_id = ? LIMIT ?"
                ),
            )
            .await?,
            list_messages_before: prepare(
                &session,
                format!(
                    "SELECT {MESSAGE_COLS} FROM portal_chat.messages_by_channel \
                     WHERE channel_id = ? AND message_id < ? LIMIT ?"
                ),
            )
            .await?,
            find_message: prepare(
                &session,
                format!(
                    "SELECT {MESSAGE_COLS} FROM portal_chat.messages_by_channel \
                     WHERE channel_id = ? AND message_id = ?"
                ),
            )
            .await?,
            save_message: prepare(
                &session,
                "INSERT INTO portal_chat.messages_by_channel \
                 (channel_id, message_id, sender_user_id, body, mentions, attachment_keys, \
                  is_announcement, edited_at, deleted_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .await?,
            find_announcement: prepare(
                &session,
                format!(
                    "SELECT {ANNOUNCEMENT_COLS} FROM portal_chat.announcements_by_channel \
                     WHERE channel_id = ? AND message_id = ?"
                ),
            )
            .await?,
            list_announcements: prepare(
                &session,
                format!(
                    "SELECT {ANNOUNCEMENT_COLS} FROM portal_chat.announcements_by_channel \
                     WHERE channel_id = ?"
                ),
            )
            .await?,
            save_announcement: prepare(
                &session,
                "INSERT INTO portal_chat.announcements_by_channel \
                 (channel_id, message_id, sender_user_id, body, edited_at) \
                 VALUES (?, ?, ?, ?, ?)",
            )
            .await?,
            delete_announcement_row: prepare(
                &session,
                "DELETE FROM portal_chat.announcements_by_channel \
                 WHERE channel_id = ? AND message_id = ?",
            )
            .await?,
            delete_announcement_message: prepare(
                &session,
                "DELETE FROM portal_chat.messages_by_channel \
                 WHERE channel_id = ? AND message_id = ?",
            )
            .await?,
        };
        Ok(Self { session, stmts })
    }
}

async fn prepare(
    session: &Session,
    query: impl Into<String>,
) -> Result<PreparedStatement, RepositoryError> {
    session
        .prepare(query.into())
        .await
        .map_err(|e| RepositoryError::Backend(e.to_string()))
}

fn backend<E: std::fmt::Display>(e: E) -> RepositoryError {
    RepositoryError::Backend(e.to_string())
}

// Single-partition UNLOGGED batch insert for one channel's chunk; a free fn so the future has a concrete type the stream combinators accept.
async fn write_message_batch(
    session: &Session,
    stmt: &PreparedStatement,
    chunk: Vec<&Message>,
) -> Result<(), RepositoryError> {
    // Unlogged: a single-partition batch is already atomic, so a logged batch's batchlog round-trip would be pure write-amplification.
    let mut batch = Batch::new(BatchType::Unlogged);
    let mut values = Vec::with_capacity(chunk.len());
    for message in &chunk {
        batch.append_statement(stmt.clone());
        let mentions: HashSet<Uuid> = message.mentions.iter().map(|u| u.0).collect();
        values.push((
            message.channel_id.0,
            message.id.0,
            message.sender_user_id.0,
            message.body.clone(),
            mentions,
            message.attachment_keys.clone(),
            message.is_announcement,
            message.edited_at,
            message.deleted_at,
        ));
    }
    session
        .batch(&batch, values.as_slice())
        .await
        .map_err(backend)?;
    Ok(())
}

#[async_trait]
impl ChatRepository for ScyllaChatRepo {
    #[tracing::instrument(skip_all, fields(id = ?id))]
    async fn find_channel(&self, id: ChannelId) -> Result<Option<Channel>, RepositoryError> {
        let result = self
            .session
            .execute_unpaged(&self.stmts.find_channel, (id.0,))
            .await
            .map_err(backend)?;
        let rows = result.into_rows_result().map_err(backend)?;
        match rows.maybe_first_row::<ChannelRow>().map_err(backend)? {
            Some(row) => Ok(Some(row_to_channel(row)?)),
            None => Ok(None),
        }
    }

    #[tracing::instrument(skip_all, fields(a = ?a, b = ?b))]
    async fn find_direct_channel(
        &self,
        a: UserId,
        b: UserId,
    ) -> Result<Option<Channel>, RepositoryError> {
        let (low, high) = if a.0 <= b.0 { (a.0, b.0) } else { (b.0, a.0) };
        let result = self
            .session
            .execute_unpaged(&self.stmts.find_direct_channel_id, (low, high))
            .await
            .map_err(backend)?;
        let rows = result.into_rows_result().map_err(backend)?;
        let Some((channel_id,)) = rows.maybe_first_row::<(Uuid,)>().map_err(backend)? else {
            return Ok(None);
        };
        self.find_channel(ChannelId(channel_id)).await
    }

    #[tracing::instrument(skip_all)]
    async fn save_channel(&self, channel: &Channel) -> Result<(), RepositoryError> {
        match channel {
            Channel::Group(c) => {
                // Keep the canonical row and the group->channel lookup consistent.
                let mut batch = Batch::default();
                batch.append_statement(self.stmts.insert_group_channel.clone());
                batch.append_statement(self.stmts.insert_group_channel_lookup.clone());
                self.session
                    .batch(
                        &batch,
                        (
                            (c.id.0, &c.name, c.group_id.0, c.created_at),
                            (c.group_id.0, c.id.0),
                        ),
                    )
                    .await
                    .map_err(backend)?;
            }
            Channel::General(c) => {
                // Register the singleton so find_general_channel can resolve it.
                let mut batch = Batch::default();
                batch.append_statement(self.stmts.insert_general_channel.clone());
                batch.append_statement(self.stmts.insert_singleton_channel.clone());
                self.session
                    .batch(&batch, ((c.id.0, c.created_at), ("general", c.id.0)))
                    .await
                    .map_err(backend)?;
            }
            Channel::Direct(c) => {
                // Logged batch keeps the lookup table and per-user index consistent with the canonical row; direct channels seed both participants here.
                let mut batch = Batch::default();
                batch.append_statement(self.stmts.insert_direct_channel.clone());
                batch.append_statement(self.stmts.insert_direct_channel_lookup.clone());
                batch.append_statement(self.stmts.insert_channel_by_user.clone());
                batch.append_statement(self.stmts.insert_channel_by_user.clone());
                let kind = channel_kind_str(ChannelKind::Direct);
                self.session
                    .batch(
                        &batch,
                        (
                            (c.id.0, c.user_low_id.0, c.user_high_id.0, c.created_at),
                            (c.user_low_id.0, c.user_high_id.0, c.id.0),
                            (c.user_low_id.0, c.id.0, kind, None::<OffsetDateTime>),
                            (c.user_high_id.0, c.id.0, kind, None::<OffsetDateTime>),
                        ),
                    )
                    .await
                    .map_err(backend)?;
            }
        }
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(group_id = ?group_id))]
    async fn find_group_channel(
        &self,
        group_id: GroupId,
    ) -> Result<Option<Channel>, RepositoryError> {
        let result = self
            .session
            .execute_unpaged(&self.stmts.find_group_channel_id, (group_id.0,))
            .await
            .map_err(backend)?;
        let rows = result.into_rows_result().map_err(backend)?;
        let Some((channel_id,)) = rows.maybe_first_row::<(Uuid,)>().map_err(backend)? else {
            return Ok(None);
        };
        self.find_channel(ChannelId(channel_id)).await
    }

    #[tracing::instrument(skip_all)]
    async fn find_general_channel(&self) -> Result<Option<Channel>, RepositoryError> {
        let result = self
            .session
            .execute_unpaged(&self.stmts.find_general_channel_id, ())
            .await
            .map_err(backend)?;
        let rows = result.into_rows_result().map_err(backend)?;
        let Some((channel_id,)) = rows.maybe_first_row::<(Uuid,)>().map_err(backend)? else {
            return Ok(None);
        };
        self.find_channel(ChannelId(channel_id)).await
    }

    #[tracing::instrument(skip_all, fields(user_id = ?user_id, channel_id = ?channel_id))]
    async fn subscribe_member(
        &self,
        user_id: UserId,
        channel_id: ChannelId,
        kind: ChannelKind,
    ) -> Result<(), RepositoryError> {
        self.session
            .execute_unpaged(
                &self.stmts.insert_channel_by_user,
                (
                    user_id.0,
                    channel_id.0,
                    channel_kind_str(kind),
                    None::<OffsetDateTime>,
                ),
            )
            .await
            .map_err(backend)?;
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(user_id = ?user_id, channel_id = ?channel_id))]
    async fn unsubscribe_member(
        &self,
        user_id: UserId,
        channel_id: ChannelId,
    ) -> Result<(), RepositoryError> {
        self.session
            .execute_unpaged(
                &self.stmts.delete_channel_by_user,
                (user_id.0, channel_id.0),
            )
            .await
            .map_err(backend)?;
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(user_id = ?user_id))]
    async fn list_channels_for_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<ChannelMembership>, RepositoryError> {
        let result = self
            .session
            .execute_unpaged(&self.stmts.list_channels_for_user, (user_id.0,))
            .await
            .map_err(backend)?;
        let rows = result.into_rows_result().map_err(backend)?;
        let mut out = Vec::new();
        for row in rows.rows::<ChannelMembershipRow>().map_err(backend)? {
            out.push(row_to_membership(user_id, row.map_err(backend)?)?);
        }
        Ok(out)
    }

    #[tracing::instrument(skip_all, fields(user_id = ?user_id, channel_id = ?channel_id))]
    async fn update_last_read(
        &self,
        user_id: UserId,
        channel_id: ChannelId,
        at: OffsetDateTime,
    ) -> Result<(), RepositoryError> {
        self.session
            .execute_unpaged(&self.stmts.update_last_read, (at, user_id.0, channel_id.0))
            .await
            .map_err(backend)?;
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(channel_id = ?channel_id, limit = ?limit))]
    async fn list_messages(
        &self,
        channel_id: ChannelId,
        before: Option<MessageId>,
        limit: u32,
    ) -> Result<Vec<Message>, RepositoryError> {
        let limit = i32::try_from(limit).unwrap_or(i32::MAX);
        let result = match before {
            None => self
                .session
                .execute_unpaged(&self.stmts.list_messages_latest, (channel_id.0, limit))
                .await
                .map_err(backend)?,
            Some(cursor) => self
                .session
                .execute_unpaged(
                    &self.stmts.list_messages_before,
                    (channel_id.0, cursor.0, limit),
                )
                .await
                .map_err(backend)?,
        };
        let rows = result.into_rows_result().map_err(backend)?;
        let mut out = Vec::new();
        for row in rows.rows::<MessageRow>().map_err(backend)? {
            out.push(row_to_message(channel_id, row.map_err(backend)?));
        }
        Ok(out)
    }

    #[tracing::instrument(skip_all, fields(channel_id = ?channel_id, message_id = ?message_id))]
    async fn find_message(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> Result<Option<Message>, RepositoryError> {
        let result = self
            .session
            .execute_unpaged(&self.stmts.find_message, (channel_id.0, message_id.0))
            .await
            .map_err(backend)?;
        let rows = result.into_rows_result().map_err(backend)?;
        Ok(rows
            .maybe_first_row::<MessageRow>()
            .map_err(backend)?
            .map(|row| row_to_message(channel_id, row)))
    }

    #[tracing::instrument(skip_all)]
    async fn save_message(&self, message: &Message) -> Result<(), RepositoryError> {
        let mentions: HashSet<Uuid> = message.mentions.iter().map(|u| u.0).collect();
        self.session
            .execute_unpaged(
                &self.stmts.save_message,
                (
                    message.channel_id.0,
                    message.id.0,
                    message.sender_user_id.0,
                    &message.body,
                    mentions,
                    &message.attachment_keys,
                    message.is_announcement,
                    message.edited_at,
                    message.deleted_at,
                ),
            )
            .await
            .map_err(backend)?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn save_messages(&self, messages: &[Message]) -> Result<(), RepositoryError> {
        // Scylla batches are only efficient within one partition, so group by channel_id then chunk under the per-batch statement limit.
        const MAX_BATCH_STATEMENTS: usize = 100;
        const MAX_CONCURRENT_BATCHES: usize = 16;

        if messages.is_empty() {
            return Ok(());
        }

        let mut by_channel: HashMap<ChannelId, Vec<&Message>> = HashMap::new();
        for message in messages {
            by_channel
                .entry(message.channel_id)
                .or_default()
                .push(message);
        }

        let mut batches = Vec::new();
        for group in by_channel.into_values() {
            for chunk in group.chunks(MAX_BATCH_STATEMENTS) {
                batches.push(write_message_batch(
                    &self.session,
                    &self.stmts.save_message,
                    chunk.to_vec(),
                ));
            }
        }

        stream::iter(batches)
            .buffer_unordered(MAX_CONCURRENT_BATCHES)
            .try_for_each(|()| async { Ok(()) })
            .await
    }

    #[tracing::instrument(skip_all, fields(channel_id = ?channel_id, message_id = ?message_id))]
    async fn find_announcement(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> Result<Option<Announcement>, RepositoryError> {
        let result = self
            .session
            .execute_unpaged(&self.stmts.find_announcement, (channel_id.0, message_id.0))
            .await
            .map_err(backend)?;
        let rows = result.into_rows_result().map_err(backend)?;
        match rows.maybe_first_row::<AnnouncementRow>().map_err(backend)? {
            Some(row) => Ok(Some(row_to_announcement(channel_id, row)?)),
            None => Ok(None),
        }
    }

    #[tracing::instrument(skip_all, fields(channel_id = ?channel_id))]
    async fn list_announcements(
        &self,
        channel_id: ChannelId,
    ) -> Result<Vec<Announcement>, RepositoryError> {
        let result = self
            .session
            .execute_unpaged(&self.stmts.list_announcements, (channel_id.0,))
            .await
            .map_err(backend)?;
        let rows = result.into_rows_result().map_err(backend)?;
        let mut out = Vec::new();
        for row in rows.rows::<AnnouncementRow>().map_err(backend)? {
            out.push(row_to_announcement(channel_id, row.map_err(backend)?)?);
        }
        Ok(out)
    }

    #[tracing::instrument(skip_all)]
    async fn save_announcement(&self, announcement: &Announcement) -> Result<(), RepositoryError> {
        self.session
            .execute_unpaged(
                &self.stmts.save_announcement,
                (
                    announcement.channel_id.0,
                    announcement.id.0,
                    announcement.sender_user_id.0,
                    &announcement.body,
                    announcement.edited_at,
                ),
            )
            .await
            .map_err(backend)?;
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(channel_id = ?channel_id, message_id = ?message_id))]
    async fn delete_announcement(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> Result<(), RepositoryError> {
        // Hard-delete from both the announcement rail and the underlying message log; batch keeps the two tables consistent.
        let mut batch = Batch::default();
        batch.append_statement(self.stmts.delete_announcement_row.clone());
        batch.append_statement(self.stmts.delete_announcement_message.clone());
        let id = message_id.0;
        self.session
            .batch(&batch, ((channel_id.0, id), (channel_id.0, id)))
            .await
            .map_err(backend)?;
        Ok(())
    }
}
