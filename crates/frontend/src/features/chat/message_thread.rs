//! The open channel's transcript: REST history merged with live [`ServerFrame`]s from the WebSocket, plus a typing indicator.

use leptos::{prelude::*, task};

use shared::dto::chat::{EditMessageRequest, MessageDto};
use shared::dto::ids::{ChannelId, MessageId, UserId};
use shared::dto::ws::ClientFrame;
use shared::validation::chat;

use crate::api::ws::WsClient;
use crate::features::chat::api;
use crate::features::chat::ws;
use crate::primitives::avatar::{Avatar, AvatarSize};
use crate::primitives::button::{Button, ButtonSize, ButtonVariant};
use crate::primitives::cluster::Cluster;
use crate::primitives::dialog::{Dialog, DialogBody, DialogFooter, DialogHeader};
use crate::primitives::icon::{Icon, IconName};
use crate::primitives::pagination::LoadMore;
use crate::primitives::stack::{Gap, Stack};
use crate::primitives::textarea::Textarea;
use crate::state::auth::AuthState;
use crate::state::toast::ToastState;
use crate::theme::{self, color, space, typography};
use crate::util::format;

const PAGE: u32 = 50;

#[component]
pub fn MessageThread(
    #[prop(into)] channel: Signal<Option<ChannelId>>,
    messages: RwSignal<Vec<MessageDto>>,
    typing: RwSignal<bool>,
) -> impl IntoView {
    let ws = use_context::<WsClient>().expect("WsClient context");
    let toast = use_context::<ToastState>().expect("ToastState context");
    let auth = use_context::<AuthState>().expect("AuthState context");
    let me = Signal::derive(move || auth.user.get().map(|u| u.id));
    let oldest = RwSignal::new(None::<MessageId>);
    let loading_older = RwSignal::new(false);

    // Shared edit-dialog target, populated from the caller's own message rows.
    let edit_open = RwSignal::new(false);
    let edit_msg = RwSignal::new(None::<MessageId>);
    let edit_body = RwSignal::new(String::new());

    let begin_edit = move |mid: MessageId, body: String| {
        edit_msg.set(Some(mid));
        edit_body.set(body);
        edit_open.set(true);
    };
    // The WS echo (apply_server_frame) folds the delete back into messages, so no optimistic update here.
    let do_delete = move |cid: ChannelId, mid: MessageId| {
        task::spawn_local(async move {
            if let Err(e) = api::delete(cid, mid).await {
                toast.error_from(&e);
            }
        });
    };

    // Channel switch: reset, subscribe over WS, load the latest page, mark read.
    Effect::new(move |_| {
        let Some(cid) = channel.get() else { return };
        messages.set(Vec::new());
        typing.set(false);
        oldest.set(None);
        ws.send(ClientFrame::Subscribe { channel_id: cid });
        task::spawn_local(async move {
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
            && let Some(cid) = channel.get_untracked()
        {
            ws::apply_server_frame(&frame, cid, messages, typing);
        }
    });

    let load_older = Callback::new(move |()| {
        let Some(cid) = channel.get_untracked() else {
            return;
        };
        let Some(before) = oldest.get_untracked() else {
            return;
        };
        loading_older.set(true);
        task::spawn_local(async move {
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

    let scroll = theme::class(format!(
        "flex: 1; min-height: 0; overflow-y: auto; padding: {p} 0; display: flex; flex-direction: column;",
        p = space::D2,
    ));
    let typing_cls = theme::class(format!(
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
                let cid = channel.get();
                let me_id = me.get();
                if list.is_empty() {
                    view! { <div class=empty_cls.clone()>"No messages yet — say hello."</div> }.into_any()
                } else {
                    let rows = list
                        .into_iter()
                        .map(|m| message_row(m, cid, me_id, begin_edit, do_delete))
                        .collect_view();
                    view! { <Stack gap=Gap::Xs>{rows}</Stack> }.into_any()
                }
            }}
            <Show when=move || typing.get() fallback=|| ()>
                <div class=typing_cls.clone()>"Someone is typing…"</div>
            </Show>
        </div>
        <MessageEditDialog open=edit_open channel=channel message=edit_msg body=edit_body />
    }
}

fn message_row(
    m: MessageDto,
    channel: Option<ChannelId>,
    me: Option<UserId>,
    begin_edit: impl Fn(MessageId, String) + Copy + Send + Sync + 'static,
    do_delete: impl Fn(ChannelId, MessageId) + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let author = m.sender.full_name.clone();
    let when = format::relative_time(m.created_at);
    let deleted = m.deleted_at.is_some();
    let edited = m.edited_at.is_some();
    let is_mine = me.is_some() && me == Some(m.sender.id);
    let mid = m.id;
    let edit_seed = m.body.clone();
    let body = m.body.clone();
    let attachments = m.attachments.clone();

    let row = theme::class(format!(
        "display: flex; gap: {g}; padding: {py} {px}; border-radius: {r}; \
         &:hover {{ background: {bh}; }}",
        g = space::D3,
        py = space::D2,
        px = space::D4,
        r = "6px",
        bh = color::BG_SUBTLE,
    ));
    let bodywrap = theme::class("min-width: 0; flex: 1;");
    let meta = theme::class("display: flex; align-items: center; gap: 8px; margin-bottom: 2px;");
    let author_cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    let time_cls = theme::class(format!(
        "font-family: {ff}; font-size: 11.5px; color: {c};",
        ff = typography::FONT_SANS,
        c = color::TEXT_FAINT,
    ));
    let text_cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c}; line-height: 1.5; word-wrap: break-word; \
         white-space: pre-wrap;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT,
    ));
    let deleted_cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c}; font-style: italic;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT_FAINT,
    ));
    let when_label = if edited && !deleted {
        format!("{when} · edited")
    } else {
        when
    };

    // Edit/Delete only on your own, still-present messages; the server stays authority and failures surface as toasts.
    let controls = match (is_mine && !deleted).then_some(channel).flatten() {
        Some(cid) => {
            let actions_cls = theme::class("margin-left: auto;");
            let edit_cb = Callback::new(move |_| begin_edit(mid, edit_seed.clone()));
            let delete_cb = Callback::new(move |_| do_delete(cid, mid));
            view! {
                <div class=actions_cls>
                    <Cluster gap=Gap::Xs>
                        <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm on_click=edit_cb>"Edit"</Button>
                        <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm on_click=delete_cb>"Delete"</Button>
                    </Cluster>
                </div>
            }
            .into_any()
        }
        None => ().into_any(),
    };

    view! {
        <div class=row>
            <Avatar name=author.clone() size=AvatarSize::Sm tone=format::tone_for(&author) />
            <div class=bodywrap>
                <div class=meta>
                    <span class=author_cls>{author}</span>
                    <span class=time_cls>{when_label}</span>
                    {controls}
                </div>
                {if deleted {
                    view! { <div class=deleted_cls>"message deleted"</div> }.into_any()
                } else {
                    view! { <div class=text_cls>{body}</div> }.into_any()
                }}
                {if deleted || attachments.is_empty() {
                    ().into_any()
                } else {
                    attachment_views(attachments)
                }}
            </div>
        </div>
    }
}

