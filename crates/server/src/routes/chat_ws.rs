//! Chat WebSocket: one task per connection, fed by Redis pub/sub planes.
//!
//! - `portal.chat` carries durable `DomainEvent`s (message posted/deleted) the
//!   services emit; the task projects them to `ServerFrame`s for subscribed
//!   channels.
//! - `portal.ws` carries ephemeral `WsSignal`s (typing / presence / read-marker)
//!   the WS layer publishes itself.
//! - `portal.user` / `portal.group` carry revocation-relevant events: the task
//!   closes on the user's own deactivation and re-validates its subscriptions
//!   when a membership is revoked, so access ends without waiting for a
//!   reconnect.
//!
//! Inbound `SendMessage` goes through the write-behind `ChatIngest` buffer, which
//! validates and acks on this task then persists and fans out in batches; the
//! resulting event returns to every subscribed connection (including the sender)
//! through the `portal.chat` plane, so there is no echo special case.
//!
//! Authorization is checked per `Subscribe` through `ChatService::ensure_can_view`
//! (never cached at connect), so newly created channels work immediately and
//! revoked ones stop working immediately.

use std::collections::HashSet;
use std::pin::Pin;
use std::time::Duration;

use axum::{
    Router,
    extract::{
        State,
        ws::{Message, Utf8Bytes, WebSocket, WebSocketUpgrade},
    },
    response::Response,
    routing::get,
};
use futures::{
    SinkExt, Stream, StreamExt,
    stream::{SelectAll, SplitSink, select_all},
};

use application::{DomainEvent, commands::chat::PostMessageCommand};
use domain::{
    error::EventError,
    ids::{ChannelId, MessageId, UserId},
    model::Message as ChatMessage,
    ports::file_storage::FileStorage,
};
use shared::{
    dto::{
        chat::MessageDto,
        ws::{ClientFrame, ServerFrame},
    },
    validation::chat::{validate_message_body, validate_message_extras},
};

use crate::{
    app::AppState,
    dto,
    error::AppError,
    extractors::auth_user::AuthUser,
    realtime::{WS_TOPIC, WsSignal},
    resolve,
};

/// Presence key TTL; must exceed [`HEARTBEAT`] so a live connection never lets
/// its presence lapse between refreshes.
const PRESENCE_TTL_SECS: u64 = 60;
const HEARTBEAT: Duration = Duration::from_secs(30);
/// Lifetime of presigned attachment URLs embedded in live message frames.
const DOWNLOAD_URL_TTL: Duration = Duration::from_hours(1);

type ByteStream = Pin<Box<dyn Stream<Item = Vec<u8>> + Send>>;

pub fn router() -> Router<AppState> {
    Router::new().route("/chat/ws", get(ws))
}

async fn ws(State(state): State<AppState>, auth: AuthUser, upgrade: WebSocketUpgrade) -> Response {
    upgrade.on_upgrade(move |socket| connection(socket, state, auth.user_id))
}

async fn connection(socket: WebSocket, state: AppState, uid: UserId) {
    let (mut sink, mut stream) = socket.split();

    // Presence/typing/read-marker signals are ephemeral best-effort: a redis or cache
    // hiccup must not tear down the socket, so the `let _ =` publish/cache failures
    // throughout this task are intentionally ignored.
    let _ = state.presence.set_online(uid, PRESENCE_TTL_SECS).await;
    let _ = state
        .realtime
        .publish_signal(&WsSignal::Presence {
            user_id: uid,
            online: true,
        })
        .await;

    let mut subscribed: HashSet<ChannelId> = HashSet::new();

    let mut events = match open_streams(&state).await {
        Ok(streams) => streams,
        Err(e) => {
            tracing::error!(error = %e, "ws: failed to open redis subscriptions");
            return;
        }
    };

    let mut heartbeat = tokio::time::interval(HEARTBEAT);

    loop {
        tokio::select! {
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    if !on_client_frame(&state, uid, text.as_str(), &mut subscribed, &mut sink)
                        .await
                    {
                        break;
                    }
                }
                Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                Some(Ok(_)) => {} // ignore binary / ping / pong control frames
            },
            Some(bytes) = events.next() => {
                if !on_event(&state, uid, &bytes, &mut subscribed, &mut sink).await {
                    break;
                }
            }
            _ = heartbeat.tick() => {
                let _ = state.presence.set_online(uid, PRESENCE_TTL_SECS).await;
            }
        }
    }

    let _ = state
        .realtime
        .publish_signal(&WsSignal::Presence {
            user_id: uid,
            online: false,
        })
        .await;
}

