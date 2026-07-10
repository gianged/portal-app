//! Overtime UI: a request form (showing the monthly cap from policy) + "my
//! overtime" list (with cancel), and approval queues for leaders and HR. The
//! server is the real authorization and cap gate.

use leptos::{prelude::*, task};
use uuid::Uuid;

use shared::dto::group::GroupDto;
use shared::dto::ids::GroupId;
use shared::dto::overtime::{
    CreateOvertimeRequest, DecideOvertimeRequest, OvertimeDto, OvertimeStatus,
};
use shared::dto::policy::PolicyDto;
use shared::dto::user::UserRole;
use shared::validation::overtime::validate_overtime;

use crate::features::groups::api as groups_api;
use crate::features::overtime::api;
use crate::features::policy::api as policy_api;
use crate::primitives::button::{Button, ButtonSize, ButtonVariant};
use crate::primitives::card::Card;
use crate::primitives::input::{FieldError, FieldLabel, Input};
use crate::primitives::select::Select;
use crate::primitives::stack::{Gap, Stack};
use crate::state::auth::AuthState;
use crate::state::toast::ToastState;
use crate::theme::{self, color, space, typography};
use crate::util::date::{days_ago_iso, today_iso};
use crate::util::load::{self, Loadable};

fn muted_cls() -> String {
    theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT_MUTED,
    ))
}

fn strong_cls() -> String {
    theme::class(format!(
        "font-family: {ff}; font-weight: {fw}; color: {c};",
        ff = typography::FONT_SANS,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ))
}

// --- request + my list ---

#[component]
pub fn Overtime() -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");

    let work_date = RwSignal::new(today_iso());
    let hours = RwSignal::new("1".to_string());
    let reason = RwSignal::new(String::new());
    let err = RwSignal::new(None::<String>);
    let saving = RwSignal::new(false);

    let mine: Loadable<Vec<OvertimeDto>> = RwSignal::new(None);
    let tick = RwSignal::new(0u32);
    Effect::new(move |_| {
        let _ = tick.get();
        let from = days_ago_iso(120.0);
        let to = days_ago_iso(-365.0);
        load::load(mine, async move { api::list_mine(&from, &to).await });
    });

    // Monthly legal cap, surfaced from the attendance policy.
    let policy: Loadable<PolicyDto> = RwSignal::new(None);
    Effect::new(move |_| load::load(policy, policy_api::get_policy()));

    let submit = Callback::new(move |_| {
        if saving.get_untracked() {
            return;
        }
        err.set(None);
        let parsed = hours.get_untracked().trim().parse::<f64>().unwrap_or(0.0);
        let req = CreateOvertimeRequest {
            work_date: work_date.get_untracked(),
            hours: parsed,
            reason: reason.get_untracked(),
        };
        if let Err(e) = validate_overtime(&req) {
            err.set(Some(e.to_string()));
            return;
        }
        saving.set(true);
        task::spawn_local(async move {
            match api::create(&req).await {
                Ok(_) => {
                    toast.success("Overtime requested");
                    reason.set(String::new());
                    tick.update(|n| *n += 1);
                }
                Err(e) => {
                    toast.error_from(&e);
                    err.set(Some(e.to_string()));
                }
            }
            saving.set(false);
        });
    });

    let on_changed = Callback::new(move |()| tick.update(|n| *n += 1));

    let grid = theme::class(format!(
        "display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: {g};",
        g = space::D4,
    ));
    let muted = muted_cls();

    view! {
        <Stack gap=Gap::Lg>
            <Card>
                <Stack gap=Gap::Md>
                    {move || {
                        let muted = muted.clone();
                        match policy.get() {
                            Some(Ok(p)) => view! {
                                <div class=muted.clone()>
                                    {format!("Monthly overtime cap: {} hours", p.overtime_max_hours_per_month)}
                                </div>
                            }.into_any(),
                            _ => ().into_any(),
                        }
                    }}
                    <div class=grid.clone()>
                        <div>
                            <FieldLabel for_id="ot-date".to_string()>"Work date"</FieldLabel>
                            <Input value=work_date on_input=Callback::new(move |v| work_date.set(v)) type_="date".to_string() />
                        </div>
                        <div>
                            <FieldLabel for_id="ot-hours".to_string()>"Hours"</FieldLabel>
                            <Input value=hours on_input=Callback::new(move |v| hours.set(v)) type_="number".to_string() />
                        </div>
                    </div>
                    <div>
                        <FieldLabel for_id="ot-reason".to_string()>"Reason"</FieldLabel>
                        <Input value=reason on_input=Callback::new(move |v| reason.set(v)) placeholder="Optional".to_string() />
                    </div>
                    {move || err.get().map(|m| view! { <FieldError message=m /> })}
                    <div>
                        <Button variant=ButtonVariant::Primary on_click=submit disabled=Signal::derive(move || saving.get())>
                            {move || if saving.get() { "Requesting…" } else { "Request overtime" }}
                        </Button>
                    </div>
                </Stack>
            </Card>

            <Stack gap=Gap::Sm>
                <SectionTitle title="My overtime" />
                {move || match mine.get() {
                    None => load::note("Loading…"),
                    Some(Err(e)) => load::load_error(&e),
                    Some(Ok(list)) if list.is_empty() => load::note("No overtime requests."),
                    Some(Ok(list)) => view! {
                        <Stack gap=Gap::Sm>
                            {list.into_iter().map(|o| view! {
                                <MyRow overtime=o on_changed=on_changed />
                            }).collect_view()}
                        </Stack>
                    }.into_any(),
                }}
            </Stack>
        </Stack>
    }
}

