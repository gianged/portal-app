//! Chat message ingest: throughput + no-loss, two paths compared.
//!
//! Gated behind the `integration` feature AND `#[ignore]`, so normal `cargo test`
//! and CI never touch infrastructure. Run against a live dep stack:
//!
//!   cargo make bootstrap                 # postgres + scylla + redis + schema
//!   cargo test -p server --features integration -- --ignored --nocapture --test-threads=1 throughput
//!
//! Two tests fire `TOTAL_MESSAGES` posts at one channel with `CONCURRENCY`
//! in-flight, then assert every posted message is both persisted in Scylla and
//! fanned out on the Redis `portal.chat` plane. No message may be lost.
//!
//! - [`chat_throughput_and_no_loss`] drives the synchronous `ChatService::post_message`
//!   path (`require_active` -> Scylla INSERT -> Redis PUBLISH), the baseline.
//! - [`chat_ingest_throughput_and_no_loss`] drives the write-behind `ChatIngest::enqueue`
//!   path (optimistic ack -> batched Scylla write -> batched fan-out off the caller).
//!   It reports both the ack rate and the end-to-end (persist + fan-out) rate, and
//!   respects the load-shed backpressure policy by retrying on `chat_overloaded`,
//!   so the no-loss checks prove the buffer drops nothing under load.
//!
//! The general channel is a singleton that survives across runs, so each run tags
//! its bodies with a unique marker and asserts only on its own messages. The
//! notification/audit job enqueues are stubbed out (background side channels, not
//! the persistence path under test); the Scylla write and Redis publish are real.
#![cfg(feature = "integration")]
#![allow(clippy::cast_precision_loss)]

