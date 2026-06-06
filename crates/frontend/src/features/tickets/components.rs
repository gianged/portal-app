//! IT-ticket UI: a scope-filtered index with a raise dialog, and the detail view
//! with status-gated lifecycle actions, triage (set priority), and assignment.

use futures::FutureExt;
use futures::future::LocalBoxFuture;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use uuid::Uuid;

use shared::dto::ids::{TicketId, UserId};
use shared::dto::ticket::{
    AssignTicketRequest, RaiseTicketRequest, TicketCategory, TicketDto, TicketPriority,
    TicketStatus, TriageTicketRequest,
};
use shared::validation::ticket::{validate_ticket_description, validate_ticket_title};

use crate::api::error::FrontendError;
use crate::features::tickets::api::{self, Scope};
use crate::features::ui::{back_link, page_title, subtle};
use crate::features::users::components::UserPicker;
use crate::primitives::avatar::{Avatar, AvatarSize};
use crate::primitives::badge::Badge;
use crate::primitives::button::{Button, ButtonSize, ButtonVariant};
use crate::primitives::card::Card;
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
use crate::theme::{class, color, space, typography};
use crate::util::format::{
    relative_time, ticket_priority_variant, ticket_status_variant, tone_for,
};
use crate::util::load::{Loadable, load, note};

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

fn priority_wire(p: TicketPriority) -> &'static str {
    match p {
        TicketPriority::Low => "low",
        TicketPriority::Normal => "normal",
        TicketPriority::High => "high",
        TicketPriority::Urgent => "urgent",
    }
}

fn priority_from_wire(s: &str) -> TicketPriority {
    match s {
        "low" => TicketPriority::Low,
        "high" => TicketPriority::High,
        "urgent" => TicketPriority::Urgent,
        _ => TicketPriority::Normal,
    }
}

fn short_id(id: &Uuid) -> String {
    let s = id.to_string();
    format!("#{}", s.get(..8).unwrap_or(&s))
}

// ─────────────────────────── Index ───────────────────────────

