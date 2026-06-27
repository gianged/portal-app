//! Dashboard widgets: the stat-tile row and the Requests/Tickets/Channels panels, each fed a [`Loadable`] signal owned by the dashboard page.

use leptos::prelude::*;

use shared::dto::{
    chat::ChannelSummaryDto,
    group::GroupDto,
    request::{RequestDto, RequestStatus},
    ticket::{TicketDto, TicketStatus},
};

use crate::primitives::avatar::{Avatar, AvatarSize};
use crate::primitives::badge::Badge;
use crate::primitives::card::Card;
use crate::primitives::stack::{Gap, Stack};
use crate::primitives::table::{Table, TableToolbar, TableWrap};
use crate::theme::{self, color, space, typography};
use crate::util::format;
use crate::util::load;

pub use crate::util::load::Loadable;

#[component]
pub fn StatTiles(
    requests: Loadable<Vec<RequestDto>>,
    tickets: Loadable<Vec<TicketDto>>,
    channels: Loadable<Vec<ChannelSummaryDto>>,
    groups: Loadable<Vec<GroupDto>>,
) -> impl IntoView {
    let grid = theme::class(format!(
        "display: grid; grid-template-columns: repeat(4, 1fr); gap: {g}; margin-bottom: {mb};",
        g = space::D4,
        mb = space::D5,
    ));
    view! {
        <div class=grid>
            <StatTile
                label="Assigned requests"
                value=Signal::derive(move || num(opt_count(requests)))
                sub=Signal::derive(move || {
                    sub_label(count_where(requests, |r| r.status == RequestStatus::Review), "in review")
                })
            />
            <StatTile
                label="Open tickets"
                value=Signal::derive(move || num(count_where(tickets, |t| t.status != TicketStatus::Closed)))
                sub=Signal::derive(move || {
                    sub_label(count_where(tickets, |t| t.status == TicketStatus::Resolved), "resolved")
                })
            />
            <StatTile
                label="Channels"
                value=Signal::derive(move || num(opt_count(channels)))
                sub=Signal::derive(move || sub_label(count_where(channels, |c| c.unread), "unread"))
            />
            <StatTile
                label="Groups"
                value=Signal::derive(move || num(opt_count(groups)))
                sub=Signal::derive(move || String::from("across the org"))
            />
        </div>
    }
}

#[component]
fn StatTile(
    #[prop(into)] label: String,
    #[prop(into)] value: Signal<String>,
    #[prop(into)] sub: Signal<String>,
) -> impl IntoView {
    let label_cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; text-transform: uppercase; \
         letter-spacing: 0.08em; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_EYEBROW,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_FAINT,
    ));
    let value_cls = theme::class(format!(
        "font-family: {ff}; font-size: 26px; font-weight: {fw}; \
         letter-spacing: -0.02em; color: {c};",
        ff = typography::FONT_SANS,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    let sub_cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_CAPTION,
        c = color::TEXT_MUTED,
    ));
    let row_cls = theme::class("display: flex; align-items: baseline; gap: 8px;");
    view! {
        <Card>
            <Stack gap=Gap::Md>
                <div class=label_cls>{label}</div>
                <div class=row_cls>
                    <span class=value_cls>{move || value.get()}</span>
                    <span class=sub_cls>{move || sub.get()}</span>
                </div>
            </Stack>
        </Card>
    }
}

#[component]
pub fn RequestsPanel(requests: Loadable<Vec<RequestDto>>) -> impl IntoView {
    view! {
        <TableWrap>
            <TableToolbar>
                {panel_heading("Requests assigned to you")}
                <span></span>
            </TableToolbar>
            {move || match requests.get() {
                None => note("Loading requests…", true),
                Some(Err(e)) => load::load_error(&e),
                Some(Ok(items)) if items.is_empty() => {
                    note("Nothing assigned to you right now.", true)
                }
                Some(Ok(items)) => requests_table(items),
            }}
        </TableWrap>
    }
}

#[component]
pub fn TicketsPanel(tickets: Loadable<Vec<TicketDto>>) -> impl IntoView {
    view! {
        <Card>
            <Stack gap=Gap::Md>
                {panel_heading("My IT tickets")}
                {move || match tickets.get() {
                    None => note("Loading tickets…", false),
                    Some(Err(e)) => load::load_error(&e),
                    Some(Ok(items)) if items.is_empty() => {
                        note("You haven't raised any tickets.", false)
                    }
                    Some(Ok(items)) => tickets_list(items),
                }}
            </Stack>
        </Card>
    }
}

#[component]
pub fn ChannelsPanel(channels: Loadable<Vec<ChannelSummaryDto>>) -> impl IntoView {
    view! {
        <Card>
            <Stack gap=Gap::Md>
                {panel_heading("Channels")}
                {move || match channels.get() {
                    None => note("Loading channels…", false),
                    Some(Err(e)) => load::load_error(&e),
                    Some(Ok(items)) if items.is_empty() => note("No channels yet.", false),
                    Some(Ok(items)) => channels_list(items),
                }}
            </Stack>
        </Card>
    }
}

fn requests_table(items: Vec<RequestDto>) -> AnyView {
    let total = items.len();
    let rows = items.into_iter().take(6).map(request_row).collect_view();
    let footer = (total > 6).then(|| {
        let cls = theme::class(format!(
            "padding: {p}; font-family: {ff}; font-size: {fs}; color: {c};",
            p = space::D3,
            ff = typography::FONT_SANS,
            fs = typography::TEXT_CAPTION,
            c = color::TEXT_FAINT,
        ));
        view! { <div class=cls>{format!("+{} more", total - 6)}</div> }
    });
    view! {
        <Table>
            <thead>
                <tr>
                    <th>"ID"</th>
                    <th>"Title"</th>
                    <th>"Status"</th>
                    <th>"Priority"</th>
                    <th>"Assignee"</th>
                    <th>"Updated"</th>
                </tr>
            </thead>
            <tbody>{rows}</tbody>
        </Table>
        {footer}
    }
    .into_any()
}

