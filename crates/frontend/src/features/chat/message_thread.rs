//! The open channel's transcript: REST history (paged backwards via "load
//! older") merged with live [`ServerFrame`]s from the WebSocket, plus a typing
//! indicator.

use leptos::prelude::*;
use leptos::task::spawn_local;

use shared::dto::chat::MessageDto;
use shared::dto::ids::{ChannelId, MessageId};
use shared::dto::ws::ClientFrame;

use crate::api::ws::WsClient;
use crate::features::chat::api;
use crate::features::chat::ws::apply_server_frame;
use crate::primitives::avatar::{Avatar, AvatarSize};
use crate::primitives::pagination::LoadMore;
use crate::primitives::stack::{Gap, Stack};
use crate::theme::{class, color, space, typography};
use crate::util::format::{relative_time, tone_for};

const PAGE: u32 = 50;

#[component]
pub fn MessageThread(
    #[prop(into)] channel: Signal<Option<ChannelId>>,
    messages: RwSignal<Vec<MessageDto>>,
    typing: RwSignal<bool>,
) -> impl IntoView {
    let ws = use_context::<WsClient>().expect("WsClient context");
    let oldest = RwSignal::new(None::<MessageId>);
    let loading_older = RwSignal::new(false);

    // Channel switch: reset, subscribe over WS, load the latest page, mark read.
    Effect::new(move |_| {
        let Some(cid) = channel.get() else { return };
        messages.set(Vec::new());
        typing.set(false);
        oldest.set(None);
        ws.send(ClientFrame::Subscribe { channel_id: cid });
        spawn_local(async move {
            if let Ok(mut history) = api::messages(cid, None, PAGE).await {
                history.reverse();
                oldest.set(history.first().map(|m| m.id));
                messages.set(history);
            }
            let _ = api::mark_read(cid).await;
        });
    });

    // Live frames for the open channel.
    Effect::new(move |_| {
        if let Some(frame) = ws.last_frame.get()
            && let Some(cid) = channel.get_untracked() {
                apply_server_frame(&frame, cid, messages, typing);
            }
    });

    let load_older = Callback::new(move |()| {
        let Some(cid) = channel.get_untracked() else { return };
        let Some(before) = oldest.get_untracked() else { return };
        loading_older.set(true);
        spawn_local(async move {
            if let Ok(mut older) = api::messages(cid, Some(before), PAGE).await {
                older.reverse();
                if let Some(first) = older.first() {
                    oldest.set(Some(first.id));
                }
                if !older.is_empty() {
                    messages.update(move |v| {
                        let mut combined = older;
                        combined.append(v);
                        *v = combined;
                    });
                }
            }
            loading_older.set(false);
        });
    });

    let scroll = class(format!(
        "flex: 1; min-height: 0; overflow-y: auto; padding: {p} 0; display: flex; flex-direction: column;",
        p = space::D2,
    ));
    let typing_cls = class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c}; padding: {p};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_CAPTION,
        c = color::TEXT_FAINT,
        p = space::D3,
    ));

    let empty_cls = typing_cls.clone();
    view! {
        <div class=scroll>
            <Show when=move || oldest.get().is_some() && !messages.get().is_empty() fallback=|| ()>
                <LoadMore on_click=load_older loading=loading_older.into() label="Load older" />
            </Show>
            {move || {
                let list = messages.get();
                if list.is_empty() {
                    view! { <div class=empty_cls.clone()>"No messages yet — say hello."</div> }.into_any()
                } else {
                    view! { <Stack gap=Gap::Xs>{list.into_iter().map(message_row).collect_view()}</Stack> }.into_any()
                }
            }}
            <Show when=move || typing.get() fallback=|| ()>
                <div class=typing_cls.clone()>"Someone is typing…"</div>
            </Show>
        </div>
    }
}

fn message_row(m: MessageDto) -> impl IntoView {
    let author = m.sender.full_name.clone();
    let when = relative_time(m.created_at);
    let deleted = m.deleted_at.is_some();
    let edited = m.edited_at.is_some();
    let body = m.body.clone();

    let row = class(format!(
        "display: flex; gap: {g}; padding: {py} {px}; border-radius: {r}; \
         &:hover {{ background: {bh}; }}",
        g = space::D3,
        py = space::D2,
        px = space::D4,
        r = "6px",
        bh = color::BG_SUBTLE,
    ));
    let bodywrap = class("min-width: 0; flex: 1;");
    let meta = class("display: flex; align-items: baseline; gap: 8px; margin-bottom: 2px;");
    let author_cls = class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    let time_cls = class(format!(
        "font-family: {ff}; font-size: 11.5px; color: {c};",
        ff = typography::FONT_SANS,
        c = color::TEXT_FAINT,
    ));
    let text_cls = class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c}; line-height: 1.5; word-wrap: break-word; \
         white-space: pre-wrap;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT,
    ));
    let deleted_cls = class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c}; font-style: italic;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT_FAINT,
    ));
    let when_label = if edited && !deleted { format!("{when} · edited") } else { when };

    view! {
        <div class=row>
            <Avatar name=author.clone() size=AvatarSize::Sm tone=tone_for(&author) />
            <div class=bodywrap>
                <div class=meta>
                    <span class=author_cls>{author}</span>
                    <span class=time_cls>{when_label}</span>
                </div>
                {if deleted {
                    view! { <div class=deleted_cls>"message deleted"</div> }.into_any()
                } else {
                    view! { <div class=text_cls>{body}</div> }.into_any()
                }}
            </div>
        </div>
    }
}
