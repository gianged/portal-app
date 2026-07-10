//! The single chat WebSocket client, built once at the app root and shared via
//! context. A background task owns the (non-`Send`) socket on the JS event loop
//! via `spawn_local`, reconnecting with backoff, replaying channel subscriptions
//! after each reconnect, and sending periodic `Ping`s to keep presence alive.
//! Outbound frames flow through an unbounded channel so the `WsClient` handle
//! stays cheap and `Copy`; incoming frames fan out to registered handlers.

use std::rc::Rc;

use futures::{
    FutureExt, SinkExt, StreamExt,
    channel::mpsc::{self, UnboundedReceiver, UnboundedSender},
};
use gloo::{
    net::websocket::{Message, futures::WebSocket},
    timers::future::TimeoutFuture,
};
use leptos::{prelude::*, task};

use shared::dto::ids::ChannelId;
use shared::dto::ws::{ClientFrame, ServerFrame};

const HEARTBEAT_MS: u32 = 25_000;
const BACKOFF_MIN_MS: u32 = 500;
const BACKOFF_MAX_MS: u32 = 10_000;

type FrameHandler = Rc<dyn Fn(&ServerFrame)>;

#[derive(Clone, Copy)]
pub struct WsClient {
    tx: StoredValue<UnboundedSender<ClientFrame>>,
    /// Holds the receiver until [`WsClient::start`] hands it to the read task.
    rx: StoredValue<Option<UnboundedReceiver<ClientFrame>>>,
    started: RwSignal<bool>,
    /// Bumped by [`WsClient::stop`]; a run task exits once its generation is stale.
    generation: StoredValue<u64>,
    /// Live channel subscriptions, replayed after every (re)connect because the
    /// server tracks them per connection.
    subscriptions: StoredValue<Vec<ChannelId>>,
    /// Frame subscribers, invoked in registration order for every server frame.
    handlers: StoredValue<Vec<(u64, FrameHandler)>, LocalStorage>,
    next_handler_id: StoredValue<u64>,
    pub connected: RwSignal<bool>,
}

impl Default for WsClient {
    fn default() -> Self {
        Self::new()
    }
}

impl WsClient {
    /// Create the handle without opening a socket. Provided at the app root;
    /// [`start`](Self::start) is deferred until the session is authenticated so a
    /// logged-out client never reconnect-loops against a 401 upgrade.
    #[must_use]
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded::<ClientFrame>();
        Self {
            tx: StoredValue::new(tx),
            rx: StoredValue::new(Some(rx)),
            started: RwSignal::new(false),
            generation: StoredValue::new(0),
            subscriptions: StoredValue::new(Vec::new()),
            handlers: StoredValue::new_local(Vec::new()),
            next_handler_id: StoredValue::new(0),
            connected: RwSignal::new(false),
        }
    }

    /// Open the socket and spawn the read/reconnect task. Idempotent: safe to
    /// call from an effect that fires whenever auth resolves.
    pub fn start(&self) {
        if self.started.get_untracked() {
            return;
        }
        let Some(rx) = self.rx.try_update_value(Option::take).flatten() else {
            return;
        };
        self.started.set(true);
        task::spawn_local(run(*self, rx, self.generation.get_value()));
    }

    /// Close the socket and halt the reconnect loop; a later [`start`](Self::start)
    /// (next login) begins fresh. Frame handlers stay registered.
    pub fn stop(&self) {
        if !self.started.get_untracked() {
            return;
        }
        // Bumping the generation retires the run task; dropping the old sender
        // wakes it immediately so the socket closes.
        self.generation.update_value(|g| *g += 1);
        let (tx, rx) = mpsc::unbounded::<ClientFrame>();
        self.tx.set_value(tx);
        self.rx.set_value(Some(rx));
        self.subscriptions.update_value(Vec::clear);
        self.connected.set(false);
        self.started.set(false);
    }

    /// Queue a frame for delivery. Frames sent while reconnecting are buffered and
    /// flushed once the socket is back. Errors (channel closed) are swallowed since
    /// chat actions have a REST fallback.
    pub fn send(&self, frame: ClientFrame) {
        match &frame {
            ClientFrame::Subscribe { channel_id } => {
                let cid = *channel_id;
                self.subscriptions.update_value(|subs| {
                    if !subs.contains(&cid) {
                        subs.push(cid);
                    }
                });
            }
            ClientFrame::Unsubscribe { channel_id } => {
                let cid = *channel_id;
                self.subscriptions
                    .update_value(|subs| subs.retain(|c| *c != cid));
            }
            _ => {}
        }
        self.tx.with_value(|tx| {
            let _ = tx.unbounded_send(frame);
        });
    }

    /// Register `handler` for every incoming server frame; returns an id for
    /// [`off_frame`](Self::off_frame). Handlers survive [`stop`](Self::stop).
    #[must_use]
    pub fn on_frame(&self, handler: impl Fn(&ServerFrame) + 'static) -> u64 {
        let id = self.next_handler_id.get_value();
        self.next_handler_id.set_value(id + 1);
        self.handlers
            .update_value(|hs| hs.push((id, Rc::new(handler))));
        id
    }

    /// Remove a handler registered with [`on_frame`](Self::on_frame).
    pub fn off_frame(&self, id: u64) {
        self.handlers
            .update_value(|hs| hs.retain(|(hid, _)| *hid != id));
    }

    // Snapshot the registry so a handler may (un)register during dispatch.
    fn dispatch(&self, frame: &ServerFrame) {
        for (_, handler) in self.handlers.get_value() {
            handler(frame);
        }
    }
}