#[component]
fn MyRow(overtime: OvertimeDto, on_changed: Callback<()>) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let busy = RwSignal::new(false);
    let id = overtime.id;
    let cancellable = matches!(
        overtime.status,
        OvertimeStatus::Pending | OvertimeStatus::LeaderApproved
    );

    let cancel = move |_| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        task::spawn_local(async move {
            match api::cancel(id).await {
                Ok(_) => {
                    toast.success("Cancelled");
                    on_changed.run(());
                }
                Err(e) => toast.error_from(&e),
            }
            busy.set(false);
        });
    };

    let row = theme::class(format!(
        "display: flex; align-items: center; justify-content: space-between; gap: {g};",
        g = space::D3,
    ));
    let muted = muted_cls();
    let strong = strong_cls();
    let line = format!("{} · {} hour(s)", overtime.work_date, overtime.hours);

    view! {
        <Card>
            <div class=row>
                <div>
                    <span class=strong>{line}</span>
                    <span class=muted>{format!("  ·  {}", overtime.status.label())}</span>
                </div>
                {cancellable.then(|| view! {
                    <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm
                        on_click=Callback::new(cancel) disabled=Signal::derive(move || busy.get())>
                        "Cancel"
                    </Button>
                })}
            </div>
        </Card>
    }
}

// --- approval queues ---

#[component]
pub fn Approvals() -> impl IntoView {
    let auth = use_context::<AuthState>().expect("AuthState context");
    let (show_leader, is_hr) = auth.user.with(|u| {
        u.as_ref().map_or((false, false), |x| {
            (
                matches!(
                    x.role,
                    UserRole::GroupLeader | UserRole::Director | UserRole::Hr
                ),
                matches!(x.role, UserRole::Hr),
            )
        })
    });

    view! {
        <Stack gap=Gap::Lg>
            {show_leader.then(|| view! { <LeaderQueue /> })}
            {is_hr.then(|| view! { <HrQueue /> })}
            {(!show_leader && !is_hr).then(|| load::note("You have no approval queues."))}
        </Stack>
    }
}

