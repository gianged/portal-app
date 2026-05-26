use std::{collections::HashSet, sync::Arc};

use async_trait::async_trait;
use scylla::{
    client::session::Session,
    statement::{batch::Batch, prepared::PreparedStatement},
};
use time::OffsetDateTime;
use uuid::Uuid;

use domain::{
    announcement::Announcement,
    chat::{Channel, ChannelKind, ChannelMembership, Message},
    error::RepositoryError,
    ids::{ChannelId, MessageId, UserId},
    ports::chat_repository::ChatRepository,
};

use super::mappers::{
    AnnouncementRow, ChannelMembershipRow, ChannelRow, MessageRow, channel_kind_str,
    row_to_announcement, row_to_channel, row_to_membership, row_to_message, uuid_to_timeuuid,
};

/// Prepared statements held for the lifetime of the repository. Preparing
/// once at startup avoids the per-call round-trip and gives the driver
/// a stable token to route by.
struct Statements {
    // Partition: id — single-row channel lookup.
    find_channel: PreparedStatement,
    // Partition: (user_low_id, user_high_id) — direct-channel lookup by canonical pair.
    find_direct_channel_id: PreparedStatement,
    // Partition: id — write paths split by channel kind so each `INSERT` is shape-correct.
    insert_group_channel: PreparedStatement,
    insert_general_channel: PreparedStatement,
    insert_direct_channel: PreparedStatement,
    insert_direct_channel_lookup: PreparedStatement,
    insert_channel_by_user: PreparedStatement,
    // Partition: user_id — list a user's joined channels with read marker.
    list_channels_for_user: PreparedStatement,
    update_last_read: PreparedStatement,
    // Partition: channel_id, clustering: message_id DESC — reverse-chrono.
    list_messages_latest: PreparedStatement,
    list_messages_before: PreparedStatement,
    find_message: PreparedStatement,
    save_message: PreparedStatement,
    // Partition: channel_id, clustering: message_id DESC — denormalised announcement rail.
    find_announcement: PreparedStatement,
    list_announcements: PreparedStatement,
    save_announcement: PreparedStatement,
    delete_announcement_row: PreparedStatement,
    delete_announcement_message: PreparedStatement,
}

const CHANNEL_COLS: &str = "id, kind, name, group_id, user_a_id, user_b_id, created_at";
const MESSAGE_COLS: &str =
    "message_id, sender_user_id, body, mentions, attachment_keys, is_announcement, edited_at, deleted_at";
const ANNOUNCEMENT_COLS: &str = "message_id, sender_user_id, body, edited_at";

pub struct ScyllaChatRepo {
    session: Arc<Session>,
    stmts: Statements,
}

impl ScyllaChatRepo {
    // Setup-only: a straight-line list of `prepare` calls is the clearest
    // shape for the prepared-statement table; splitting it just relocates noise.
    #[allow(clippy::too_many_lines)]
    pub async fn new(session: Arc<Session>) -> Result<Self, RepositoryError> {
        let stmts = Statements {
            find_channel: prepare(
                &session,
                format!(
                    "SELECT {CHANNEL_COLS} FROM portal_chat.channels WHERE id = ?"
                ),
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
            insert_channel_by_user: prepare(
                &session,
                "INSERT INTO portal_chat.channels_by_user \
                 (user_id, channel_id, kind, last_read_at) VALUES (?, ?, ?, ?)",
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

#[async_trait]
impl ChatRepository for ScyllaChatRepo {
    async fn find_channel(
        &self,
        id: ChannelId,
    ) -> Result<Option<Channel>, RepositoryError> {
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

    async fn save_channel(&self, channel: &Channel) -> Result<(), RepositoryError> {
        match channel {
            Channel::Group(c) => {
                self.session
                    .execute_unpaged(
                        &self.stmts.insert_group_channel,
                        (c.id.0, &c.name, c.group_id.0, c.created_at),
                    )
                    .await
                    .map_err(backend)?;
            }
            Channel::General(c) => {
                self.session
                    .execute_unpaged(&self.stmts.insert_general_channel, (c.id.0, c.created_at))
                    .await
                    .map_err(backend)?;
            }
            Channel::Direct(c) => {
                // Logged batch: the lookup table and the per-user index must
                // stay consistent with the canonical row in `channels`. Group
                // memberships for group/general channels are populated by event
                // handlers reacting to Postgres membership changes — not here.
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
            Some(cursor) => {
                let cursor_tuuid = uuid_to_timeuuid(cursor.0);
                self.session
                    .execute_unpaged(
                        &self.stmts.list_messages_before,
                        (channel_id.0, cursor_tuuid, limit),
                    )
                    .await
                    .map_err(backend)?
            }
        };
        let rows = result.into_rows_result().map_err(backend)?;
        let mut out = Vec::new();
        for row in rows.rows::<MessageRow>().map_err(backend)? {
            out.push(row_to_message(channel_id, row.map_err(backend)?));
        }
        Ok(out)
    }

    async fn find_message(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> Result<Option<Message>, RepositoryError> {
        let cursor = uuid_to_timeuuid(message_id.0);
        let result = self
            .session
            .execute_unpaged(&self.stmts.find_message, (channel_id.0, cursor))
            .await
            .map_err(backend)?;
        let rows = result.into_rows_result().map_err(backend)?;
        Ok(rows
            .maybe_first_row::<MessageRow>()
            .map_err(backend)?
            .map(|row| row_to_message(channel_id, row)))
    }

    async fn save_message(&self, message: &Message) -> Result<(), RepositoryError> {
        let mentions: HashSet<Uuid> = message.mentions.iter().map(|u| u.0).collect();
        let message_id = uuid_to_timeuuid(message.id.0);
        self.session
            .execute_unpaged(
                &self.stmts.save_message,
                (
                    message.channel_id.0,
                    message_id,
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

    async fn find_announcement(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> Result<Option<Announcement>, RepositoryError> {
        let cursor = uuid_to_timeuuid(message_id.0);
        let result = self
            .session
            .execute_unpaged(&self.stmts.find_announcement, (channel_id.0, cursor))
            .await
            .map_err(backend)?;
        let rows = result.into_rows_result().map_err(backend)?;
        match rows.maybe_first_row::<AnnouncementRow>().map_err(backend)? {
            Some(row) => Ok(Some(row_to_announcement(channel_id, row)?)),
            None => Ok(None),
        }
    }

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

    async fn save_announcement(
        &self,
        announcement: &Announcement,
    ) -> Result<(), RepositoryError> {
        let message_id = uuid_to_timeuuid(announcement.id.0);
        self.session
            .execute_unpaged(
                &self.stmts.save_announcement,
                (
                    announcement.channel_id.0,
                    message_id,
                    announcement.sender_user_id.0,
                    &announcement.body,
                    announcement.edited_at,
                ),
            )
            .await
            .map_err(backend)?;
        Ok(())
    }

    async fn delete_announcement(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> Result<(), RepositoryError> {
        // Announcements are hard-deleted from both the announcement rail and
        // the underlying message log (per the schema comment). Batch keeps
        // the two tables consistent.
        let mut batch = Batch::default();
        batch.append_statement(self.stmts.delete_announcement_row.clone());
        batch.append_statement(self.stmts.delete_announcement_message.clone());
        let id_tuuid = uuid_to_timeuuid(message_id.0);
        self.session
            .batch(
                &batch,
                (
                    (channel_id.0, id_tuuid),
                    (channel_id.0, id_tuuid),
                ),
            )
            .await
            .map_err(backend)?;
        Ok(())
    }
}

