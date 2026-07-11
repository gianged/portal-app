//! The open channel's transcript: REST history merged with live [`ServerFrame`]s from the WebSocket, plus a typing indicator.

use std::sync::LazyLock;

use leptos::{prelude::*, task};

use shared::dto::chat::{ChatAttachmentDto, EditMessageRequest, MessageDto};
use shared::dto::ids::{ChannelId, MessageId, UserId};
use shared::dto::ws::ClientFrame;
use shared::validation::chat;

use crate::api::error::FrontendError;
use crate::api::ws::WsClient;
use crate::features::chat::api;
use crate::features::chat::ws;
use crate::primitives::avatar::{Avatar, AvatarSize};
use crate::primitives::button::{Button, ButtonSize, ButtonVariant};
use crate::primitives::cluster::Cluster;
use crate::primitives::confirm::ConfirmDialog;
use crate::primitives::dialog::{Dialog, DialogBody, DialogFooter, DialogHeader};
use crate::primitives::icon::{Icon, IconName};
use crate::primitives::pagination::LoadMore;
use crate::primitives::stack::{Gap, Stack};
use crate::primitives::textarea::Textarea;
use crate::state::auth::AuthState;
use crate::state::toast::ToastState;
use crate::theme::{self, color, radius, space, typography};
use crate::util::format;
use crate::util::load;

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
    let history_err = RwSignal::new(None::<FrontendError>);

    // Shared edit-dialog target, populated from the caller's own message rows.
    let edit_open = RwSignal::new(false);
    let edit_msg = RwSignal::new(None::<MessageId>);
    let edit_body = RwSignal::new(String::new());

    let begin_edit = move |mid: MessageId, body: String| {
        edit_msg.set(Some(mid));
        edit_body.set(body);
        edit_open.set(true);
    };
    // Deleting asks for confirmation first; the WS echo (apply_server_frame)
    // folds the delete back into messages, so no optimistic update here.
    let confirm_delete = RwSignal::new(None::<(ChannelId, MessageId)>);
    let do_delete = move |cid: ChannelId, mid: MessageId| confirm_delete.set(Some((cid, mid)));
    let run_delete = Callback::new(move |()| {
        let Some((cid, mid)) = confirm_delete.get_untracked() else {
            return;
        };
        task::spawn_local(async move {
            if let Err(e) = api::delete(cid, mid).await {
                toast.error_from(&e);
            }
        });
    });

    // Channel switch: reset, subscribe over WS, load the latest page, mark read.
    Effect::new(move |_| {
        let Some(cid) = channel.get() else { return };
        messages.set(Vec::new());
        typing.set(false);
        oldest.set(None);
        history_err.set(None);
        ws.send(ClientFrame::Subscribe { channel_id: cid });
        task::spawn_local(async move {
            match api::messages(cid, None, PAGE).await {
                Ok(mut history) => {
                    history.reverse();
                    oldest.set(history.first().map(|m| m.id));
                    messages.set(history);
                }
                Err(e) => history_err.set(Some(e)),
            }
            // Best-effort; unread state self-corrects on the next channel list load.
            let _ = api::mark_read(cid).await;
        });
    });

    // Live frames for the open channel.
    let typing_gen = StoredValue::new(0u64);
    let frame_handler = ws.on_frame(move |frame| {
        if let Some(cid) = channel.get_untracked() {
            ws::apply_server_frame(frame, cid, messages, typing, typing_gen);
        }
    });
    on_cleanup(move || ws.off_frame(frame_handler));

    let load_older = Callback::new(move |()| {
        let Some(cid) = channel.get_untracked() else {
            return;
        };
        let Some(before) = oldest.get_untracked() else {
            return;
        };
        loading_older.set(true);
        task::spawn_local(async move {
            match api::messages(cid, Some(before), PAGE).await {
                Ok(mut older) => {
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
                Err(e) => toast.error_from(&e),
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
    let is_empty = Memo::new(move |_| messages.with(Vec::is_empty));
    view! {
        <div class=scroll>
            <Show when=move || oldest.get().is_some() && !messages.get().is_empty() fallback=|| ()>
                <LoadMore on_click=load_older loading=loading_older.into() label="Load older" />
            </Show>
            <Show
                when=move || !is_empty.get()
                fallback=move || match history_err.get() {
                    Some(e) => load::load_error(&e),
                    None => view! { <div class=empty_cls.clone()>"No messages yet — say hello."</div> }.into_any(),
                }
            >
                <Stack gap=Gap::Xs>
                    <For
                        each=move || messages.get()
                        key=|m| (m.id, m.edited_at, m.deleted_at)
                        children=move |m| {
                            message_row(&m, channel.get_untracked(), me.get_untracked(), begin_edit, do_delete)
                        }
                    />
                </Stack>
            </Show>
            <Show when=move || typing.get() fallback=|| ()>
                <div class=typing_cls.clone()>"Someone is typing…"</div>
            </Show>
        </div>
        <MessageEditDialog open=edit_open channel=channel message=edit_msg body=edit_body />
        <ConfirmDialog
            open=Signal::derive(move || confirm_delete.get().is_some())
            title="Delete message"
            message=Signal::derive(|| "Delete this message? This can't be undone.".to_owned())
            confirm_label="Delete"
            on_confirm=run_delete
            on_close=Callback::new(move |()| confirm_delete.set(None))
        />
    }
}

/// Row and attachment classes, built once; row rendering repeats per message.
struct RowClasses {
    row: String,
    bodywrap: String,
    meta: String,
    author: String,
    time: String,
    text: String,
    deleted: String,
    actions: String,
    img: String,
    file: String,
}

static ROW_CLS: LazyLock<RowClasses> = LazyLock::new(|| RowClasses {
    row: theme::class(format!(
        "display: flex; gap: {g}; padding: {py} {px}; border-radius: {r}; \
         &:hover {{ background: {bh}; }}",
        g = space::D3,
        py = space::D2,
        px = space::D4,
        r = radius::SM,
        bh = color::BG_SUBTLE,
    )),
    bodywrap: theme::class("min-width: 0; flex: 1;"),
    meta: theme::class("display: flex; align-items: center; gap: 8px; margin-bottom: 2px;"),
    author: theme::class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    )),
    time: theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_TINY,
        c = color::TEXT_FAINT,
    )),
    text: theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c}; line-height: 1.5; word-wrap: break-word; \
         white-space: pre-wrap;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT,
    )),
    deleted: theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c}; font-style: italic;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT_FAINT,
    )),
    actions: theme::class("margin-left: auto;"),
    img: theme::class(format!(
        "max-width: 320px; max-height: 240px; border-radius: {r}; border: 1px solid {b}; \
         display: block;",
        r = radius::SM,
        b = color::BORDER,
    )),
    file: theme::class(format!(
        "display: inline-flex; align-items: center; gap: 6px; font-family: {ff}; \
         font-size: {fs}; color: {c}; text-decoration: none; &:hover {{ color: {a}; }}",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT_MUTED,
        a = color::ACCENT,
    )),
});

