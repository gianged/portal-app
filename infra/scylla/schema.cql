-- =============================================================================
-- schema.cql
-- Cassandra / ScyllaDB schema for the Internal Portal chat subsystem.
-- Apply:   cqlsh -f infra/cassandra/schema.cql
--
-- Why Cassandra: the chat workload is append-heavy, time-ordered, and
-- partitioned naturally by channel. Cross-instance fanout to WebSocket
-- clients is handled by Redis pub/sub (see docs/tech.txt); persistence
-- and history queries live here.
--
-- Modelling notes:
--   - Each table answers one access pattern. Don't query across them.
--   - message_id is TIMEUUID (type 1 UUID) so rows are naturally
--     time-ordered without a separate timestamp clustering column.
--   - All "*_at" columns are TIMESTAMP (Cassandra has no TIMESTAMPTZ;
--     always serialize from UTC on the client side).
--   - At 100-1000 users we expect modest channel volumes; if a hot
--     channel ever pushes past ~100MB per partition, bucket the
--     partition key as ((channel_id, bucket_yyyymm)) without changing
--     the access patterns. Not done preemptively.
-- =============================================================================


-- -----------------------------------------------------------------------------
-- 1. Keyspace
-- -----------------------------------------------------------------------------
-- SimpleStrategy is fine for single-DC dev/test. Swap to
-- NetworkTopologyStrategy with a real RF for production.
CREATE KEYSPACE IF NOT EXISTS portal_chat
    WITH REPLICATION = {
        'class': 'SimpleStrategy',
        'replication_factor': 1
    }
    AND DURABLE_WRITES = true;

USE portal_chat;


-- -----------------------------------------------------------------------------
-- 2. channels
--    Authoritative metadata for every channel. One row per channel.
--    kind: 'group' | 'general' | 'direct'.
--      group   -> group_id references Postgres org.groups.id
--      general -> exactly one row exists, no group/user refs
--      direct  -> user_a_id < user_b_id always (sorted) so the pair is
--                 canonical; name/group_id are null
-- -----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS channels (
    id           UUID,
    kind         TEXT,
    name         TEXT,
    group_id     UUID,
    user_a_id    UUID,
    user_b_id    UUID,
    created_at   TIMESTAMP,
    PRIMARY KEY (id)
);


-- -----------------------------------------------------------------------------
-- 3. messages_by_channel
--    Primary message log. Hot path: "load the last N messages of channel X
--    (optionally before message_id Y for pagination)".
--
--    Regular messages: soft-deleted via deleted_at (the body stays so audit /
--    moderation can review). 15-minute self-delete grace is enforced at the
--    application layer.
--
--    Announcements: hard-deleted per the doc. Application issues DELETE
--    against this table AND announcements_by_channel.
--
--    TimeWindowCompactionStrategy fits time-series perfectly: SSTables are
--    bucketed by write time, so old messages compact independently from
--    recent ones and old buckets can age out cheaply if a retention policy
--    is ever applied.
-- -----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS messages_by_channel (
    channel_id        UUID,
    message_id        TIMEUUID,
    sender_user_id    UUID,
    body              TEXT,
    mentions          SET<UUID>,
    attachment_keys   LIST<TEXT>,
    is_announcement   BOOLEAN,
    edited_at         TIMESTAMP,
    deleted_at        TIMESTAMP,
    PRIMARY KEY ((channel_id), message_id)
) WITH CLUSTERING ORDER BY (message_id DESC)
  AND compaction = {
      'class': 'TimeWindowCompactionStrategy',
      'compaction_window_unit': 'DAYS',
      'compaction_window_size': '7'
  }
  AND comment = 'Per-channel message log. Newest-first via TIMEUUID DESC.';


-- -----------------------------------------------------------------------------
-- 4. announcements_by_channel
--    Denormalized read path for the pinned / highlighted announcement rail.
--    Written by the application transactionally with messages_by_channel when
--    is_announcement = true. Hard-deleted when the sender (or HR) deletes the
--    announcement.
--
--    Why a separate table instead of an index: Cassandra secondary indexes
--    on low-cardinality booleans scale poorly. Denormalization is the
--    idiomatic answer.
-- -----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS announcements_by_channel (
    channel_id        UUID,
    message_id        TIMEUUID,
    sender_user_id    UUID,
    body              TEXT,
    edited_at         TIMESTAMP,
    PRIMARY KEY ((channel_id), message_id)
) WITH CLUSTERING ORDER BY (message_id DESC)
  AND comment = 'Announcements view of messages_by_channel; hard-delete on takedown.';


-- -----------------------------------------------------------------------------
-- 5. channels_by_user
--    "What channels can this user see, and what is the user's read state in
--    each?" Maintained by the application when group memberships change in
--    Postgres or when a direct channel is created.
--
--    last_read_at advances when the user opens the channel; the unread badge
--    count is computed by counting messages in messages_by_channel with
--    message_id newer than dateOf(last_read_at).
-- -----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS channels_by_user (
    user_id        UUID,
    channel_id     UUID,
    kind           TEXT,
    last_read_at   TIMESTAMP,
    PRIMARY KEY ((user_id), channel_id)
);


-- -----------------------------------------------------------------------------
-- 6. direct_channel_by_users
--    Lookup: "does a direct channel between user A and user B already
--    exist?" before creating one. The key is the sorted user pair so
--    (A, B) and (B, A) collapse to one row. Pair sort happens on the
--    application side before write.
-- -----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS direct_channel_by_users (
    user_low_id    UUID,
    user_high_id   UUID,
    channel_id     UUID,
    PRIMARY KEY ((user_low_id, user_high_id))
);