use std::{
    collections::HashSet,
    env,
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use futures::{StreamExt, stream};
use time::OffsetDateTime;
use tokio::task::JoinHandle;
use uuid::Uuid;

use application::{
    Error, bootstrap,
    commands::chat::PostMessageCommand,
    events::EventBus,
    permissions::Permissions,
    service::{ChatIngest, ChatIngestConfig, ChatService},
};
use domain::{
    error::{AuthzError, JobError, RepositoryError, StorageError},
    ids::{ChannelId, MessageId, UserId},
    model::{ChatAttachment, SystemRole, User, UserStatus},
    ports::{
        authz_client::{AuthzClient, RelationTuple},
        file_storage::{FileStorage, StorageObject},
        job_queue::JobQueue,
    },
    repository::{ChatAttachmentRepository, ChatRepository, GroupRepository, UserRepository},
};
use infrastructure::{
    postgres::{self, PgGroupRepo, PgUserRepo},
    redis::RedisEventPublisher,
    scylla::{self, ScyllaChatRepo},
};

// Tunables. Bump TOTAL_MESSAGES to push the ceiling; CONCURRENCY is the in-flight
// post depth (how many messages are mid-pipeline at once).
const TOTAL_MESSAGES: usize = 20_000;
const CONCURRENCY: usize = 256;
const READBACK_PAGE: u32 = 1_000;
// Hard caps so a lost message fails the assertion instead of hanging forever.
const FANOUT_WAIT: Duration = Duration::from_mins(2);

const CHAT_EVENT_KEY: &str = "portal:event:portal.chat";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live dep stack; run with --features integration -- --ignored"]
async fn chat_throughput_and_no_loss() {
    let h = setup().await;
    let marker = format!("loadtest:{}:", Uuid::now_v7());
    let collector = spawn_collector(h.redis_url.clone(), marker.clone()).await;

    // Fire the load, bounded to CONCURRENCY in-flight, timing the inline path.
    let start = Instant::now();
    let posted: Vec<MessageId> = stream::iter(0..TOTAL_MESSAGES)
        .map(|i| {
            let chat = h.chat.clone();
            let marker = marker.clone();
            let (channel_id, actor) = (h.channel_id, h.actor);
            async move {
                let cmd = PostMessageCommand {
                    channel_id,
                    body: format!("{marker}{i}"),
                    mentions: vec![],
                    attachment_keys: vec![],
                };
                chat.post_message(actor, cmd)
                    .await
                    .expect("post_message")
                    .id
            }
        })
        .buffer_unordered(CONCURRENCY)
        .collect()
        .await;
    let elapsed = start.elapsed();
    let rate = TOTAL_MESSAGES as f64 / elapsed.as_secs_f64();
    println!(
        "[inline] posted {TOTAL_MESSAGES} messages in {elapsed:.2?} => {rate:.0} msg/sec (concurrency {CONCURRENCY})",
    );

    let posted_ids = unique_ids(&posted);
    assert_persisted(&h.chats, h.channel_id, &posted_ids).await;
    assert_fanned_out(collector, &posted_ids).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a live dep stack; run with --features integration -- --ignored"]
async fn chat_ingest_throughput_and_no_loss() {
    let h = setup().await;
    let marker = format!("loadtest:{}:", Uuid::now_v7());
    let collector = spawn_collector(h.redis_url.clone(), marker.clone()).await;

    // Write-behind buffer in front of the same ChatService, with its drain loop.
    let (ingest, rx) = ChatIngest::new(
        h.chat.clone(),
        h.chats.clone(),
        h.events.clone(),
        None,
        ChatIngestConfig::default(),
    );
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let drain = tokio::spawn(ingest.clone().run(rx, shutdown_rx));

    // Fire the load through enqueue (optimistic ack). On a full buffer the policy
    // sheds with `chat_overloaded`; the producer retries so no accepted message is
    // lost while still respecting backpressure.
    let start = Instant::now();
    let posted: Vec<MessageId> = stream::iter(0..TOTAL_MESSAGES)
        .map(|i| {
            let ingest = ingest.clone();
            let marker = marker.clone();
            let (channel_id, actor) = (h.channel_id, h.actor);
            async move {
                let cmd = PostMessageCommand {
                    channel_id,
                    body: format!("{marker}{i}"),
                    mentions: vec![],
                    attachment_keys: vec![],
                };
                loop {
                    match ingest.enqueue(actor, cmd.clone()).await {
                        Ok(message) => break message.id,
                        Err(Error::Conflict(c)) if c.as_str() == "chat_overloaded" => {
                            tokio::time::sleep(Duration::from_millis(1)).await;
                        }
                        Err(e) => panic!("enqueue failed: {e}"),
                    }
                }
            }
        })
        .buffer_unordered(CONCURRENCY)
        .collect()
        .await;
    let ack_elapsed = start.elapsed();
    let ack_rate = TOTAL_MESSAGES as f64 / ack_elapsed.as_secs_f64();

    // Flush the tail: signal shutdown and wait for the drain to persist + fan out
    // everything still buffered before measuring end-to-end throughput.
    let _ = shutdown_tx.send(());
    drain.await.expect("drain task panicked");
    let total_elapsed = start.elapsed();
    let total_rate = TOTAL_MESSAGES as f64 / total_elapsed.as_secs_f64();
    println!(
        "[buffered] acked {TOTAL_MESSAGES} in {ack_elapsed:.2?} => {ack_rate:.0} msg/sec; persisted+fanned in {total_elapsed:.2?} => {total_rate:.0} msg/sec end-to-end (concurrency {CONCURRENCY})",
    );

    let posted_ids = unique_ids(&posted);
    assert_persisted(&h.chats, h.channel_id, &posted_ids).await;
    assert_fanned_out(collector, &posted_ids).await;
}

/// Real backends + a seeded HR actor and the general channel, shared by both
/// throughput tests.
struct Harness {
    chats: Arc<dyn ChatRepository>,
    chat: Arc<ChatService>,
    events: Arc<EventBus>,
    actor: UserId,
    channel_id: ChannelId,
    redis_url: String,
}

/// Stands up Postgres / Scylla / Redis, wires a `ChatService` over them with no-op
/// side channels, seeds an active HR user (HR may post to general), and ensures
/// the general channel exists.
async fn setup() -> Harness {
    dotenvy::dotenv().ok();
    let database_url = require_env("DATABASE_URL");
    let redis_url = require_env("REDIS_URL");
    let scylla_hosts: Vec<String> = env_or("SCYLLA_HOSTS", "127.0.0.1:9042")
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();
    let scylla_keyspace = env_or("SCYLLA_KEYSPACE", "portal_chat");

    let pool = postgres::build_pool(&database_url, 16)
        .await
        .expect("postgres pool");
    let session = scylla::build_session(&scylla_hosts, &scylla_keyspace)
        .await
        .expect("scylla session");
    let publisher = Arc::new(
        RedisEventPublisher::new(&redis_url)
            .await
            .expect("redis publisher"),
    );

    let users: Arc<dyn UserRepository> = Arc::new(PgUserRepo::new(pool.clone()));
    let groups: Arc<dyn GroupRepository> = Arc::new(PgGroupRepo::new(pool.clone()));
    let chats: Arc<dyn ChatRepository> = Arc::new(
        ScyllaChatRepo::new(session)
            .await
            .expect("scylla chat repo"),
    );

    // Post path makes zero authz-client calls (general post = require_hr, a role
    // check), so a no-op authz client is faithful here.
    let authz: Arc<dyn AuthzClient> = Arc::new(NoopAuthz);
    let perms = Arc::new(Permissions::new(users.clone(), groups.clone(), authz));

    let jobs: Arc<dyn JobQueue> = Arc::new(NoopJobs);
    let events = Arc::new(EventBus::new(publisher, jobs.clone(), jobs));

    let attachments: Arc<dyn ChatAttachmentRepository> = Arc::new(NoopChatAttachments);
    let storage: Arc<dyn FileStorage> = Arc::new(NoopStorage);
    let chat = Arc::new(ChatService::new(
        chats.clone(),
        users.clone(),
        attachments,
        storage,
        perms.clone(),
        events.clone(),
    ));

    let now = OffsetDateTime::now_utc();
    let actor = UserId(Uuid::now_v7());
    let user = User {
        id: actor,
        email: format!("loadtest-{}@portal.local", actor.0),
        password_hash: "x".into(),
        full_name: "Load Test".into(),
        avatar_storage_key: None,
        phone: None,
        timezone: "UTC".into(),
        status: UserStatus::Active,
        system_role: Some(SystemRole::Hr),
        // Active users must carry a first-login time (chk_users_status_first_login_consistency).
        first_logged_in_at: Some(now),
        deactivated_at: None,
        created_at: now,
        updated_at: now,
    };
    users.save(&user).await.expect("seed hr user");
    bootstrap::seed_company(chats.as_ref(), perms.as_ref())
        .await
        .expect("seed company / general channel");
    let channel_id = chats
        .find_general_channel()
        .await
        .expect("find general channel")
        .expect("general channel exists")
        .id();

    Harness {
        chats,
        chat,
        events,
        actor,
        channel_id,
        redis_url,
    }
}

/// Subscribes to the chat fan-out plane and returns a task collecting the ids of
/// this run's marked messages. Awaits subscriber readiness before returning, so
/// no published event posted afterwards is missed.
async fn spawn_collector(redis_url: String, marker: String) -> JoinHandle<HashSet<Uuid>> {
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        let client = redis::Client::open(redis_url).expect("redis client");
        let mut pubsub = client.get_async_pubsub().await.expect("pubsub conn");
        pubsub.subscribe(CHAT_EVENT_KEY).await.expect("subscribe");
        let _ = ready_tx.send(());

        let mut seen: HashSet<Uuid> = HashSet::new();
        let mut stream = pubsub.on_message();
        let deadline = tokio::time::Instant::now() + FANOUT_WAIT;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            match tokio::time::timeout(remaining, stream.next()).await {
                Ok(Some(msg)) => {
                    let Ok(payload) = msg.get_payload::<Vec<u8>>() else {
                        continue;
                    };
                    if let Some(id) = parse_marked_message(&payload, &marker) {
                        seen.insert(id);
                        if seen.len() >= TOTAL_MESSAGES {
                            break;
                        }
                    }
                }
                _ => break, // stream ended or timed out
            }
        }
        seen
    });
    ready_rx.await.expect("subscriber ready");
    handle
}

