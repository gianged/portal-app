//! Announcement UI: a channel-scoped feed (defaulting to General) with a
//! broadcast composer and grace-window edit/delete on your own posts.

use leptos::prelude::*;
use leptos::task::spawn_local;
use time::OffsetDateTime;
use uuid::Uuid;

use shared::dto::announcement::{
    AnnouncementDto, EDIT_GRACE_MINUTES, EditAnnouncementRequest, PostAnnouncementRequest,
};
use shared::dto::chat::{ChannelKind, ChannelSummaryDto};
use shared::dto::ids::{ChannelId, MessageId};
use shared::validation::announcement::validate_announcement_body;

use crate::features::announcements::api;
use crate::features::chat::api as chat_api;
use crate::features::ui::subtle;
use crate::primitives::avatar::{Avatar, AvatarSize};
use crate::primitives::badge::{Badge, BadgeVariant};
use crate::primitives::button::{Button, ButtonSize, ButtonVariant};
use crate::primitives::card::Card;
use crate::primitives::cluster::Cluster;
use crate::primitives::dialog::{Dialog, DialogBody, DialogFooter, DialogHeader};
use crate::primitives::empty_state::EmptyState;
use crate::primitives::icon::{Icon, IconName};
use crate::primitives::select::Select;
use crate::primitives::stack::{Gap, Stack};
use crate::primitives::textarea::Textarea;
use crate::state::toast::ToastState;
use crate::theme::{class, color, typography};
use crate::util::format::{relative_time, tone_for};
use crate::util::load::{Loadable, load, load_error, note};

/// Minutes left in the edit grace window, or `None` once it has closed.
fn remaining_edit_minutes(created_at: OffsetDateTime) -> Option<i64> {
    let elapsed = (OffsetDateTime::now_utc() - created_at).whole_minutes();
    let left = EDIT_GRACE_MINUTES - elapsed;
    (left > 0).then_some(left)
}

#[component]
pub fn AnnouncementsIndex() -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let channels: Loadable<Vec<ChannelSummaryDto>> = RwSignal::new(None);
    load(channels, chat_api::channels());
    let channel = RwSignal::new(None::<ChannelId>);
    let items: Loadable<Vec<AnnouncementDto>> = RwSignal::new(None);
    let reload = RwSignal::new(0u32);
    let post_open = RwSignal::new(false);

    // Default to the General channel once the list loads.
    Effect::new(move |_| {
        if channel.get_untracked().is_some() {
            return;
        }
        if let Some(Ok(list)) = channels.get()
            && let Some(c) = list.iter().find(|c| c.kind == ChannelKind::General)
        {
            channel.set(Some(c.id));
        }
    });

    Effect::new(move |_| {
        let _ = reload.get();
        if let Some(cid) = channel.get() {
            load(items, api::list(cid));
        }
    });

    let on_channel =
        Callback::new(move |s: String| channel.set(Uuid::parse_str(&s).ok().map(ChannelId)));
    let channel_value =
        Signal::derive(move || channel.get().map(|c| c.0.to_string()).unwrap_or_default());
    let open_post = Callback::new(move |_| post_open.set(true));
    let posted = Callback::new(move |()| reload.update(|n| *n += 1));

    // Shared edit-dialog target.
    let edit_open = RwSignal::new(false);
    let edit_channel = RwSignal::new(None::<ChannelId>);
    let edit_msg = RwSignal::new(None::<MessageId>);
    let edit_body = RwSignal::new(String::new());

    let begin_edit = move |a: &AnnouncementDto| {
        edit_channel.set(Some(a.channel_id));
        edit_msg.set(Some(a.id));
        edit_body.set(a.body.clone());
        edit_open.set(true);
    };
    let do_delete = move |cid: ChannelId, mid: MessageId| {
        spawn_local(async move {
            match api::delete(cid, mid).await {
                Ok(()) => {
                    toast.success("Announcement deleted");
                    reload.update(|n| *n += 1);
                }
                Err(e) => toast.error_from(&e),
            }
        });
    };
    let edited = Callback::new(move |()| reload.update(|n| *n += 1));
    let select_wrap = class("width: 220px;");

    view! {
        <Stack gap=Gap::Lg>
            <Cluster gap=Gap::Sm justify="space-between".to_string()>
                <div class=select_wrap>
                    <Select value=channel_value on_change=on_channel>
                        <option value="">"Select a channel…"</option>
                        {move || channels.get().and_then(Result::ok).map(|l| {
                            l.into_iter()
                                .filter(|c| c.kind != ChannelKind::Direct)
                                .map(|c| {
                                    let id = c.id.0.to_string();
                                    view! { <option value=id>{c.title}</option> }
                                })
                                .collect_view()
                        })}
                    </Select>
                </div>
                <Button variant=ButtonVariant::Primary size=ButtonSize::Sm on_click=open_post>
                    <Icon name=IconName::Megaphone size=14 /> " Broadcast"
                </Button>
            </Cluster>

            {move || match items.get() {
                None => note("Loading announcements…"),
                Some(Err(e)) => load_error(&e),
                Some(Ok(list)) if list.is_empty() => view! {
                    <EmptyState icon=IconName::Megaphone title="No announcements" description="Broadcasts to this channel appear here." />
                }.into_any(),
                Some(Ok(list)) => {
                    let cards = list.into_iter().map(|a| {
                        announcement_card(&a, begin_edit, do_delete)
                    }).collect_view();
                    view! { <Stack gap=Gap::Md>{cards}</Stack> }.into_any()
                }
            }}

            <PostAnnouncementDialog open=post_open channel=channel on_posted=posted />
            <EditAnnouncementDialog
                open=edit_open
                channel=edit_channel
                message=edit_msg
                body=edit_body
                on_saved=edited
            />
        </Stack>
    }
}

