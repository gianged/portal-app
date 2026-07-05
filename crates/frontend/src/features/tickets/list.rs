//! IT-ticket index: a scope-filtered table (Mine / Assigned / Triage queue) with a raise dialog.

use leptos::{prelude::*, task};
use leptos_router::components::A;
use uuid::Uuid;

use shared::dto::ticket::{RaiseTicketRequest, TicketCategory, TicketDto};
use shared::validation::ticket;

use crate::features::tickets::api::{self, Scope};
use crate::primitives::avatar::{Avatar, AvatarSize};
use crate::primitives::badge::Badge;
use crate::primitives::button::{Button, ButtonSize, ButtonVariant};
use crate::primitives::cluster::Cluster;
use crate::primitives::dialog::{Dialog, DialogBody, DialogFooter, DialogHeader};
use crate::primitives::empty_state::EmptyState;
use crate::primitives::icon::{Icon, IconName};
use crate::primitives::input::{FieldError, FieldLabel, Input};
use crate::primitives::segmented::{Segmented, SegmentedItem};
use crate::primitives::select::Select;
use crate::primitives::stack::{Gap, Stack};
use crate::primitives::table::{Table, TableToolbar, TableWrap};
use crate::primitives::textarea::Textarea;
use crate::state::toast::ToastState;
use crate::theme::{self, color, space, typography};
use crate::util::debounce;
use crate::util::format;
use crate::util::load::{self, Loadable};

fn category_wire(c: TicketCategory) -> &'static str {
    match c {
        TicketCategory::Hardware => "hardware",
        TicketCategory::Software => "software",
        TicketCategory::Access => "access",
        TicketCategory::Other => "other",
    }
}

fn category_from_wire(s: &str) -> TicketCategory {
    match s {
        "hardware" => TicketCategory::Hardware,
        "software" => TicketCategory::Software,
        "access" => TicketCategory::Access,
        _ => TicketCategory::Other,
    }
}

fn short_id(id: &Uuid) -> String {
    let s = id.to_string();
    format!("#{}", s.get(..8).unwrap_or(&s))
}

#[component]
pub fn TicketsIndex() -> impl IntoView {
    let scope = RwSignal::new(Scope::Mine);
    let items: Loadable<Vec<TicketDto>> = RwSignal::new(None);
    let reload = RwSignal::new(0u32);
    let raise_open = RwSignal::new(false);
    let search = RwSignal::new(String::new());
    let dq = debounce::debounced(search.into(), 300);

    Effect::new(move |_| {
        let _ = reload.get();
        let term = dq.get().trim().to_owned();
        load::load(
            items,
            api::list(scope.get(), (!term.is_empty()).then_some(term)),
        );
    });

    let raised = Callback::new(move |()| reload.update(|n| *n += 1));
    let open_raise = Callback::new(move |_| raise_open.set(true));
    let search_wrap = theme::class("width: 220px;");

    let seg = move |label: &'static str, s: Scope| {
        let active = Signal::derive(move || scope.get() == s);
        let on_click = Callback::new(move |_| scope.set(s));
        view! { <SegmentedItem active=active on_click=on_click>{label}</SegmentedItem> }
    };

    view! {
        <Stack gap=Gap::Lg>
            <TableWrap>
                <TableToolbar>
                    <Segmented>
                        {seg("Mine", Scope::Mine)}
                        {seg("Assigned to me", Scope::Assigned)}
                        {seg("Triage queue", Scope::Triage)}
                    </Segmented>
                    <Cluster gap=Gap::Sm>
                        <div class=search_wrap>
                            <Input value=search on_input=Callback::new(move |v| search.set(v)) placeholder="Search tickets…" />
                        </div>
                        <Button variant=ButtonVariant::Primary size=ButtonSize::Sm on_click=open_raise>
                            <Icon name=IconName::Plus size=14 /> " Raise ticket"
                        </Button>
                    </Cluster>
                </TableToolbar>
                {move || match items.get() {
                    None => load::note("Loading tickets…"),
                    Some(Err(e)) => load::load_error(&e),
                    Some(Ok(list)) if list.is_empty() => view! {
                        <EmptyState
                            icon=IconName::Ticket
                            title="No tickets here"
                            description="Tickets in this view will appear here."
                        />
                    }.into_any(),
                    Some(Ok(list)) => tickets_table(list),
                }}
            </TableWrap>
            <RaiseTicketDialog open=raise_open on_raised=raised />
        </Stack>
    }
}

fn tickets_table(items: Vec<TicketDto>) -> AnyView {
    view! {
        <Table>
            <thead>
                <tr>
                    <th>"ID"</th>
                    <th>"Title"</th>
                    <th>"Status"</th>
                    <th>"Priority"</th>
                    <th>"Requester"</th>
                    <th>"Updated"</th>
                </tr>
            </thead>
            <tbody>
                {items.iter().map(ticket_row).collect_view()}
            </tbody>
        </Table>
    }
    .into_any()
}