async fn open_streams(state: &AppState) -> Result<SelectAll<ByteStream>, EventError> {
    let chat = state.realtime.subscribe("portal.chat").await?;
    let ephemeral = state.realtime.subscribe(WS_TOPIC).await?;
    // Revocation planes: deactivation closes the socket, membership changes re-validate subscriptions (see `on_domain_event`).
    let user = state.realtime.subscribe("portal.user").await?;
    let group = state.realtime.subscribe("portal.group").await?;
    Ok(select_all(vec![chat, ephemeral, user, group]))
}

/// Handles one client frame. Returns `false` when the connection should close.
#[allow(clippy::too_many_lines)]
async fn on_client_frame(
    state: &AppState,
    uid: UserId,
    text: &str,
    subscribed: &mut HashSet<ChannelId>,
    sink: &mut SplitSink<WebSocket, Message>,
) -> bool {
    let frame = match serde_json::from_str::<ClientFrame>(text) {
        Ok(frame) => frame,
        Err(e) => {
            return send(
                sink,
                &ServerFrame::Error {
                    code: "bad_frame".to_owned(),
                    message: e.to_string(),
                },
            )
            .await;
        }
    };

    match frame {
        ClientFrame::Subscribe { channel_id } => {
            let cid = ChannelId(channel_id.0);
            // Authoritative check on every subscribe; fail-closed on backend
            // errors (the client may simply retry).
            match state.chat.ensure_can_view(uid, cid).await {
                Ok(()) => {
                    subscribed.insert(cid);
                    send(sink, &ServerFrame::Subscribed { channel_id }).await
                }
                Err(application::Error::Forbidden | application::Error::NotFound(_)) => {
                    send(
                        sink,
                        &ServerFrame::Error {
                            code: "forbidden".to_owned(),
                            message: "not a member of that channel".to_owned(),
                        },
                    )
                    .await
                }
                Err(e) => {
                    tracing::warn!(error = %e, "ws: subscribe check failed");
                    send(
                        sink,
                        &ServerFrame::Error {
                            code: "subscribe_failed".to_owned(),
                            message: "could not verify channel access".to_owned(),
                        },
                    )
                    .await
                }
            }
        }
        ClientFrame::Unsubscribe { channel_id } => {
            subscribed.remove(&ChannelId(channel_id.0));
            true
        }
        ClientFrame::SendMessage {
            channel_id,
            body,
            mentions,
            attachment_keys,
        } => {
            // Same validation as the REST path; the WS plane is not a bypass.
            if let Err(e) = validate_message_body(&body)
                .and_then(|()| validate_message_extras(mentions.len(), &attachment_keys))
            {
                return send(
                    sink,
                    &ServerFrame::Error {
                        code: "bad_frame".to_owned(),
                        message: e.to_string(),
                    },
                )
                .await;
            }
            // Per-user WS rate gate before the expensive enqueue (the HTTP per-user
            // limiter never sees WS frames).
            if !crate::middleware::rate_limit::within_chat_rate(state, uid).await {
                return send(
                    sink,
                    &ServerFrame::Error {
                        code: "rate_limited".to_owned(),
                        message: "too many messages, slow down".to_owned(),
                    },
                )
                .await;
            }
            let cmd = PostMessageCommand {
                channel_id: ChannelId(channel_id.0),
                body,
                mentions: mentions.into_iter().map(|u| UserId(u.0)).collect(),
                attachment_keys,
            };
            match state.chat_ingest.enqueue(uid, cmd).await {
                Ok(_) => true, // echo arrives via the portal.chat plane
                Err(e) => {
                    send(
                        sink,
                        &ServerFrame::Error {
                            code: "send_failed".to_owned(),
                            message: e.to_string(),
                        },
                    )
                    .await
                }
            }
        }
        ClientFrame::Typing { channel_id } => {
            // Best-effort (see `connection`): ephemeral UX signal, failure is swallowed.
            let _ = state
                .realtime
                .publish_signal(&WsSignal::Typing {
                    channel_id: ChannelId(channel_id.0),
                    user_id: uid,
                })
                .await;
            true
        }
        ClientFrame::MarkRead { channel_id, up_to } => {
            let cid = ChannelId(channel_id.0);
            let _ = state.chat.mark_read(uid, cid).await;
            let _ = state
                .realtime
                .publish_signal(&WsSignal::ReadMarker {
                    channel_id: cid,
                    user_id: uid,
                    up_to: MessageId(up_to.0),
                })
                .await;
            true
        }
        ClientFrame::Ping => send(sink, &ServerFrame::Pong).await,
    }
}