fn message_row(
    m: &MessageDto,
    channel: Option<ChannelId>,
    me: Option<UserId>,
    begin_edit: impl Fn(MessageId, String) + Copy + Send + Sync + 'static,
    do_delete: impl Fn(ChannelId, MessageId) + Copy + Send + Sync + 'static,
) -> AnyView {
    let author = m.sender.full_name.clone();
    let when = format::relative_time(m.created_at);
    let deleted = m.deleted_at.is_some();
    let edited = m.edited_at.is_some();
    let is_mine = me.is_some() && me == Some(m.sender.id);
    let mid = m.id;
    let edit_seed = m.body.clone();
    let body = m.body.clone();
    let attachments = m.attachments.clone();

    let cls = &*ROW_CLS;
    let when_label = if edited && !deleted {
        format!("{when} · edited")
    } else {
        when
    };

    // Edit/Delete only on your own, still-present messages; the server stays authority and failures surface as toasts.
    let controls = match (is_mine && !deleted).then_some(channel).flatten() {
        Some(cid) => {
            let edit_cb = Callback::new(move |_| begin_edit(mid, edit_seed.clone()));
            let delete_cb = Callback::new(move |_| do_delete(cid, mid));
            view! {
                <div class=cls.actions.clone()>
                    <Cluster gap=Gap::Xs>
                        <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm on_click=edit_cb>"Edit"</Button>
                        <Button variant=ButtonVariant::Destructive size=ButtonSize::Sm on_click=delete_cb>"Delete"</Button>
                    </Cluster>
                </div>
            }
            .into_any()
        }
        None => ().into_any(),
    };

    view! {
        <div class=cls.row.clone()>
            <Avatar name=author.clone() size=AvatarSize::Sm tone=format::tone_for(&author) />
            <div class=cls.bodywrap.clone()>
                <div class=cls.meta.clone()>
                    <span class=cls.author.clone()>{author}</span>
                    <span class=cls.time.clone()>{when_label}</span>
                    {controls}
                </div>
                {if deleted {
                    view! { <div class=cls.deleted.clone()>"message deleted"</div> }.into_any()
                } else {
                    view! { <div class=cls.text.clone()>{body}</div> }.into_any()
                }}
                {if deleted || attachments.is_empty() {
                    ().into_any()
                } else {
                    attachment_views(attachments)
                }}
            </div>
        </div>
    }
    .into_any()
}

/// Renders attachments: images inline, everything else as a paperclip file row; URLs are presigned per viewer.
fn attachment_views(attachments: Vec<ChatAttachmentDto>) -> AnyView {
    let views = attachments
        .into_iter()
        .map(|a| {
            let href = a.download_url.clone();
            if a.content_type.starts_with("image/") {
                let img = ROW_CLS.img.clone();
                let src = href.clone();
                let alt = a.filename.clone();
                view! {
                    <a href=href target="_blank" rel="noopener">
                        <img class=img src=src alt=alt />
                    </a>
                }
                .into_any()
            } else {
                let file = ROW_CLS.file.clone();
                let label = format!("{} ({})", a.filename, format::human_size(a.size_bytes));
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
                <Button variant=ButtonVariant::Primary on_click=submit disabled=Signal::derive(move || submitting.get())>
                    {move || if submitting.get() { "Saving…" } else { "Save" }}
                </Button>
            </DialogFooter>
        </Dialog>
    }
}
