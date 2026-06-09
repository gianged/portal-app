//! Inbox UI: the notification list with an unread filter, mark-read on click,
//! mark-all, and navigation to the referenced request/ticket/project/etc.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::NavigateOptions;
use leptos_router::hooks::use_navigate;

use shared::dto::notification::{NotificationDto, NotificationPayloadDto};

use crate::features::notifications::api;
use crate::features::ui::{section_heading, subtle};
use crate::primitives::button::{Button, ButtonSize, ButtonVariant};
use crate::primitives::card::Card;
use crate::primitives::cluster::Cluster;
use crate::primitives::empty_state::EmptyState;
use crate::primitives::icon::{Icon, IconName};
use crate::primitives::segmented::{Segmented, SegmentedItem};
use crate::primitives::stack::{Gap, Stack};
use crate::state::notifications::NotificationsState;
use crate::state::toast::ToastState;
use crate::theme::{class, color, radius, space, typography};
use crate::util::format::relative_time;
use crate::util::load::{Loadable, load, load_error, note};

fn payload_icon(p: &NotificationPayloadDto) -> IconName {
    match p {
        NotificationPayloadDto::Announcement { .. } => IconName::Megaphone,
        NotificationPayloadDto::Mention { .. } => IconName::Chat,
        NotificationPayloadDto::TicketUrgent { .. }
        | NotificationPayloadDto::TicketAssigned { .. }
        | NotificationPayloadDto::TicketStatusChange { .. }
        | NotificationPayloadDto::TicketRaised { .. } => IconName::Ticket,
        NotificationPayloadDto::RequestAssigned { .. }
        | NotificationPayloadDto::RequestStatusChange { .. } => IconName::Doc,
        NotificationPayloadDto::ProjectInvite { .. }
        | NotificationPayloadDto::ProjectInviteResponse { .. } => IconName::Folder,
        NotificationPayloadDto::System { .. } => IconName::AlertCircle,
    }
}

fn payload_href(p: &NotificationPayloadDto) -> Option<String> {
    match p {
        NotificationPayloadDto::Announcement { .. } => Some("/announcements".to_owned()),
        NotificationPayloadDto::Mention { .. } => Some("/chat".to_owned()),
        NotificationPayloadDto::TicketUrgent { ticket_id }
        | NotificationPayloadDto::TicketAssigned { ticket_id }
        | NotificationPayloadDto::TicketStatusChange { ticket_id, .. }
        | NotificationPayloadDto::TicketRaised { ticket_id } => {
            Some(format!("/tickets/{}", ticket_id.0))
        }
        NotificationPayloadDto::RequestAssigned { request_id }
        | NotificationPayloadDto::RequestStatusChange { request_id, .. } => {
            Some(format!("/requests/{}", request_id.0))
        }
        NotificationPayloadDto::ProjectInvite { project_id, .. }
        | NotificationPayloadDto::ProjectInviteResponse { project_id, .. } => {
            Some(format!("/projects/{}", project_id.0))
        }
        NotificationPayloadDto::System { .. } => None,
    }
}

fn payload_summary(p: &NotificationPayloadDto) -> String {
    match p {
        NotificationPayloadDto::Announcement { .. } => "New announcement".to_owned(),
        NotificationPayloadDto::Mention { .. } => "You were mentioned".to_owned(),
        NotificationPayloadDto::TicketUrgent { .. } => {
            "An urgent ticket needs attention".to_owned()
        }
        NotificationPayloadDto::RequestAssigned { .. } => {
            "A request was assigned to you".to_owned()
        }
        NotificationPayloadDto::RequestStatusChange { from, to, .. } => {
            format!("Request moved from {} to {}", from.label(), to.label())
        }
        NotificationPayloadDto::ProjectInvite { .. } => {
            "Your group was invited to a project".to_owned()
        }
        NotificationPayloadDto::TicketAssigned { .. } => "A ticket was assigned to you".to_owned(),
        NotificationPayloadDto::TicketStatusChange { from, to, .. } => {
            format!("Ticket moved from {} to {}", from.label(), to.label())
        }
        NotificationPayloadDto::ProjectInviteResponse { status, .. } => {
            format!("Project invite {}", status.label().to_lowercase())
        }
        NotificationPayloadDto::TicketRaised { .. } => "A new ticket was raised".to_owned(),
        NotificationPayloadDto::System { message } => message.clone(),
    }
}