#[component]
pub fn TicketsIndex() -> impl IntoView {
    let scope = RwSignal::new(Scope::Mine);
    let items: Loadable<Vec<TicketDto>> = RwSignal::new(None);
    let reload = RwSignal::new(0u32);
    let raise_open = RwSignal::new(false);

    Effect::new(move |_| {
        let _ = reload.get();
        load(items, api::list(scope.get()));
    });

    let raised = Callback::new(move |()| reload.update(|n| *n += 1));
    let open_raise = Callback::new(move |_| raise_open.set(true));

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
                    <Button variant=ButtonVariant::Primary size=ButtonSize::Sm on_click=open_raise>
                        <Icon name=IconName::Plus size=14 /> " Raise ticket"
                    </Button>
                </TableToolbar>
                {move || match items.get() {
                    None => note("Loading tickets…", false),
                    Some(Err(e)) => note(&format!("Couldn't load tickets: {e}"), true),
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
                {items.into_iter().map(ticket_row).collect_view()}
            </tbody>
        </Table>
    }
    .into_any()
}

fn ticket_row(t: TicketDto) -> impl IntoView {
    let href = format!("/tickets/{}", t.id.0);
    let id_label = short_id(&t.id.0);
    let title = t.title.clone();
    let status = t.status;
    let priority = t.priority;
    let requester = t.requester.full_name.clone();
    let updated = relative_time(t.updated_at);
    let link_cls = class(format!(
        "color: {c}; font-weight: {fw}; text-decoration: none; &:hover {{ color: {a}; }}",
        c = color::TEXT_STRONG,
        fw = typography::WEIGHT_MEDIUM,
        a = color::ACCENT,
    ));
    let wrap = class(format!(
        "display: inline-flex; align-items: center; gap: {g};",
        g = space::D2
    ));
    view! {
        <tr>
            <td><span class="mono cell-muted">{id_label}</span></td>
            <td><A href=href attr:class=link_cls>{title}</A></td>
            <td><Badge variant=ticket_status_variant(status)>{status.label()}</Badge></td>
            <td>{match priority {
                Some(p) => view! { <Badge variant=ticket_priority_variant(p)>{p.label()}</Badge> }.into_any(),
                None => view! { <span class="cell-muted">"—"</span> }.into_any(),
            }}</td>
            <td>
                <span class=wrap>
                    <Avatar name=requester.clone() size=AvatarSize::Sm tone=tone_for(&requester) />
                    <span class="cell-strong">{requester}</span>
                </span>
            </td>
            <td><span class="cell-muted">{updated}</span></td>
        </tr>
    }
}

// ─────────────────────────── Raise dialog ───────────────────────────

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
        if let Err(e) = validate_ticket_title(&t) {
            title_err.set(Some(e.to_string()));
            ok = false;
        }
        if let Err(e) = validate_ticket_description(&d) {
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
        spawn_local(async move {
            match api::raise(&req).await {
                Ok(_) => {
                    toast.success("Ticket raised");
                    title.set(String::new());
                    description.set(String::new());
                    category.set(TicketCategory::Other);
                    open.set(false);
                    on_raised.run(());
                }
                Err(e) => toast.error(e.to_string()),
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
                <Button variant=ButtonVariant::Primary on_click=submit disabled=submitting.get()>
                    {move || if submitting.get() { "Raising…" } else { "Raise ticket" }}
                </Button>
            </DialogFooter>
        </Dialog>
    }
}

// ─────────────────────────── Detail ───────────────────────────

#[derive(Clone, Copy)]
enum TicketAction {
    Start,
    Resolve,
    Reject,
    Close,
    Reopen,
}

fn action_future(
    action: TicketAction,
    id: TicketId,
) -> LocalBoxFuture<'static, Result<TicketDto, FrontendError>> {
    match action {
        TicketAction::Start => api::start(id).boxed_local(),
        TicketAction::Resolve => api::resolve(id).boxed_local(),
        TicketAction::Reject => api::reject(id).boxed_local(),
        TicketAction::Close => api::close(id).boxed_local(),
        TicketAction::Reopen => api::reopen(id).boxed_local(),
    }
}

#[component]
pub fn TicketDetail(#[prop(into)] id: Signal<Option<TicketId>>) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let detail: Loadable<TicketDto> = RwSignal::new(None);
    let reload = RwSignal::new(0u32);
    let triage_open = RwSignal::new(false);
    let assign_open = RwSignal::new(false);
    let assign_target = RwSignal::new(None::<UserId>);
    let triage_priority = RwSignal::new(TicketPriority::Normal);

    Effect::new(move |_| {
        let _ = reload.get();
        if let Some(tid) = id.get() {
            load(detail, api::get(tid));
        }
    });

    let run = move |action: TicketAction| {
        let Some(tid) = id.get_untracked() else {
            return;
        };
        spawn_local(async move {
            match action_future(action, tid).await {
                Ok(_) => {
                    toast.success("Ticket updated");
                    reload.update(|n| *n += 1);
                }
                Err(e) => toast.error(e.to_string()),
            }
        });
    };

    let confirm_triage = Callback::new(move |()| {
        let Some(tid) = id.get_untracked() else {
            return;
        };
        triage_open.set(false);
        let req = TriageTicketRequest {
            priority: triage_priority.get_untracked(),
        };
        spawn_local(async move {
            match api::triage(tid, &req).await {
                Ok(_) => {
                    toast.success("Ticket triaged");
                    reload.update(|n| *n += 1);
                }
                Err(e) => toast.error(e.to_string()),
            }
        });
    });

    let confirm_assign = Callback::new(move |()| {
        let Some(uid) = assign_target.get_untracked() else {
            toast.error("Pick someone to assign.");
            return;
        };
        let Some(tid) = id.get_untracked() else {
            return;
        };
        assign_open.set(false);
        spawn_local(async move {
            let req = AssignTicketRequest {
                assignee_user_id: uid,
            };
            match api::assign(tid, &req).await {
                Ok(_) => {
                    toast.success("Ticket assigned");
                    reload.update(|n| *n += 1);
                }
                Err(e) => toast.error(e.to_string()),
            }
        });
    });

    let open_triage = Callback::new(move |_| triage_open.set(true));
    let open_assign = Callback::new(move |_| assign_open.set(true));

    view! {
        <Stack gap=Gap::Lg>
            {back_link("/tickets", "Back to tickets")}
            {move || match detail.get() {
                None => note("Loading ticket…", false),
                Some(Err(e)) => note(&format!("Couldn't load ticket: {e}"), true),
                Some(Ok(t)) => {
                    let status = t.status;
                    let priority = t.priority;
                    let title_v = page_title(&t.title);
                    let meta_v = meta_line(&t);
                    let desc_v = desc_block(&t.description);
                    let actions_v = lifecycle_bar(status, run, open_triage, open_assign);
                    view! {
                        <Stack gap=Gap::Lg>
                            <Card>
                                <Stack gap=Gap::Md>
                                    <Cluster gap=Gap::Sm justify="space-between".to_string()>
                                        {title_v}
                                        <Cluster gap=Gap::Xs>
                                            <Badge variant=ticket_status_variant(status)>{status.label()}</Badge>
                                            {match priority {
                                                Some(p) => view! { <Badge variant=ticket_priority_variant(p)>{p.label()}</Badge> }.into_any(),
                                                None => ().into_any(),
                                            }}
                                        </Cluster>
                                    </Cluster>
                                    {meta_v}
                                    {desc_v}
                                </Stack>
                            </Card>
                            {actions_v}
                        </Stack>
                    }.into_any()
                }
            }}
            <TriageDialog open=triage_open priority=triage_priority on_confirm=confirm_triage />
            <AssignDialog open=assign_open target=assign_target on_confirm=confirm_assign />
        </Stack>
    }
}

fn lifecycle_bar(
    status: TicketStatus,
    run: impl Fn(TicketAction) + Copy + Send + Sync + 'static,
    open_triage: Callback<leptos::ev::MouseEvent>,
    open_assign: Callback<leptos::ev::MouseEvent>,
) -> AnyView {
    let btn = move |label: &'static str, variant: ButtonVariant, action: TicketAction| {
        let cb = Callback::new(move |_| run(action));
        view! { <Button variant=variant size=ButtonSize::Sm on_click=cb>{label}</Button> }
            .into_any()
    };
    let triage_btn = move || {
        view! {
        <Button variant=ButtonVariant::Primary size=ButtonSize::Sm on_click=open_triage>"Triage"</Button>
    }.into_any()
    };
    let assign_btn = move || {
        view! {
            <Button variant=ButtonVariant::Secondary size=ButtonSize::Sm on_click=open_assign>
                <Icon name=IconName::Users size=14 /> " Assign"
            </Button>
        }
        .into_any()
    };

    let buttons: Vec<AnyView> = match status {
        TicketStatus::Open | TicketStatus::Reopened => vec![triage_btn()],
        TicketStatus::Triaged => vec![assign_btn()],
        TicketStatus::Assigned => vec![
            btn("Start work", ButtonVariant::Primary, TicketAction::Start),
            assign_btn(),
        ],
        TicketStatus::InProgress => vec![btn(
            "Resolve",
            ButtonVariant::Primary,
            TicketAction::Resolve,
        )],
        TicketStatus::Resolved => vec![
            btn("Close", ButtonVariant::Primary, TicketAction::Close),
            btn(
                "Reject resolution",
                ButtonVariant::Secondary,
                TicketAction::Reject,
            ),
        ],
        TicketStatus::Closed => vec![btn(
            "Reopen",
            ButtonVariant::Secondary,
            TicketAction::Reopen,
        )],
    };

    if buttons.is_empty() {
        return ().into_any();
    }
    view! { <Card><Cluster gap=Gap::Sm>{buttons}</Cluster></Card> }.into_any()
}

#[component]
fn TriageDialog(
    open: RwSignal<bool>,
    priority: RwSignal<TicketPriority>,
    on_confirm: Callback<()>,
) -> impl IntoView {
    let on_close = Callback::new(move |()| open.set(false));
    let cancel = Callback::new(move |_| open.set(false));
    let confirm = Callback::new(move |_| on_confirm.run(()));
    let on_priority = Callback::new(move |v: String| priority.set(priority_from_wire(&v)));
    let priority_value = Signal::derive(move || priority_wire(priority.get()).to_owned());
    view! {
        <Dialog open=open on_close=on_close>
            <DialogHeader title="Triage ticket" subtitle="Set a priority to move it into the queue." />
            <DialogBody>
                <div>
                    <FieldLabel for_id="tk-priority">"Priority"</FieldLabel>
                    <Select value=priority_value on_change=on_priority>
                        <option value="low">"Low"</option>
                        <option value="normal">"Normal"</option>
                        <option value="high">"High"</option>
                        <option value="urgent">"Urgent"</option>
                    </Select>
                </div>
            </DialogBody>
            <DialogFooter>
                <Button variant=ButtonVariant::Ghost on_click=cancel>"Cancel"</Button>
                <Button variant=ButtonVariant::Primary on_click=confirm>"Triage"</Button>
            </DialogFooter>
        </Dialog>
    }
}

#[component]
fn AssignDialog(
    open: RwSignal<bool>,
    target: RwSignal<Option<UserId>>,
    on_confirm: Callback<()>,
) -> impl IntoView {
    let on_close = Callback::new(move |()| open.set(false));
    let cancel = Callback::new(move |_| open.set(false));
    let on_select = Callback::new(move |u: UserId| target.set(Some(u)));
    let confirm = Callback::new(move |_| on_confirm.run(()));
    view! {
        <Dialog open=open on_close=on_close>
            <DialogHeader title="Assign ticket" subtitle="Assign this ticket to an IT staffer." />
            <DialogBody>
                <UserPicker selected=target on_select=on_select />
            </DialogBody>
            <DialogFooter>
                <Button variant=ButtonVariant::Ghost on_click=cancel>"Cancel"</Button>
                <Button variant=ButtonVariant::Primary on_click=confirm>"Assign"</Button>
            </DialogFooter>
        </Dialog>
    }
}

fn meta_line(t: &TicketDto) -> AnyView {
    let requester = t.requester.full_name.clone();
    let assignee = t
        .assignee
        .as_ref()
        .map_or_else(|| "Unassigned".to_owned(), |a| a.full_name.clone());
    let created = relative_time(t.created_at);
    let category = t.category.label();
    subtle(&format!(
        "{category} · raised by {requester} · {created} · Assignee: {assignee}"
    ))
}

fn desc_block(description: &str) -> AnyView {
    let cls = class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c}; line-height: 1.55; white-space: pre-wrap;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT,
    ));
    view! { <p class=cls>{description.to_owned()}</p> }.into_any()
}
