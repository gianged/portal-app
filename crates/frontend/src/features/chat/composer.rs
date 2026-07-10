//! Message composer: sends over the WebSocket when connected, else falls back to a REST post; emits typing signals and attaches files by storage key on send.

use gloo::timers::future::TimeoutFuture;
use leptos::{ev::SubmitEvent, html::Input as HtmlInputEl, prelude::*, task};
use web_sys::{Blob, FormData};

use shared::dto::chat::{ChatAttachmentDto, MessageDto, SendMessageRequest};
use shared::dto::ids::ChannelId;
use shared::dto::ws::{ClientFrame, ServerFrame};

use crate::api::ws::WsClient;
use crate::features::chat::api;
use crate::features::chat::ws;
use crate::primitives::button::{Button, ButtonSize, ButtonVariant};
use crate::primitives::cluster::Cluster;
use crate::primitives::icon::{Icon, IconName};
use crate::primitives::input::InputGroup;
use crate::primitives::stack::{Gap, Stack};
use crate::state::toast::ToastState;
use crate::theme::{self, color, radius, space, typography};

const TYPING_THROTTLE_MS: u32 = 2_500;

#[component]
pub fn Composer(
    #[prop(into)] channel: Signal<Option<ChannelId>>,
    messages: RwSignal<Vec<MessageDto>>,
) -> impl IntoView {
    let ws = use_context::<WsClient>().expect("WsClient context");
    let toast = use_context::<ToastState>().expect("ToastState context");
    let text = RwSignal::new(String::new());
    let pending = RwSignal::new(Vec::<ChatAttachmentDto>::new());
    let sending = RwSignal::new(false);
    let file_ref: NodeRef<HtmlInputEl> = NodeRef::new();
    let last_sent = StoredValue::new(None::<String>);

    // Channel switch drops pending uploads; keys are channel-bound.
    Effect::new(move |_| {
        let _ = channel.get();
        pending.set(Vec::new());
    });

    // The composer clears on send; a failed WS send would lose the text silently.
    let restore_handler = ws.on_frame(move |frame| {
        if let ServerFrame::Error { code, .. } = frame
            && matches!(code.as_str(), "send_failed" | "rate_limited")
            && let Some(prev) = last_sent.try_update_value(Option::take).flatten()
            && text.get_untracked().is_empty()
        {
            text.set(prev);
        }
    });
    on_cleanup(move || ws.off_frame(restore_handler));

    let do_send = move || {
        let Some(cid) = channel.get_untracked() else {
            return;
        };
        if sending.get_untracked() {
            return;
        }
        let body = text.get_untracked();
        if body.trim().is_empty() {
            return;
        }
        let attachment_keys: Vec<String> = pending
            .get_untracked()
            .iter()
            .map(|a| a.storage_key.clone())
            .collect();
        text.set(String::new());
        pending.set(Vec::new());
        if ws.connected.get_untracked() {
            last_sent.set_value(Some(body.clone()));
            ws.send(ClientFrame::SendMessage {
                channel_id: cid,
                body,
                mentions: Vec::new(),
                attachment_keys,
            });
        } else {
            sending.set(true);
            let restore = body.clone();
            task::spawn_local(async move {
                let req = SendMessageRequest {
                    body,
                    mentions: Vec::new(),
                    attachment_keys,
                };
                match api::send(cid, &req).await {
                    Ok(msg) => ws::push_message(messages, msg),
                    Err(e) => {
                        toast.error_from(&e);
                        // Give the typed text back unless a new draft was started.
                        if text.get_untracked().is_empty() {
                            text.set(restore);
                        }
                    }
                }
                sending.set(false);
            });
        }
    };

    let pick_file = Callback::new(move |_| {
        if let Some(input) = file_ref.get() {
            input.click();
        }
    });
    let on_file = move |_| {
        let Some(input) = file_ref.get() else { return };
        let Some(files) = input.files() else { return };
        let Some(file) = files.get(0) else { return };
        input.set_value("");
        let Some(cid) = channel.get_untracked() else {
            return;
        };
        let form = FormData::new().expect("FormData is constructible in the browser");
        let blob: &Blob = file.as_ref();
        let _ = form.append_with_blob_and_filename("file", blob, &file.name());
        task::spawn_local(async move {
            match api::upload_attachment(cid, form).await {
                Ok(attachment) => pending.update(|v| v.push(attachment)),
                Err(e) => toast.error_from(&e),
            }
        });
    };

    // Throttle gate: at most one Typing frame per TYPING_THROTTLE_MS.
    let typing_throttled = StoredValue::new(false);
    let on_input = Callback::new(move |v: String| {
        let typing = !v.trim().is_empty();
        text.set(v);
        if typing
            && let Some(cid) = channel.get_untracked()
            && !typing_throttled.get_value()
        {
            typing_throttled.set_value(true);
            ws.send(ClientFrame::Typing { channel_id: cid });
            task::spawn_local(async move {
                TimeoutFuture::new(TYPING_THROTTLE_MS).await;
                typing_throttled.set_value(false);
            });
        }
    });
    let send_btn = Callback::new(move |_| do_send());

    let wrap = theme::class(format!(
        "padding: {p}; border-top: 1px solid {b}; background: {bg};",
        p = space::D3,
        b = color::BORDER,
        bg = color::BG_SUBTLE,
    ));
    let hidden_input = theme::class("display: none;");

    let send_disabled = Signal::derive(move || text.get().trim().is_empty() || sending.get());
    let trailing = view! {
        <Button variant=ButtonVariant::Icon on_click=pick_file>
            <Icon name=IconName::Paperclip size=15 />
        </Button>
        <Button variant=ButtonVariant::Icon on_click=send_btn disabled=send_disabled>
            <Icon name=IconName::Send size=15 />
        </Button>
    }
    .into_any();

    view! {
        <div class=wrap>
            <Stack gap=Gap::Xs>
                <Show when=move || !pending.get().is_empty() fallback=|| ()>
                    <Cluster gap=Gap::Xs>
                        {move || pending.get().into_iter().map(|a| pending_chip(&a, pending)).collect_view()}
                    </Cluster>
                </Show>
                <form on:submit=move |ev: SubmitEvent| { ev.prevent_default(); do_send(); }>
                    <InputGroup value=text on_input=on_input placeholder="Message…" trailing=trailing />
                </form>
            </Stack>
            <input type="file" class=hidden_input node_ref=file_ref on:change=on_file />
        </div>
    }
}

/// One pending-upload chip with a remove button.
fn pending_chip(a: &ChatAttachmentDto, pending: RwSignal<Vec<ChatAttachmentDto>>) -> AnyView {
    let chip_cls = theme::class(format!(
        "display: inline-flex; align-items: center; gap: 6px; padding: 2px {p}; \
         border: 1px solid {b}; border-radius: {r}; font-family: {ff}; font-size: {fs}; color: {c};",
        p = space::D2,
        b = color::BORDER,
        r = radius::SM,
        ff = typography::FONT_SANS,
        fs = typography::TEXT_CAPTION,
        c = color::TEXT_MUTED,
    ));
    let key = a.storage_key.clone();
    let remove = Callback::new(move |_| {
        let key = key.clone();
        pending.update(|v| v.retain(|p| p.storage_key != key));
    });
    let filename = a.filename.clone();
    view! {
        <span class=chip_cls>
            <Icon name=IconName::Paperclip size=12 />
            {filename}
            <Button variant=ButtonVariant::Icon size=ButtonSize::Sm on_click=remove>
                <Icon name=IconName::Close size=12 />
            </Button>
        </span>
    }
    .into_any()
}