/// Renders attachments: images inline, everything else as a paperclip file row; URLs are presigned per viewer.
fn attachment_views(attachments: Vec<shared::dto::chat::ChatAttachmentDto>) -> AnyView {
    let img_cls = theme::class(format!(
        "max-width: 320px; max-height: 240px; border-radius: 6px; border: 1px solid {b}; \
         display: block;",
        b = color::BORDER,
    ));
    let file_cls = theme::class(format!(
        "display: inline-flex; align-items: center; gap: 6px; font-family: {ff}; \
         font-size: {fs}; color: {c}; text-decoration: none; &:hover {{ color: {a}; }}",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT_MUTED,
        a = color::ACCENT,
    ));
    let views = attachments
        .into_iter()
        .map(|a| {
            let href = a.download_url.clone();
            if a.content_type.starts_with("image/") {
                let img = img_cls.clone();
                let src = href.clone();
                let alt = a.filename.clone();
                view! {
                    <a href=href target="_blank" rel="noopener">
                        <img class=img src=src alt=alt />
                    </a>
                }
                .into_any()
            } else {
                let file = file_cls.clone();
                let label = format!("{} ({})", a.filename, human_size(a.size_bytes));
                view! {
                    <a class=file href=href target="_blank" rel="noopener">
                        <Icon name=IconName::Paperclip size=13 />
                        {label}
                    </a>
                }
                .into_any()
            }
        })
        .collect_view();
    view! { <Stack gap=Gap::Xs>{views}</Stack> }.into_any()
}

fn human_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

#[component]
fn MessageEditDialog(
    open: RwSignal<bool>,
    #[prop(into)] channel: Signal<Option<ChannelId>>,
    #[prop(into)] message: Signal<Option<MessageId>>,
    body: RwSignal<String>,
) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let submitting = RwSignal::new(false);
    let on_close = Callback::new(move |()| open.set(false));
    let cancel = Callback::new(move |_| open.set(false));

    let submit = Callback::new(move |_| {
        if submitting.get_untracked() {
            return;
        }
        let (Some(cid), Some(mid)) = (channel.get_untracked(), message.get_untracked()) else {
            return;
        };
        let b = body.get_untracked();
        if let Err(e) = chat::validate_message_body(&b) {
            toast.error(e.to_string());
            return;
        }
        submitting.set(true);
        let req = EditMessageRequest { body: b };
        task::spawn_local(async move {
            match api::edit(cid, mid, &req).await {
                Ok(_) => {
                    toast.success("Message updated");
                    open.set(false);
                }
                Err(e) => toast.error_from(&e),
            }
            submitting.set(false);
        });
    });

    view! {
        <Dialog open=open on_close=on_close>
            <DialogHeader title="Edit message" subtitle="Update your message." />
            <DialogBody>
                <Textarea value=body on_input=Callback::new(move |v| body.set(v)) rows=4 />
            </DialogBody>
            <DialogFooter>
                <Button variant=ButtonVariant::Ghost on_click=cancel>"Cancel"</Button>
                <Button variant=ButtonVariant::Primary on_click=submit disabled=submitting.get()>
                    {move || if submitting.get() { "Saving…" } else { "Save" }}
                </Button>
            </DialogFooter>
        </Dialog>
    }
}
