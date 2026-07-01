//! IT-ticket detail: status-gated lifecycle actions, triage (set priority), and assignment, plus the comment thread and audit trail.

use futures::FutureExt;
use futures::future::LocalBoxFuture;
use leptos::{ev::MouseEvent, prelude::*, task};

use shared::dto::ids::{TicketId, UserId};
use shared::dto::ticket::{
    AssignTicketRequest, TicketDto, TicketPriority, TicketStatus, TriageTicketRequest,
};

use crate::api::error::FrontendError;
use crate::features::audit::components::{AuditTrailPanel, TrailKind};
use crate::features::comments::{CommentTarget, CommentThread};
use crate::features::tickets::api;
use crate::features::ui;
use crate::features::users::picker::UserPicker;
use crate::primitives::badge::Badge;
use crate::primitives::button::{Button, ButtonSize, ButtonVariant};
use crate::primitives::card::Card;
use crate::primitives::cluster::Cluster;
use crate::primitives::dialog::{Dialog, DialogBody, DialogFooter, DialogHeader};
use crate::primitives::icon::{Icon, IconName};
use crate::primitives::input::FieldLabel;
use crate::primitives::select::Select;
use crate::primitives::stack::{Gap, Stack};
use crate::state::toast::ToastState;
use crate::theme::{self, color, typography};
use crate::util::format;
use crate::util::load::{self, Loadable};

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
            load::load(detail, api::get(tid));
        }
    });

    let run = move |action: TicketAction| {
        let Some(tid) = id.get_untracked() else {
            return;
        };
        task::spawn_local(async move {
            match action_future(action, tid).await {
                Ok(_) => {
                    toast.success("Ticket updated");
                    reload.update(|n| *n += 1);
                }
                Err(e) => toast.error_from(&e),
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
        task::spawn_local(async move {
            match api::triage(tid, &req).await {
                Ok(_) => {
                    toast.success("Ticket triaged");
                    reload.update(|n| *n += 1);
                }
                Err(e) => toast.error_from(&e),
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
        task::spawn_local(async move {
            let req = AssignTicketRequest {
                assignee_user_id: uid,
            };
            match api::assign(tid, &req).await {
                Ok(_) => {
                    toast.success("Ticket assigned");
                    reload.update(|n| *n += 1);
                }
                Err(e) => toast.error_from(&e),
            }
        });
    });

    let open_triage = Callback::new(move |_| triage_open.set(true));
    let open_assign = Callback::new(move |_| assign_open.set(true));

    view! {
        <Stack gap=Gap::Lg>
            {ui::back_link("/tickets", "Back to tickets")}
            {move || match detail.get() {
                None => load::note("Loading ticket…"),
                Some(Err(e)) => load::load_error(&e),
                Some(Ok(t)) => {
                    let status = t.status;
                    let priority = t.priority;
                    let title_v = ui::page_title(&t.title);
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
                                            <Badge variant=format::ticket_status_variant(status)>{status.label()}</Badge>
                                            {match priority {
                                                Some(p) => view! { <Badge variant=format::ticket_priority_variant(p)>{p.label()}</Badge> }.into_any(),
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
            <CommentThread target=Signal::derive(move || id.get().map(CommentTarget::Ticket)) />
            <AuditTrailPanel
                id=Signal::derive(move || id.get().map(|t| t.0))
                kind=TrailKind::Ticket
                refresh=reload
            />
            <TriageDialog open=triage_open priority=triage_priority on_confirm=confirm_triage />
            <AssignDialog open=assign_open target=assign_target on_confirm=confirm_assign />
        </Stack>
    }
}

fn lifecycle_bar(
    status: TicketStatus,
    run: impl Fn(TicketAction) + Copy + Send + Sync + 'static,
    open_triage: Callback<MouseEvent>,
    open_assign: Callback<MouseEvent>,
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
    let created = format::relative_time(t.created_at);
    let category = t.category.label();
    ui::subtle(&format!(
        "{category} · raised by {requester} · {created} · Assignee: {assignee}"
    ))
}

fn desc_block(description: &str) -> AnyView {
    let cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c}; line-height: 1.55; white-space: pre-wrap;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT,
    ));
    view! { <p class=cls>{description.to_owned()}</p> }.into_any()
}