/// Collects the posted ids into a set, asserting none collided.
fn unique_ids(posted: &[MessageId]) -> HashSet<Uuid> {
    let ids: HashSet<Uuid> = posted.iter().map(|m| m.0).collect();
    assert_eq!(ids.len(), TOTAL_MESSAGES, "duplicate message ids generated");
    ids
}

/// No-loss in Scylla: page the channel newest-first, collecting our ids until all
/// are found or the pages run out.
async fn assert_persisted(
    chats: &Arc<dyn ChatRepository>,
    channel_id: ChannelId,
    posted_ids: &HashSet<Uuid>,
) {
    let mut found: HashSet<Uuid> = HashSet::new();
    let mut before: Option<MessageId> = None;
    let max_pages = TOTAL_MESSAGES / READBACK_PAGE as usize + 50;
    for _ in 0..max_pages {
        let batch = chats
            .list_messages(channel_id, before, READBACK_PAGE)
            .await
            .expect("list_messages");
        if batch.is_empty() {
            break;
        }
        for m in &batch {
            if posted_ids.contains(&m.id.0) {
                found.insert(m.id.0);
            }
        }
        before = batch.last().map(|m| m.id);
        if found.len() == posted_ids.len() {
            break;
        }
    }
    assert_eq!(
        found.len(),
        posted_ids.len(),
        "Scylla lost messages: {} of {} persisted",
        found.len(),
        posted_ids.len(),
    );
    println!("persisted {}/{} in Scylla", found.len(), posted_ids.len());
}