/// Routes a pub/sub payload: try a durable domain event, then an ephemeral
/// signal. Returns `false` when the connection should close.
async fn on_event(
    state: &AppState,
    uid: UserId,
    bytes: &[u8],
    subscribed: &mut HashSet<ChannelId>,
    sink: &mut SplitSink<WebSocket, Message>,
) -> bool {
    if let Ok(event) = serde_json::from_slice::<DomainEvent>(bytes) {
        return on_domain_event(state, uid, &event, subscribed, sink).await;
    }
    if let Ok(signal) = serde_json::from_slice::<WsSignal>(bytes) {
        return on_signal(uid, &signal, subscribed, sink).await;
    }
    true // unknown payload (another topic's shape); ignore
}

/// Re-checks every current subscription, dropping the ones the user can no
/// longer view. Errors drop the subscription too (fail-closed); the client
/// can re-subscribe once the backend recovers.
async fn revalidate_subscriptions(
    state: &AppState,
    uid: UserId,
    subscribed: &mut HashSet<ChannelId>,
) {
    let current: Vec<ChannelId> = subscribed.iter().copied().collect();
    for cid in current {
        if state.chat.ensure_can_view(uid, cid).await.is_err() {
            subscribed.remove(&cid);
        }
    }
}

async fn on_domain_event(
    state: &AppState,
    uid: UserId,
    event: &DomainEvent,
    subscribed: &mut HashSet<ChannelId>,
    sink: &mut SplitSink<WebSocket, Message>,
) -> bool {
    match event {
        // Own deactivation closes the socket (with the token-version bump, cuts the user off immediately).
        DomainEvent::UserDeactivated { user_id, .. } if *user_id == uid => false,
        // Lost membership / deleted group: drop subscriptions that no longer pass the view check.
        DomainEvent::MembershipDeactivated { user_id, .. } if *user_id == uid => {
            revalidate_subscriptions(state, uid, subscribed).await;
            true
        }
        DomainEvent::GroupDeleted { .. } => {
            revalidate_subscriptions(state, uid, subscribed).await;
            true
        }
        DomainEvent::MessagePosted {
            channel_id, after, ..
        } => {
            if !subscribed.contains(channel_id) {
                return true;
            }
            match build_message_dto(state, uid, after).await {
                Ok(message) => send(sink, &ServerFrame::MessageCreated { message }).await,
                Err(e) => {
                    tracing::warn!(error = %e, "ws: failed to project posted message");
                    true
                }
            }
        }
        DomainEvent::MessageEdited {
            channel_id, after, ..
        } => {
            if !subscribed.contains(channel_id) {
                return true;
            }
            match build_message_dto(state, uid, after).await {
                Ok(message) => send(sink, &ServerFrame::MessageEdited { message }).await,
                Err(e) => {
                    tracing::warn!(error = %e, "ws: failed to project edited message");
                    true
                }
            }
        }
        DomainEvent::MessageDeleted {
            channel_id,
            message_id,
            ..
        } => {
            if !subscribed.contains(channel_id) {
                return true;
            }
            send(
                sink,
                &ServerFrame::MessageDeleted {
                    channel_id: dto::channel_id(*channel_id),
                    message_id: dto::message_id(*message_id),
                },
            )
            .await
        }
        // Other events (announcements, user/group/project/request/ticket) are not
        // chat frames; announcements reach users via REST + notifications.
        _ => true,
    }
}

