//! The message composer: sends over the WebSocket when connected (the echo
//! arrives via the chat plane), or falls back to a REST post and inserts the
//! returned message. Emits typing signals as you type.

use leptos::prelude::*;
use leptos::task::spawn_local;

use shared::dto::chat::{MessageDto, SendMessageRequest};
use shared::dto::ids::ChannelId;
use shared::dto::ws::ClientFrame;

use crate::api::ws::WsClient;
use crate::features::chat::api;
use crate::features::chat::ws::push_message;
use crate::primitives::button::{Button, ButtonVariant};
use crate::primitives::icon::{Icon, IconName};
use crate::primitives::input::InputGroup;
use crate::state::toast::ToastState;
use crate::theme::{class, color, space};

#[component]
pub fn Composer(
    #[prop(into)] channel: Signal<Option<ChannelId>>,
    messages: RwSignal<Vec<MessageDto>>,
) -> impl IntoView {
    let ws = use_context::<WsClient>().expect("WsClient context");
    let toast = use_context::<ToastState>().expect("ToastState context");
    let text = RwSignal::new(String::new());

    let do_send = move || {
        let Some(cid) = channel.get_untracked() else {
            return;
        };
        let body = text.get_untracked();
        if body.trim().is_empty() {
            return;
        }
        text.set(String::new());
        if ws.connected.get_untracked() {
            ws.send(ClientFrame::SendMessage {
                channel_id: cid,
                body,
                mentions: Vec::new(),
                attachment_keys: Vec::new(),
            });
        } else {
            spawn_local(async move {
                let req = SendMessageRequest {
                    body,
                    mentions: Vec::new(),
                    attachment_keys: Vec::new(),
                };
                match api::send(cid, &req).await {
                    Ok(msg) => push_message(messages, msg),
                    Err(e) => toast.error_from(&e),
                }
            });
        }
    };

    let on_input = Callback::new(move |v: String| {
        let typing = !v.trim().is_empty();
        text.set(v);
        if typing && let Some(cid) = channel.get_untracked() {
            ws.send(ClientFrame::Typing { channel_id: cid });
        }
    });
    let send_btn = Callback::new(move |_| do_send());

    let wrap = class(format!(
        "padding: {p}; border-top: 1px solid {b}; background: {bg};",
        p = space::D3,
        b = color::BORDER,
        bg = color::BG_SUBTLE,
    ));

    let trailing = view! {
        <Button variant=ButtonVariant::Icon on_click=send_btn>
            <Icon name=IconName::Send size=15 />
        </Button>
    }
    .into_any();

    view! {
        <div class=wrap>
            <form on:submit=move |ev: leptos::ev::SubmitEvent| { ev.prevent_default(); do_send(); }>
                <InputGroup value=text on_input=on_input placeholder="Message…" trailing=trailing />
            </form>
        </div>
    }
}