fn announcement_card(
    a: &AnnouncementDto,
    begin_edit: impl Fn(&AnnouncementDto) + Copy + Send + Sync + 'static,
    do_delete: impl Fn(ChannelId, MessageId) + Copy + Send + Sync + 'static,
) -> AnyView {
    let sender = a.sender.full_name.clone();
    let body = a.body.clone();
    let when = relative_time(a.created_at);
    let edited = a.edited_at.is_some();
    let remaining = a
        .editable
        .then(|| remaining_edit_minutes(a.created_at))
        .flatten();
    let cid = a.channel_id;
    let mid = a.id;

    let a_clone = a.clone();
    let edit_cb = Callback::new(move |_| begin_edit(&a_clone));
    let delete_cb = Callback::new(move |_| do_delete(cid, mid));

    let name_cls = class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    let body_cls = class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c}; line-height: 1.55; white-space: pre-wrap; margin: 0;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT,
    ));

    let controls = match remaining {
        Some(left) => view! {
            <Cluster gap=Gap::Xs>
                <Badge variant=BadgeVariant::Warning>
                    <Icon name=IconName::Clock size=10 /> {format!(" {left}m to edit")}
                </Badge>
                <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm on_click=edit_cb>"Edit"</Button>
                <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm on_click=delete_cb>"Delete"</Button>
            </Cluster>
        }.into_any(),
        None => ().into_any(),
    };

    view! {
        <Card>
            <Stack gap=Gap::Sm>
                <Cluster gap=Gap::Sm justify="space-between".to_string()>
                    <Cluster gap=Gap::Sm>
                        <Avatar name=sender.clone() size=AvatarSize::Sm tone=tone_for(&sender) />
                        <div class=name_cls>{sender}</div>
                        {subtle(&if edited { format!("{when} · edited") } else { when })}
                    </Cluster>
                    {controls}
                </Cluster>
                <p class=body_cls>{body}</p>
            </Stack>
        </Card>
    }
    .into_any()
}

#[component]
fn PostAnnouncementDialog(
    open: RwSignal<bool>,
    #[prop(into)] channel: Signal<Option<ChannelId>>,
    on_posted: Callback<()>,
) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let body = RwSignal::new(String::new());
    let submitting = RwSignal::new(false);

    let on_close = Callback::new(move |()| open.set(false));
    let cancel = Callback::new(move |_| open.set(false));

    let submit = Callback::new(move |_| {
        if submitting.get_untracked() {
            return;
        }
        let Some(cid) = channel.get_untracked() else {
            toast.error("Select a channel to broadcast to.");
            return;
        };
        let b = body.get_untracked();
        if let Err(e) = validate_announcement_body(&b) {
            toast.error(e.to_string());
            return;
        }
        submitting.set(true);
        let req = PostAnnouncementRequest {
            channel_id: cid,
            body: b,
        };
        spawn_local(async move {
            match api::post(&req).await {
                Ok(_) => {
                    toast.success("Announcement broadcast");
                    body.set(String::new());
                    open.set(false);
                    on_posted.run(());
                }
                Err(e) => toast.error_from(&e),
            }
            submitting.set(false);
        });
    });

    view! {
        <Dialog open=open on_close=on_close>
            <DialogHeader title="Broadcast announcement" subtitle="Editable for 15 minutes after posting." />
            <DialogBody>
                <Textarea value=body on_input=Callback::new(move |v| body.set(v)) placeholder="What do you want everyone to know?" rows=5 />
            </DialogBody>
            <DialogFooter>
                <Button variant=ButtonVariant::Ghost on_click=cancel>"Cancel"</Button>
                <Button variant=ButtonVariant::Primary on_click=submit disabled=submitting.get()>
                    {move || if submitting.get() { "Broadcasting…" } else { "Broadcast" }}
                </Button>
            </DialogFooter>
        </Dialog>
    }
}

#[component]
fn EditAnnouncementDialog(
    open: RwSignal<bool>,
    #[prop(into)] channel: Signal<Option<ChannelId>>,
    #[prop(into)] message: Signal<Option<MessageId>>,
    body: RwSignal<String>,
    on_saved: Callback<()>,
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
        if let Err(e) = validate_announcement_body(&b) {
            toast.error(e.to_string());
            return;
        }
        submitting.set(true);
        let req = EditAnnouncementRequest { body: b };
        spawn_local(async move {
            match api::edit(cid, mid, &req).await {
                Ok(_) => {
                    toast.success("Announcement updated");
                    open.set(false);
                    on_saved.run(());
                }
                Err(e) => toast.error_from(&e),
            }
            submitting.set(false);
        });
    });

    view! {
        <Dialog open=open on_close=on_close>
            <DialogHeader title="Edit announcement" subtitle="Only allowed within the grace window." />
            <DialogBody>
                <Textarea value=body on_input=Callback::new(move |v| body.set(v)) rows=5 />
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
