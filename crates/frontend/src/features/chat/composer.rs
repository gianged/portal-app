//! Message composer: sends over the WebSocket when connected, else falls back to a REST post; emits typing signals and attaches files by storage key on send.

use leptos::{html::Input as HtmlInputEl, prelude::*, task::spawn_local};
use web_sys::FormData;

use shared::dto::chat::{ChatAttachmentDto, MessageDto, SendMessageRequest};
use shared::dto::ids::ChannelId;
use shared::dto::ws::ClientFrame;

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

#[component]
pub fn Composer(
    #[prop(into)] channel: Signal<Option<ChannelId>>,
    messages: RwSignal<Vec<MessageDto>>,
) -> impl IntoView {
    let ws = use_context::<WsClient>().expect("WsClient context");
    let toast = use_context::<ToastState>().expect("ToastState context");
    let text = RwSignal::new(String::new());
    let pending = RwSignal::new(Vec::<ChatAttachmentDto>::new());
    let file_ref: NodeRef<HtmlInputEl> = NodeRef::new();

    // Channel switch drops pending uploads; keys are channel-bound.
    Effect::new(move |_| {
        let _ = channel.get();
        pending.set(Vec::new());
    });

    let do_send = move || {
        let Some(cid) = channel.get_untracked() else {
            return;
        };
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
            ws.send(ClientFrame::SendMessage {
                channel_id: cid,
                body,
                mentions: Vec::new(),
                attachment_keys,
            });
        } else {
            spawn_local(async move {
                let req = SendMessageRequest {
                    body,
                    mentions: Vec::new(),
                    attachment_keys,
                };
                match api::send(cid, &req).await {
                    Ok(msg) => ws::push_message(messages, msg),
                    Err(e) => toast.error_from(&e),
                }
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
        let blob: &web_sys::Blob = file.as_ref();
        let _ = form.append_with_blob_and_filename("file", blob, &file.name());
        spawn_local(async move {
            match api::upload_attachment(cid, form).await {
                Ok(attachment) => pending.update(|v| v.push(attachment)),
                Err(e) => toast.error_from(&e),
            }
        });
    };

    let on_input = Callback::new(move |v: String| {
        let typing = !v.trim().is_empty();
        text.set(v);
        if typing && let Some(cid) = channel.get_untracked() {
            ws.send(ClientFrame::Typing { channel_id: cid });
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

    let trailing = view! {
        <Button variant=ButtonVariant::Icon on_click=pick_file>
            <Icon name=IconName::Paperclip size=15 />
        </Button>
        <Button variant=ButtonVariant::Icon on_click=send_btn>
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
                <form on:submit=move |ev: leptos::ev::SubmitEvent| { ev.prevent_default(); do_send(); }>
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
            <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm on_click=remove>"✕"</Button>
        </span>
    }
    .into_any()
}