/// The websocket URL on the current origin (`/api/chat/ws`, which the Trunk proxy
/// and the server's `/api/v1` mount resolve to `/api/v1/chat/ws`).
fn ws_url() -> String {
    let Some(window) = web_sys::window() else {
        return "ws://127.0.0.1:8090/api/v1/chat/ws".to_owned();
    };
    let loc = window.location();
    let scheme = match loc.protocol().as_deref() {
        Ok("https:") => "wss",
        _ => "ws",
    };
    let host = loc.host().unwrap_or_default();
    format!("{scheme}://{host}/api/chat/ws")
}

/// Owns the socket for one [`WsClient::start`] generation: connect, replay
/// subscriptions, pump frames both ways with a heartbeat, and reconnect with
/// exponential backoff on disconnect. Exits when its generation goes stale
/// ([`WsClient::stop`]) or every sender is dropped.
async fn run(client: WsClient, mut rx: UnboundedReceiver<ClientFrame>, my_gen: u64) {
    let mut backoff = BACKOFF_MIN_MS;
    loop {
        if client.generation.get_value() != my_gen {
            return;
        }
        match WebSocket::open(&ws_url()) {
            Ok(ws) => {
                backoff = BACKOFF_MIN_MS;
                client.connected.set(true);
                let (mut write, read) = ws.split();
                // `select!` needs fused futures; the split read half isn't a
                // `FusedStream`, so fuse it (the receiver already is).
                let mut read = read.fuse();
                // Server-side subscriptions die with the old connection; replay.
                for channel_id in client.subscriptions.get_value() {
                    if let Ok(json) = serde_json::to_string(&ClientFrame::Subscribe { channel_id })
                        && write.send(Message::Text(json)).await.is_err()
                    {
                        break;
                    }
                }
                let mut heartbeat = TimeoutFuture::new(HEARTBEAT_MS).fuse();
                loop {
                    futures::select! {
                        incoming = read.next() => match incoming {
                            Some(Ok(Message::Text(text))) => {
                                if let Ok(frame) = serde_json::from_str::<ServerFrame>(&text) {
                                    client.dispatch(&frame);
                                }
                            }
                            Some(Ok(Message::Bytes(_))) => {}
                            Some(Err(_)) | None => break,
                        },
                        outgoing = rx.next() => match outgoing {
                            Some(frame) => {
                                if let Ok(json) = serde_json::to_string(&frame)
                                    && write.send(Message::Text(json)).await.is_err() {
                                        break;
                                    }
                            }
                            // Every sender dropped: stopped or the client is gone.
                            None => return,
                        },
                        () = heartbeat => {
                            if let Ok(json) = serde_json::to_string(&ClientFrame::Ping)
                                && write.send(Message::Text(json)).await.is_err() {
                                    break;
                                }
                            heartbeat = TimeoutFuture::new(HEARTBEAT_MS).fuse();
                        },
                    }
                }
                // A newer generation owns `connected` once this one is retired.
                if client.generation.get_value() == my_gen {
                    client.connected.set(false);
                }
            }
            Err(_) => client.connected.set(false),
        }
        TimeoutFuture::new(backoff).await;
        backoff = (backoff * 2).min(BACKOFF_MAX_MS);
    }
}