#[component]
pub fn InboxIndex() -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let notifications = use_context::<NotificationsState>().expect("NotificationsState context");
    let navigate = use_navigate();

    let unread_only = RwSignal::new(false);
    let items: Loadable<Vec<NotificationDto>> = RwSignal::new(None);
    let reload = RwSignal::new(0u32);

    Effect::new(move |_| {
        let _ = reload.get();
        load(items, api::list(unread_only.get(), 50));
    });

    // Pull a fresh unread count into the topbar badge after a mutation.
    let refresh_badge = move || {
        spawn_local(async move {
            if let Ok(c) = api::unread_count().await {
                notifications.set_unread(c);
            }
        });
    };

    let mark_all = Callback::new(move |_| {
        spawn_local(async move {
            match api::mark_read(Vec::new()).await {
                Ok(()) => {
                    toast.success("All caught up");
                    reload.update(|n| *n += 1);
                }
                Err(e) => toast.error_from(&e),
            }
        });
        refresh_badge();
    });

    let seg = move |label: &'static str, val: bool| {
        let active = Signal::derive(move || unread_only.get() == val);
        let on_click = Callback::new(move |_| unread_only.set(val));
        view! { <SegmentedItem active=active on_click=on_click>{label}</SegmentedItem> }
    };

    view! {
        <Stack gap=Gap::Lg>
            <Cluster gap=Gap::Sm justify="space-between".to_string()>
                <Segmented>
                    {seg("All", false)}
                    {seg("Unread", true)}
                </Segmented>
                <Button variant=ButtonVariant::Secondary size=ButtonSize::Sm on_click=mark_all>
                    "Mark all read"
                </Button>
            </Cluster>
            <Card>
                <Stack gap=Gap::Sm>
                    {section_heading("Inbox")}
                    {move || match items.get() {
                        None => note("Loading…"),
                        Some(Err(e)) => load_error(&e),
                        Some(Ok(list)) if list.is_empty() => view! {
                            <EmptyState icon=IconName::Inbox title="Inbox zero" description="You have no notifications." />
                        }.into_any(),
                        Some(Ok(list)) => {
                            let rows = list.into_iter().map(|n| {
                                notification_row(n, navigate.clone(), reload, notifications)
                            }).collect_view();
                            view! { <div>{rows}</div> }.into_any()
                        }
                    }}
                </Stack>
            </Card>
        </Stack>
    }
}

fn notification_row(
    n: NotificationDto,
    navigate: impl Fn(&str, NavigateOptions) + Clone + 'static,
    reload: RwSignal<u32>,
    notifications: NotificationsState,
) -> AnyView {
    let icon = payload_icon(&n.payload);
    let summary = payload_summary(&n.payload);
    let href = payload_href(&n.payload);
    let when = relative_time(n.created_at);
    let read = n.read;
    let id = n.id;

    let row = class(format!(
        "display: flex; align-items: center; gap: {g}; padding: {p}; border-radius: {r}; \
         cursor: pointer; transition: background 120ms ease; &:hover {{ background: {bh}; }}",
        g = space::D3,
        p = space::D3,
        r = radius::SM,
        bh = color::BG_HOVER,
    ));
    let icon_wrap = class(format!(
        "display: inline-flex; align-items: center; justify-content: center; width: 32px; height: 32px; \
         border-radius: 50%; background: {bg}; color: {c}; flex-shrink: 0;",
        bg = color::BG_SUNKEN,
        c = color::TEXT_MUTED,
    ));
    let body = class("flex: 1; min-width: 0;");
    let summary_cls = class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        fw = if read {
            typography::WEIGHT_REGULAR
        } else {
            typography::WEIGHT_MEDIUM
        },
        c = color::TEXT,
    ));
    let dot = class(format!(
        "width: 8px; height: 8px; border-radius: 50%; background: {a}; flex-shrink: 0;",
        a = color::ACCENT,
    ));
    let spacer = class("width: 8px; flex-shrink: 0;");

    let on_click = move |_| {
        let navigate = navigate.clone();
        let href = href.clone();
        spawn_local(async move {
            let _ = api::mark_read(vec![id]).await;
            if let Ok(c) = api::unread_count().await {
                notifications.set_unread(c);
            }
            reload.update(|n| *n += 1);
            if let Some(h) = href {
                navigate(&h, NavigateOptions::default());
            }
        });
    };

    view! {
        <div class=row on:click=on_click>
            {if read {
                view! { <span class=spacer></span> }.into_any()
            } else {
                view! { <span class=dot></span> }.into_any()
            }}
            <span class=icon_wrap><Icon name=icon size=16 /></span>
            <div class=body>
                <div class=summary_cls>{summary}</div>
                {subtle(&when)}
            </div>
        </div>
    }
    .into_any()
}