async fn on_signal(
    uid: UserId,
    signal: &WsSignal,
    subscribed: &HashSet<ChannelId>,
    sink: &mut SplitSink<WebSocket, Message>,
) -> bool {
    match signal {
        WsSignal::Typing {
            channel_id,
            user_id,
        } => {
            if *user_id == uid || !subscribed.contains(channel_id) {
                return true;
            }
            send(
                sink,
                &ServerFrame::Typing {
                    channel_id: dto::channel_id(*channel_id),
                    user_id: dto::user_id(*user_id),
                },
            )
            .await
        }
        WsSignal::ReadMarker {
            channel_id,
            user_id,
            up_to,
        } => {
            if *user_id == uid || !subscribed.contains(channel_id) {
                return true;
            }
            send(
                sink,
                &ServerFrame::ReadMarker {
                    channel_id: dto::channel_id(*channel_id),
                    user_id: dto::user_id(*user_id),
                    up_to: dto::message_id(*up_to),
                },
            )
            .await
        }
        WsSignal::Presence { user_id, online } => {
            if *user_id == uid {
                return true;
            }
            send(
                sink,
                &ServerFrame::Presence {
                    user_id: dto::user_id(*user_id),
                    online: *online,
                },
            )
            .await
        }
    }
}

async fn build_message_dto(
    state: &AppState,
    viewer: UserId,
    message: &ChatMessage,
) -> Result<MessageDto, AppError> {
    let sender = resolve::user_summary(&state.user, &state.group, message.sender_user_id).await?;
    let mut mentions = Vec::with_capacity(message.mentions.len());
    for mention in &message.mentions {
        mentions.push(resolve::user_summary(&state.user, &state.group, *mention).await?);
    }
    // Live frames carry presigned attachment DTOs (per recipient); deleted messages render none.
    let attachments = if message.is_deleted() || message.attachment_keys.is_empty() {
        Vec::new()
    } else {
        let metadata = state
            .chat
            .attachments_by_keys(&message.attachment_keys)
            .await?;
        let mut out = Vec::with_capacity(metadata.len());
        for a in &metadata {
            let download_url = state
                .storage
                .presign_get(&a.storage_key, DOWNLOAD_URL_TTL, viewer)
                .await
                .map_err(|e| AppError::Domain(application::Error::Storage(e)))?;
            out.push(dto::chat_attachment_dto(a, download_url));
        }
        out
    };
    Ok(dto::message_dto(message, sender, mentions, attachments))
}

/// Serializes and sends a frame. Returns `false` if the socket is gone.
async fn send(sink: &mut SplitSink<WebSocket, Message>, frame: &ServerFrame) -> bool {
    match serde_json::to_string(frame) {
        Ok(json) => sink
            .send(Message::Text(Utf8Bytes::from(json)))
            .await
            .is_ok(),
        // ServerFrame is always serializable; treat an encoding error as non-fatal.
        Err(_) => true,
    }
}