#[component]
fn LeaderQueue() -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let groups: RwSignal<Vec<GroupDto>> = RwSignal::new(Vec::new());
    Effect::new(move |_| {
        task::spawn_local(async move {
            match groups_api::list().await {
                Ok(list) => groups.set(list),
                Err(e) => toast.error_from(&e),
            }
        });
    });

    let group = RwSignal::new(String::new());
    let queue: Loadable<Vec<OvertimeDto>> = RwSignal::new(None);
    let tick = RwSignal::new(0u32);
    Effect::new(move |_| {
        let _ = tick.get();
        let Ok(gid) = Uuid::parse_str(&group.get()) else {
            queue.set(None);
            return;
        };
        load::load(queue, async move { api::leader_queue(GroupId(gid)).await });
    });
    let on_done = Callback::new(move |()| tick.update(|n| *n += 1));

    view! {
        <Stack gap=Gap::Sm>
            <SectionTitle title="Leader queue" />
            <div class=theme::class("min-width: 220px; max-width: 320px;")>
                <FieldLabel for_id="ot-grp".to_string()>"Group"</FieldLabel>
                <Select value=group on_change=Callback::new(move |v| group.set(v))>
                    <option value="">"— select group —"</option>
                    {move || groups.get().into_iter().map(|g| {
                        view! { <option value=g.id.0.to_string()>{g.name}</option> }
                    }).collect_view()}
                </Select>
            </div>
            {move || {
                if group.get().is_empty() {
                    return load::note("Pick a group to review its pending overtime.");
                }
                match queue.get() {
                    None => load::note("Loading…"),
                    Some(Err(e)) => load::load_error(&e),
                    Some(Ok(list)) if list.is_empty() => load::note("No pending requests."),
                    Some(Ok(list)) => view! {
                        <Stack gap=Gap::Sm>
                            {list.into_iter().map(|o| view! {
                                <DecideCard overtime=o is_hr=false on_done=on_done />
                            }).collect_view()}
                        </Stack>
                    }.into_any(),
                }
            }}
        </Stack>
    }
}

#[component]
fn HrQueue() -> impl IntoView {
    let queue: Loadable<Vec<OvertimeDto>> = RwSignal::new(None);
    let tick = RwSignal::new(0u32);
    Effect::new(move |_| {
        let _ = tick.get();
        load::load(queue, api::hr_queue());
    });
    let on_done = Callback::new(move |()| tick.update(|n| *n += 1));

    view! {
        <Stack gap=Gap::Sm>
            <SectionTitle title="HR queue" />
            {move || match queue.get() {
                None => load::note("Loading…"),
                Some(Err(e)) => load::load_error(&e),
                Some(Ok(list)) if list.is_empty() => load::note("Nothing awaiting HR."),
                Some(Ok(list)) => view! {
                    <Stack gap=Gap::Sm>
                        {list.into_iter().map(|o| view! {
                            <DecideCard overtime=o is_hr=true on_done=on_done />
                        }).collect_view()}
                    </Stack>
                }.into_any(),
            }}
        </Stack>
    }
}

#[component]
fn DecideCard(overtime: OvertimeDto, is_hr: bool, on_done: Callback<()>) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let note = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let id = overtime.id;

    let decide = move |approve: bool| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        let req = DecideOvertimeRequest {
            approve,
            note: note.get_untracked(),
        };
        task::spawn_local(async move {
            let res = if is_hr {
                api::hr_decision(id, &req).await
            } else {
                api::leader_decision(id, &req).await
            };
            match res {
                Ok(_) => {
                    toast.success(if approve { "Approved" } else { "Rejected" });
                    on_done.run(());
                }
                Err(e) => toast.error_from(&e),
            }
            busy.set(false);
        });
    };

    let muted = muted_cls();
    let strong = strong_cls();
    let line = format!("{} · {} hour(s)", overtime.work_date, overtime.hours);
    let reason = overtime.reason.clone();

    view! {
        <Card>
            <Stack gap=Gap::Sm>
                <div>
                    <span class=strong>{overtime.requester.full_name.clone()}</span>
                    <span class=muted.clone()>{format!("  ·  {line}")}</span>
                </div>
                {(!reason.is_empty()).then(|| view! { <div class=muted.clone()>{reason}</div> })}
                <Input value=note on_input=Callback::new(move |v| note.set(v)) placeholder="Decision note (optional)".to_string() />
                <div class=theme::class(format!("display: flex; gap: {g};", g = space::D2))>
                    <Button variant=ButtonVariant::Primary size=ButtonSize::Sm
                        on_click=Callback::new(move |_| decide(true)) disabled=Signal::derive(move || busy.get())>
                        "Approve"
                    </Button>
                    <Button variant=ButtonVariant::Secondary size=ButtonSize::Sm
                        on_click=Callback::new(move |_| decide(false)) disabled=Signal::derive(move || busy.get())>
                        "Reject"
                    </Button>
                </div>
            </Stack>
        </Card>
    }
}

#[component]
fn SectionTitle(title: &'static str) -> impl IntoView {
    let cls = theme::class(format!(
        "font-size: {fs}; font-weight: {fw}; color: {c}; text-transform: uppercase; letter-spacing: 0.04em;",
        fs = typography::TEXT_LABEL,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_MUTED,
    ));
    view! { <div class=cls>{title}</div> }
}
