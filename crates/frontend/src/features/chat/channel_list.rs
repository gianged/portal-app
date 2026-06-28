//! Chat left rail: the caller's channels with unread dots, plus a New DM action that opens (or finds) a direct channel and selects it.

use leptos::{prelude::*, task};

use shared::dto::chat::{ChannelDto, ChannelKind, ChannelSummaryDto};
use shared::dto::ids::{ChannelId, UserId};

use crate::features::chat::api;
use crate::features::ui;
use crate::features::users::picker::UserPicker;
use crate::primitives::button::{Button, ButtonSize, ButtonVariant};
use crate::primitives::dialog::{Dialog, DialogBody, DialogFooter, DialogHeader};
use crate::primitives::icon::{Icon, IconName};
use crate::primitives::stack::{Gap, Stack};
use crate::state::toast::ToastState;
use crate::theme::{self, color, radius, space, typography};
use crate::util::load::{self, Loadable};

fn channel_id(c: &ChannelDto) -> ChannelId {
    match c {
        ChannelDto::Group { id, .. }
        | ChannelDto::General { id }
        | ChannelDto::Direct { id, .. } => *id,
    }
}

fn kind_icon(kind: ChannelKind) -> IconName {
    match kind {
        ChannelKind::Group => IconName::Users,
        ChannelKind::General => IconName::Megaphone,
        ChannelKind::Direct => IconName::Chat,
    }
}

#[component]
pub fn ChannelList(selected: RwSignal<Option<ChannelId>>) -> impl IntoView {
    let channels: Loadable<Vec<ChannelSummaryDto>> = RwSignal::new(None);
    let reload = RwSignal::new(0u32);
    let dm_open = RwSignal::new(false);

    Effect::new(move |_| {
        let _ = reload.get();
        load::load(channels, api::channels());
    });

    // Auto-select the first channel once the list arrives.
    Effect::new(move |_| {
        if selected.get_untracked().is_some() {
            return;
        }
        if let Some(Ok(list)) = channels.get()
            && let Some(first) = list.first()
        {
            selected.set(Some(first.id));
        }
    });

    let open_dm = Callback::new(move |_| dm_open.set(true));
    let on_opened = Callback::new(move |cid: ChannelId| {
        selected.set(Some(cid));
        reload.update(|n| *n += 1);
    });

    let header = theme::class(format!(
        "display: flex; align-items: center; justify-content: space-between; padding: {p};",
        p = space::D3,
    ));
    let list_cls = theme::class(format!(
        "display: flex; flex-direction: column; gap: 1px; padding: 0 {p};",
        p = space::D2
    ));

    view! {
        <Stack gap=Gap::Xs>
            <div class=header>
                {ui::section_heading("Channels")}
                <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm on_click=open_dm>
                    <Icon name=IconName::Plus size=14 /> " DM"
                </Button>
            </div>
            {move || match channels.get() {
                None => load::note("Loading…"),
                Some(Err(e)) => load::load_error(&e),
                Some(Ok(list)) if list.is_empty() => load::note("No channels yet."),
                Some(Ok(list)) => {
                    let rows = list.into_iter().map(|c| channel_row(c, selected)).collect_view();
                    view! { <div class=list_cls.clone()>{rows}</div> }.into_any()
                }
            }}
            <NewDmDialog open=dm_open on_opened=on_opened />
        </Stack>
    }
}

fn channel_row(c: ChannelSummaryDto, selected: RwSignal<Option<ChannelId>>) -> impl IntoView {
    let cid = c.id;
    let title = c.title.clone();
    let unread = c.unread;
    let icon = kind_icon(c.kind);

    let base = format!(
        "display: flex; align-items: center; gap: {g}; padding: 6px {p}; height: 34px; \
         border-radius: {r}; cursor: pointer; color: {c}; font-family: {ff}; font-size: {fs}; \
         font-weight: {fw}; transition: background 120ms ease; ",
        g = space::D2,
        p = space::D3,
        r = radius::SM,
        c = color::TEXT,
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        fw = typography::WEIGHT_MEDIUM,
    );
    let hover = format!("&:hover {{ background: {bh}; }}", bh = color::BG_HOVER);
    let normal = theme::class(format!("{base}{hover}"));
    let active = theme::class(format!(
        "{base} background: {bg}; color: {a}; &:hover {{ background: {bg}; }}",
        bg = color::ACCENT_BG,
        a = color::ACCENT,
    ));

    let grow = theme::class(
        "flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
    );
    let dot = theme::class(format!(
        "width: 7px; height: 7px; border-radius: 50%; background: {a}; flex-shrink: 0;",
        a = color::ACCENT,
    ));

    let cls = move || {
        if selected.get() == Some(cid) {
            active.clone()
        } else {
            normal.clone()
        }
    };
    let on_click = move |_| selected.set(Some(cid));

    view! {
        <div class=cls on:click=on_click>
            <Icon name=icon size=15 />
            <span class=grow>{title}</span>
            {if unread { view! { <span class=dot></span> }.into_any() } else { ().into_any() }}
        </div>
    }
}

#[component]
fn NewDmDialog(open: RwSignal<bool>, on_opened: Callback<ChannelId>) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let target = RwSignal::new(None::<UserId>);
    let on_close = Callback::new(move |()| open.set(false));
    let cancel = Callback::new(move |_| open.set(false));
    let on_select = Callback::new(move |u: UserId| target.set(Some(u)));

    let confirm = Callback::new(move |_| {
        let Some(uid) = target.get_untracked() else {
            toast.error("Pick someone to message.");
            return;
        };
        open.set(false);
        task::spawn_local(async move {
            match api::open_direct(uid).await {
                Ok(channel) => {
                    target.set(None);
                    on_opened.run(channel_id(&channel));
                }
                Err(e) => toast.error_from(&e),
            }
        });
    });

    view! {
        <Dialog open=open on_close=on_close>
            <DialogHeader title="New direct message" subtitle="Start a private conversation." />
            <DialogBody>
                <UserPicker selected=target on_select=on_select />
            </DialogBody>
            <DialogFooter>
                <Button variant=ButtonVariant::Ghost on_click=cancel>"Cancel"</Button>
                <Button variant=ButtonVariant::Primary on_click=confirm>"Open"</Button>
            </DialogFooter>
        </Dialog>
    }
}