fn request_row(r: RequestDto) -> impl IntoView {
    let id = r.id.0.to_string();
    let short = id.get(..8).unwrap_or(id.as_str()).to_owned();
    let title = r.title.clone();
    let status = r.status;
    let priority = r.priority;
    let assignee = r.assignee.as_ref().map(|a| a.full_name.clone());
    let updated = format::relative_time(r.updated_at);
    view! {
        <tr>
            <td><span class="mono cell-muted">{format!("#{short}")}</span></td>
            <td><span class="cell-strong">{title}</span></td>
            <td><Badge variant=format::request_status_variant(status)>{status.label()}</Badge></td>
            <td><Badge variant=format::request_priority_variant(priority)>{priority.label()}</Badge></td>
            <td>{match assignee {
                Some(name) => assignee_cell(&name),
                None => dash(),
            }}</td>
            <td><span class="cell-muted">{updated}</span></td>
        </tr>
    }
}

fn tickets_list(items: Vec<TicketDto>) -> AnyView {
    let rows = items.into_iter().take(6).map(ticket_row).collect_view();
    view! { <Stack gap=Gap::Sm>{rows}</Stack> }.into_any()
}

fn ticket_row(t: TicketDto) -> impl IntoView {
    let row = theme::class(format!(
        "display: flex; align-items: center; gap: {g};",
        g = space::D3
    ));
    let title = theme::class(format!(
        "flex: 1; min-width: 0; font-family: {ff}; font-size: {fs}; color: {c}; \
         white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT,
    ));
    let age = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c}; flex-shrink: 0;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_CAPTION,
        c = color::TEXT_FAINT,
    ));
    let label = t.title.clone();
    let status = t.status;
    let created = format::relative_time(t.created_at);
    view! {
        <div class=row>
            <span class=title>{label}</span>
            <Badge variant=format::ticket_status_variant(status)>{status.label()}</Badge>
            <span class=age>{created}</span>
        </div>
    }
}

fn channels_list(items: Vec<ChannelSummaryDto>) -> AnyView {
    let rows = items.into_iter().take(6).map(channel_row).collect_view();
    view! { <Stack gap=Gap::Sm>{rows}</Stack> }.into_any()
}

fn channel_row(c: ChannelSummaryDto) -> impl IntoView {
    let row = theme::class(format!(
        "display: flex; align-items: center; gap: {g};",
        g = space::D3
    ));
    let dot = theme::class(format!(
        "width: 7px; height: 7px; border-radius: 50%; background: {c}; flex-shrink: 0;",
        c = color::ACCENT,
    ));
    let spacer = theme::class("width: 7px; flex-shrink: 0;");
    let title = theme::class(format!(
        "flex: 1; min-width: 0; font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c}; \
         white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        fw = typography::WEIGHT_MEDIUM,
        c = color::TEXT,
    ));
    let age = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c}; flex-shrink: 0;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_CAPTION,
        c = color::TEXT_FAINT,
    ));
    let unread = c.unread;
    let name = c.title.clone();
    let when = c.last_message_at.map(format::relative_time).unwrap_or_default();
    view! {
        <div class=row>
            {if unread {
                view! { <span class=dot></span> }.into_any()
            } else {
                view! { <span class=spacer></span> }.into_any()
            }}
            <span class=title>{name}</span>
            <span class=age>{when}</span>
        </div>
    }
}

fn assignee_cell(name: &str) -> AnyView {
    let wrap = theme::class(format!(
        "display: inline-flex; align-items: center; gap: {g};",
        g = space::D2
    ));
    view! {
        <span class=wrap>
            <Avatar name=name.to_owned() size=AvatarSize::Sm tone=format::tone_for(name) />
            <span class="cell-strong">{name.to_owned()}</span>
        </span>
    }
    .into_any()
}

fn dash() -> AnyView {
    view! { <span class="cell-muted">"—"</span> }.into_any()
}

fn panel_heading(text: &str) -> AnyView {
    let cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_BODY,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    view! { <span class=cls>{text.to_owned()}</span> }.into_any()
}

fn note(text: &str, padded: bool) -> AnyView {
    let pad = if padded { space::D5 } else { "0px" };
    let cls = theme::class(format!(
        "padding: {pad}; font-family: {ff}; font-size: {fs}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT_MUTED,
    ));
    view! { <div class=cls>{text.to_owned()}</div> }.into_any()
}

fn count_where<T: Send + Sync + 'static>(
    sig: Loadable<Vec<T>>,
    pred: impl Fn(&T) -> bool,
) -> Option<usize> {
    sig.with(|opt| {
        opt.as_ref()
            .and_then(|r| r.as_ref().ok())
            .map(|v| v.iter().filter(|x| pred(x)).count())
    })
}

fn opt_count<T: Send + Sync + 'static>(sig: Loadable<Vec<T>>) -> Option<usize> {
    count_where(sig, |_| true)
}

fn num(n: Option<usize>) -> String {
    n.map_or_else(|| "—".to_owned(), |v| v.to_string())
}

fn sub_label(n: Option<usize>, suffix: &str) -> String {
    n.map_or_else(String::new, |v| format!("{v} {suffix}"))
}