/// No-loss in fan-out: every posted id must have crossed the Redis plane.
async fn assert_fanned_out(collector: JoinHandle<HashSet<Uuid>>, posted_ids: &HashSet<Uuid>) {
    let seen = tokio::time::timeout(FANOUT_WAIT + Duration::from_secs(10), collector)
        .await
        .expect("fan-out collector hung")
        .expect("fan-out collector panicked");
    let fanned = posted_ids.intersection(&seen).count();
    assert_eq!(
        fanned,
        posted_ids.len(),
        "Redis fan-out lost messages: {} of {} delivered",
        fanned,
        posted_ids.len(),
    );
    println!("fanned out {}/{} on portal.chat", fanned, posted_ids.len());
}

/// Extracts the message id from a `message_posted` event whose body carries
/// `marker`. Parsed as untyped JSON so the test does not depend on `DomainEvent`
/// being `Deserialize`.
fn parse_marked_message(payload: &[u8], marker: &str) -> Option<Uuid> {
    let v: serde_json::Value = serde_json::from_slice(payload).ok()?;
    if v.get("type")?.as_str()? != "message_posted" {
        return None;
    }
    let body = v.get("after")?.get("body")?.as_str()?;
    if !body.starts_with(marker) {
        return None;
    }
    Uuid::parse_str(v.get("message_id")?.as_str()?).ok()
}

fn require_env(key: &str) -> String {
    env::var(key).unwrap_or_else(|_| {
        panic!("{key} must be set (source .env / run cargo make bootstrap first)")
    })
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

// --- No-op test doubles for the side channels the post path doesn't exercise ---

struct NoopAuthz;
#[async_trait]
impl AuthzClient for NoopAuthz {
    async fn check(&self, _: UserId, _: &str, _: &str) -> Result<bool, AuthzError> {
        Ok(false)
    }
    async fn write_tuple(&self, _: &str, _: &str, _: &str) -> Result<(), AuthzError> {
        Ok(())
    }
    async fn delete_tuple(&self, _: &str, _: &str, _: &str) -> Result<(), AuthzError> {
        Ok(())
    }
    async fn write_tuples(
        &self,
        _: &[RelationTuple],
        _: &[RelationTuple],
    ) -> Result<(), AuthzError> {
        Ok(())
    }
    async fn list_objects(&self, _: UserId, _: &str, _: &str) -> Result<Vec<String>, AuthzError> {
        Ok(vec![])
    }
}

struct NoopJobs;
#[async_trait]
impl JobQueue for NoopJobs {
    async fn enqueue(&self, _: &str, _: &[u8]) -> Result<(), JobError> {
        Ok(())
    }
}

struct NoopStorage;
#[async_trait]
impl FileStorage for NoopStorage {
    async fn put(&self, _: &str, _: &str, _: Vec<u8>) -> Result<(), StorageError> {
        Ok(())
    }
    async fn get(&self, _: &str) -> Result<Vec<u8>, StorageError> {
        Ok(vec![])
    }
    async fn delete(&self, _: &str) -> Result<(), StorageError> {
        Ok(())
    }
    async fn presign_get(
        &self,
        _: &str,
        _: std::time::Duration,
        _: UserId,
    ) -> Result<String, StorageError> {
        Ok(String::new())
    }
    async fn list(&self, _: &str) -> Result<Vec<StorageObject>, StorageError> {
        Ok(vec![])
    }
}

struct NoopChatAttachments;
#[async_trait]
impl ChatAttachmentRepository for NoopChatAttachments {
    async fn save(&self, _: &ChatAttachment) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn find_by_keys(&self, _: &[String]) -> Result<Vec<ChatAttachment>, RepositoryError> {
        Ok(vec![])
    }
    async fn list_all_keys(&self) -> Result<Vec<String>, RepositoryError> {
        Ok(vec![])
    }
}