fn ticket_row(t: &TicketDto) -> AnyView {
    let href = format!("/tickets/{}", t.id.0);
    let id_label = short_id(&t.id.0);
    let title = t.title.clone();
    let status = t.status;
    let priority = t.priority;
    let requester = t.requester.full_name.clone();
    let updated = format::relative_time(t.updated_at);
    let link_cls = theme::class(format!(
        "color: {c}; font-weight: {fw}; text-decoration: none; &:hover {{ color: {a}; }}",
        c = color::TEXT_STRONG,
        fw = typography::WEIGHT_MEDIUM,
        a = color::ACCENT,
    ));
    let wrap = theme::class(format!(
        "display: inline-flex; align-items: center; gap: {g};",
        g = space::D2
    ));
    view! {
        <tr>
            <td><span class="mono cell-muted">{id_label}</span></td>
            <td><A href=href attr:class=link_cls>{title}</A></td>
            <td><Badge variant=format::ticket_status_variant(status)>{status.label()}</Badge></td>
            <td>{match priority {
                Some(p) => view! { <Badge variant=format::ticket_priority_variant(p)>{p.label()}</Badge> }.into_any(),
                None => view! { <span class="cell-muted">"—"</span> }.into_any(),
            }}</td>
            <td>
                <span class=wrap>
                    <Avatar name=requester.clone() size=AvatarSize::Sm tone=format::tone_for(&requester) />
                    <span class="cell-strong">{requester}</span>
                </span>
            </td>
            <td><span class="cell-muted">{updated}</span></td>
        </tr>
    }
    .into_any()
}

#[component]
fn RaiseTicketDialog(open: RwSignal<bool>, on_raised: Callback<()>) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let title = RwSignal::new(String::new());
    let description = RwSignal::new(String::new());
    let category = RwSignal::new(TicketCategory::Other);
    let title_err = RwSignal::new(None::<String>);
    let desc_err = RwSignal::new(None::<String>);
    let submitting = RwSignal::new(false);

    let on_close = Callback::new(move |()| open.set(false));
    let cancel = Callback::new(move |_| open.set(false));
    let on_category = Callback::new(move |v: String| category.set(category_from_wire(&v)));
    let category_value = Signal::derive(move || category_wire(category.get()).to_owned());

    let submit = Callback::new(move |_| {
        if submitting.get_untracked() {
            return;
        }
        title_err.set(None);
        desc_err.set(None);
        let t = title.get_untracked();
        let d = description.get_untracked();
        let mut ok = true;
        if let Err(e) = ticket::validate_ticket_title(&t) {
            title_err.set(Some(e.to_string()));
            ok = false;
        }
        if let Err(e) = ticket::validate_ticket_description(&d) {
            desc_err.set(Some(e.to_string()));
            ok = false;
        }
        if !ok {
            return;
        }
        submitting.set(true);
        let req = RaiseTicketRequest {
            title: t,
            description: d,
            category: category.get_untracked(),
        };
        task::spawn_local(async move {
            match api::raise(&req).await {
                Ok(_) => {
                    toast.success("Ticket raised");
                    title.set(String::new());
                    description.set(String::new());
                    category.set(TicketCategory::Other);
                    open.set(false);
                    on_raised.run(());
                }
                Err(e) => toast.error_from(&e),
            }
            submitting.set(false);
        });
    });

    view! {
        <Dialog open=open on_close=on_close>
            <DialogHeader title="Raise an IT ticket" subtitle="Describe the problem; IT will triage it." />
            <DialogBody>
                <Stack gap=Gap::Lg>
                    <div>
                        <FieldLabel for_id="tk-title">"Title"</FieldLabel>
                        <Input value=title on_input=Callback::new(move |v| title.set(v)) placeholder="Short summary" />
                        {move || title_err.get().map(|m| view! { <FieldError message=m /> })}
                    </div>
                    <div>
                        <FieldLabel for_id="tk-desc">"Description"</FieldLabel>
                        <Textarea value=description on_input=Callback::new(move |v| description.set(v)) placeholder="What's happening?" />
                        {move || desc_err.get().map(|m| view! { <FieldError message=m /> })}
                    </div>
                    <div>
                        <FieldLabel for_id="tk-cat">"Category"</FieldLabel>
                        <Select value=category_value on_change=on_category>
                            <option value="hardware">"Hardware"</option>
                            <option value="software">"Software"</option>
                            <option value="access">"Access"</option>
                            <option value="other">"Other"</option>
                        </Select>
                    </div>
                </Stack>
            </DialogBody>
            <DialogFooter>
                <Button variant=ButtonVariant::Ghost on_click=cancel>"Cancel"</Button>
                <Button variant=ButtonVariant::Primary on_click=submit disabled=Signal::derive(move || submitting.get())>
                    {move || if submitting.get() { "Raising…" } else { "Raise ticket" }}
                </Button>
            </DialogFooter>
        </Dialog>
    }
}
