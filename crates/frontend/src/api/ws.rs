//! The single chat WebSocket client. Built once at the app root and shared via
//! context; chat components send [`ClientFrame`]s through [`WsClient::send`] and
//! react to incoming [`ServerFrame`]s by tracking [`WsClient::last_frame`].
//!
//! A background task owns the (non-`Send`) socket and runs on the JS event loop
//! via `spawn_local`. Outbound frames flow through an unbounded channel so the
//! `WsClient` handle stays cheap and `Copy`; the task reconnects with backoff and
//! a separate heartbeat task keeps presence alive with periodic `Ping`s.

use futures::{
    SinkExt, StreamExt,
    channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded},
};
use gloo::timers::future::TimeoutFuture;
use leptos::{prelude::*, task::spawn_local};
use reqwasm::websocket::{Message, futures::WebSocket};

use shared::dto::ws::{ClientFrame, ServerFrame};

const HEARTBEAT_MS: u32 = 25_000;
const BACKOFF_MIN_MS: u32 = 500;
const BACKOFF_MAX_MS: u32 = 10_000;

#[derive(Clone, Copy)]
pub struct WsClient {
    tx: StoredValue<UnboundedSender<ClientFrame>>,
    /// Holds the receiver until [`WsClient::start`] hands it to the read task.
    rx: StoredValue<Option<UnboundedReceiver<ClientFrame>>>,
    started: RwSignal<bool>,
    /// The most recent frame pushed by the server. Subscribers react in an
    /// `Effect`; each socket message arrives in its own task poll, so frames are
    /// observed one at a time rather than coalesced.
    pub last_frame: RwSignal<Option<ServerFrame>>,
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
        let (tx, rx) = unbounded::<ClientFrame>();
        Self {
            tx: StoredValue::new(tx),
            rx: StoredValue::new(Some(rx)),
            started: RwSignal::new(false),
            last_frame: RwSignal::new(None),
            connected: RwSignal::new(false),
        }
    }

    /// Open the socket and spawn the read/reconnect + heartbeat tasks. Idempotent:
    /// safe to call from an effect that fires whenever auth resolves.
    pub fn start(&self) {
        if self.started.get_untracked() {
            return;
        }
        let Some(rx) = self.rx.try_update_value(Option::take).flatten() else {
            return;
        };
        self.started.set(true);
        spawn_local(run(rx, self.last_frame, self.connected));

        let heartbeat_tx = self.tx.get_value();
        spawn_local(async move {
            loop {
                TimeoutFuture::new(HEARTBEAT_MS).await;
                if heartbeat_tx.unbounded_send(ClientFrame::Ping).is_err() {
                    break;
                }
            }
        });
    }

    /// Queue a frame for delivery. Frames sent while reconnecting are buffered and
    /// flushed once the socket is back. Errors (channel closed) are swallowed since
    /// chat actions have a REST fallback.
    pub fn send(&self, frame: ClientFrame) {
        self.tx.with_value(|tx| {
            let _ = tx.unbounded_send(frame);
        });
    }
}

/// The websocket URL on the current origin (`/api/chat/ws`, which the Trunk proxy
/// and the server's `/api/v1` mount resolve to `/api/v1/chat/ws`).
fn ws_url() -> String {
    let Some(window) = web_sys::window() else {
        return "ws://127.0.0.1:8080/api/v1/chat/ws".to_owned();
    };
    let loc = window.location();
    let scheme = match loc.protocol().as_deref() {
        Ok("https:") => "wss",
        _ => "ws",
    };
    let host = loc.host().unwrap_or_default();
    format!("{scheme}://{host}/api/chat/ws")
}

/// Owns the socket for the app's lifetime: connect, pump frames both ways, and
/// reconnect with exponential backoff on disconnect.
async fn run(
    mut rx: UnboundedReceiver<ClientFrame>,
    last_frame: RwSignal<Option<ServerFrame>>,
    connected: RwSignal<bool>,
) {
    let mut backoff = BACKOFF_MIN_MS;
    loop {
        match WebSocket::open(&ws_url()) {
            Ok(ws) => {
                backoff = BACKOFF_MIN_MS;
                connected.set(true);
                let (mut write, read) = ws.split();
                // `select!` needs fused futures; the split read half isn't a
                // `FusedStream`, so fuse it (the receiver already is).
                let mut read = read.fuse();
                loop {
                    futures::select! {
                        incoming = read.next() => match incoming {
                            Some(Ok(Message::Text(text))) => {
                                if let Ok(frame) = serde_json::from_str::<ServerFrame>(&text) {
                                    last_frame.set(Some(frame));
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
                            // Sender dropped: the WsClient is gone; stop entirely.
                            None => return,
                        },
                    }
                }
                connected.set(false);
            }
            Err(_) => connected.set(false),
        }
        TimeoutFuture::new(backoff).await;
        backoff = (backoff * 2).min(BACKOFF_MAX_MS);
    }
}
