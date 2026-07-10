//! Flexible-hours UI: a per-day segment editor (live daily total vs the band,
//! core-hours coverage hint, running monthly delta + remaining quota) plus a "my
//! flexible hours" list (with cancel) and a leader approval queue. The server is
//! the real authorization, shape, and cap gate.

use leptos::{prelude::*, task};
use uuid::Uuid;
use web_sys::js_sys::Date;

use shared::dto::flex_hours::{
    DecideFlexRequest, FlexHoursDto, FlexMonthDeltaDto, FlexSegmentInput, FlexStatus,
    RequestFlexRequest,
};
use shared::dto::group::GroupDto;
use shared::dto::ids::GroupId;
use shared::dto::policy::PolicyDto;
use shared::dto::user::UserRole;
use shared::validation::flex_hours;
use shared::validation::policy;

use crate::features::flex_hours::api;
use crate::features::groups::api as groups_api;
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

// --- date + time helpers ---

/// Current `(year, month)` from the browser clock.
fn current_year_month() -> (i32, u32) {
    let d = Date::new_0();
    (d.get_full_year() as i32, d.get_month() + 1)
}

fn current_month_prefix() -> String {
    let (y, m) = current_year_month();
    format!("{y:04}-{m:02}")
}

/// Minutes-since-midnight of a `"HH:MM"` string, or `None` if malformed.
fn hhmm_min(s: &str) -> Option<u16> {
    let (h, m) = policy::parse_hhmm(s)?;
    Some(u16::from(h) * 60 + u16::from(m))
}

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

fn segs_summary(f: &FlexHoursDto) -> String {
    f.segments
        .iter()
        .map(|s| format!("{}-{}", s.start, s.end))
        .collect::<Vec<_>>()
        .join(", ")
}

/// One editable work block; signals are stable so editing keeps input focus.
#[derive(Clone, Copy)]
struct SegRow {
    key: usize,
    start: RwSignal<String>,
    end: RwSignal<String>,
}

fn make_row(key: usize, start: &str, end: &str) -> SegRow {
    SegRow {
        key,
        start: RwSignal::new(start.to_string()),
        end: RwSignal::new(end.to_string()),
    }
}

// --- request + my list ---

#[component]
pub fn FlexHours() -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");

    let work_date = RwSignal::new(today_iso());
    let rows = RwSignal::new(vec![make_row(0, "08:00", "17:00")]);
    let next_key = RwSignal::new(1usize);
    let err = RwSignal::new(None::<String>);
    let saving = RwSignal::new(false);

    let mine: Loadable<Vec<FlexHoursDto>> = RwSignal::new(None);
    let tick = RwSignal::new(0u32);
    Effect::new(move |_| {
        let _ = tick.get();
        let from = days_ago_iso(120.0);
        let to = days_ago_iso(-365.0);
        load::load(mine, async move { api::list_mine(&from, &to).await });
    });

    // Policy supplies the segment cap, core window and daily band for the hints.
    let policy: Loadable<PolicyDto> = RwSignal::new(None);
    Effect::new(move |_| load::load(policy, policy_api::get_policy()));

    // Running monthly settlement delta for the current month.
    let delta: Loadable<FlexMonthDeltaDto> = RwSignal::new(None);
    Effect::new(move |_| {
        let _ = tick.get();
        let (year, month) = current_year_month();
        load::load(delta, async move { api::month_delta(year, month).await });
    });

    let add_block = move |_| {
        let max = policy
            .get_untracked()
            .and_then(Result::ok)
            .map_or(2, |p| usize::from(p.flex_max_segments));
        rows.update(|v| {
            if v.len() < max {
                let k = next_key.get_untracked();
                next_key.set(k + 1);
                v.push(make_row(k, "13:00", "17:00"));
            }
        });
    };

    let submit = Callback::new(move |_| {
        if saving.get_untracked() {
            return;
        }
        err.set(None);
        let segments: Vec<FlexSegmentInput> = rows
            .get_untracked()
            .iter()
            .map(|r| FlexSegmentInput {
                start: r.start.get_untracked(),
                end: r.end.get_untracked(),
            })
            .collect();
        let req = RequestFlexRequest {
            work_date: work_date.get_untracked(),
            segments,
        };
        // Client check against the loaded policy, if available.
        if let Some(Ok(p)) = policy.get_untracked()
            && let Err(e) = flex_hours::validate_flex(&req, &p)
        {
            err.set(Some(e.to_string()));
            return;
        }
        saving.set(true);
        task::spawn_local(async move {
            match api::create(&req).await {
                Ok(_) => {
                    toast.success("Flex hours requested");
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

    // Live daily total in minutes, summing valid blocks.
    let daily_min = move || -> u16 {
        rows.get()
            .iter()
            .filter_map(|r| {
                let s = hhmm_min(&r.start.get())?;
                let e = hhmm_min(&r.end.get())?;
                (e > s).then_some(e - s)
            })
            .sum()
    };

    let muted_hint = muted_cls();
    let muted_quota = muted_cls();
    let row_layout = theme::class(format!(
        "display: grid; grid-template-columns: 1fr 1fr auto; gap: {g}; align-items: end;",
        g = space::D3,
    ));

    view! {
        <Stack gap=Gap::Lg>
            <Card>
                <Stack gap=Gap::Md>
                    {
                        let muted = muted_hint;
                        move || {
                            let muted = muted.clone();
                            match policy.get() {
                                Some(Ok(p)) => {
                                    let total = f64::from(daily_min()) / 60.0;
                                    let covers = covers_core(&rows.get(), &p);
                                    let core = format!("{} - {}", p.flex_core_start, p.flex_core_end);
                                    view! {
                                        <Stack gap=Gap::Sm>
                                            <div class=muted.clone()>
                                                {format!(
                                                    "Daily total: {total:.2}h (band {min}-{max}h, up to {n} blocks)",
                                                    min = p.flex_daily_min, max = p.flex_daily_max, n = p.flex_max_segments,
                                                )}
                                            </div>
                                            <div class=muted.clone()>
                                                {if covers {
                                                    format!("Core hours {core}: covered")
                                                } else {
                                                    format!("Core hours {core}: not yet covered")
                                                }}
                                            </div>
                                        </Stack>
                                    }.into_any()
                                }
                                _ => ().into_any(),
                            }
                        }
                    }

                    <div>
                        <FieldLabel for_id="flex-date".to_string()>"Work date"</FieldLabel>
                        <Input value=work_date on_input=Callback::new(move |v| work_date.set(v)) type_="date".to_string() />
                    </div>

                    {
                        let row_layout = row_layout.clone();
                        move || {
                            let row_layout = row_layout.clone();
                            let allow_remove = rows.get().len() > 1;
                            rows.get().into_iter().map(move |r| {
                                let remove = move |_| {
                                    rows.update(|v| if v.len() > 1 { v.retain(|x| x.key != r.key); });
                                };
                                view! {
                                    <div class=row_layout.clone()>
                                        <div>
                                            <FieldLabel for_id="flex-start".to_string()>"Start"</FieldLabel>
                                            <Input value=r.start on_input=Callback::new(move |v| r.start.set(v)) type_="time".to_string() />
                                        </div>
                                        <div>
                                            <FieldLabel for_id="flex-end".to_string()>"End"</FieldLabel>
                                            <Input value=r.end on_input=Callback::new(move |v| r.end.set(v)) type_="time".to_string() />
                                        </div>
                                        {allow_remove.then(|| view! {
                                            <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm on_click=Callback::new(remove)>
                                                "Remove"
                                            </Button>
                                        })}
                                    </div>
                                }
                            }).collect_view()
                        }
                    }

                    <div>
                        <Button variant=ButtonVariant::Secondary size=ButtonSize::Sm on_click=Callback::new(add_block)>
                            "Add block"
                        </Button>
                    </div>

                    {move || err.get().map(|m| view! { <FieldError message=m /> })}
                    <div>
                        <Button variant=ButtonVariant::Primary on_click=submit disabled=Signal::derive(move || saving.get())>
                            {move || if saving.get() { "Requesting..." } else { "Request flex hours" }}
                        </Button>
                    </div>
                </Stack>
            </Card>

            <Card>
                {
                    let muted = muted_quota;
                    move || {
                        let muted = muted.clone();
                        let quota = match (policy.get(), mine.get()) {
                            (Some(Ok(p)), Some(Ok(list))) => {
                                let prefix = current_month_prefix();
                                let used = list.iter().filter(|f| {
                                    matches!(f.status, FlexStatus::Approved) && f.work_date.starts_with(&prefix)
                                }).count();
                                let left = usize::from(p.flex_max_per_month).saturating_sub(used);
                                Some(format!("Flex days left this month: {left} of {}", p.flex_max_per_month))
                            }
                            _ => None,
                        };
                        let delta_line = match delta.get() {
                            Some(Ok(d)) => Some(format!("Monthly settlement delta: {:+.2}h (0 = reconciled)", d.delta)),
                            _ => None,
                        };
                        view! {
                            <Stack gap=Gap::Sm>
                                {quota.map(|q| view! { <div class=muted.clone()>{q}</div> })}
                                {delta_line.map(|d| view! { <div class=muted.clone()>{d}</div> })}
                            </Stack>
                        }
                    }
                }
            </Card>

            <Stack gap=Gap::Sm>
                <SectionTitle title="My flexible hours" />
                {move || match mine.get() {
                    None => load::note("Loading..."),
                    Some(Err(e)) => load::load_error(&e),
                    Some(Ok(list)) if list.is_empty() => load::note("No flex requests."),
                    Some(Ok(list)) => view! {
                        <Stack gap=Gap::Sm>
                            {list.into_iter().map(|f| view! {
                                <MyRow flex=f on_changed=on_changed />
                            }).collect_view()}
                        </Stack>
                    }.into_any(),
                }}
            </Stack>
        </Stack>
    }
}

/// Whether the editor's blocks continuously cover the policy core window.
fn covers_core(rows: &[SegRow], policy: &PolicyDto) -> bool {
    let (Some(core_start), Some(core_end)) = (
        hhmm_min(&policy.flex_core_start),
        hhmm_min(&policy.flex_core_end),
    ) else {
        return false;
    };
    let mut blocks: Vec<(u16, u16)> = rows
        .iter()
        .filter_map(|r| {
            let s = hhmm_min(&r.start.get())?;
            let e = hhmm_min(&r.end.get())?;
            (e > s).then_some((s, e))
        })
        .collect();
    blocks.sort_by_key(|(s, _)| *s);
    let mut covered = core_start;
    for (s, e) in &blocks {
        if *s <= covered {
            if *e > covered {
                covered = *e;
            }
        } else {
            break;
        }
        if covered >= core_end {
            break;
        }
    }
    covered >= core_end
}

#[component]
fn MyRow(flex: FlexHoursDto, on_changed: Callback<()>) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let busy = RwSignal::new(false);
    let id = flex.id;
    let cancellable = matches!(flex.status, FlexStatus::Pending);

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
    let line = format!(
        "{} · {:.2}h · {}",
        flex.work_date,
        flex.daily_hours,
        segs_summary(&flex)
    );

    view! {
        <Card>
            <div class=row>
                <div>
                    <span class=strong>{line}</span>
                    <span class=muted>{format!("  ·  {}", flex.status.label())}</span>
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

// --- approval queue (leader only) ---

#[component]
pub fn Approvals() -> impl IntoView {
    let auth = use_context::<AuthState>().expect("AuthState context");
    let show_leader = auth.user.with(|u| {
        u.as_ref().is_some_and(|x| {
            matches!(
                x.role,
                UserRole::GroupLeader | UserRole::Director | UserRole::Hr
            )
        })
    });

    view! {
        <Stack gap=Gap::Lg>
            {show_leader.then(|| view! { <LeaderQueue /> })}
            {(!show_leader).then(|| load::note("You have no approval queues."))}
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
    let queue: Loadable<Vec<FlexHoursDto>> = RwSignal::new(None);
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
                <FieldLabel for_id="flex-grp".to_string()>"Group"</FieldLabel>
                <Select value=group on_change=Callback::new(move |v| group.set(v))>
                    <option value="">"- select group -"</option>
                    {move || groups.get().into_iter().map(|g| {
                        view! { <option value=g.id.0.to_string()>{g.name}</option> }
                    }).collect_view()}
                </Select>
            </div>
            {move || {
                if group.get().is_empty() {
                    return load::note("Pick a group to review its pending flex requests.");
                }
                match queue.get() {
                    None => load::note("Loading..."),
                    Some(Err(e)) => load::load_error(&e),
                    Some(Ok(list)) if list.is_empty() => load::note("No pending requests."),
                    Some(Ok(list)) => view! {
                        <Stack gap=Gap::Sm>
                            {list.into_iter().map(|f| view! {
                                <DecideCard flex=f on_done=on_done />
                            }).collect_view()}
                        </Stack>
                    }.into_any(),
                }
            }}
        </Stack>
    }
}

#[component]
fn DecideCard(flex: FlexHoursDto, on_done: Callback<()>) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let note = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let id = flex.id;

    let decide = move |approve: bool| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        let req = DecideFlexRequest {
            approve,
            note: note.get_untracked(),
        };
        task::spawn_local(async move {
            match api::decision(id, &req).await {
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
    let line = format!(
        "{} · {:.2}h · {}",
        flex.work_date,
        flex.daily_hours,
        segs_summary(&flex)
    );

    view! {
        <Card>
            <Stack gap=Gap::Sm>
                <div>
                    <span class=strong>{flex.user.full_name.clone()}</span>
                    <span class=muted.clone()>{format!("  ·  {line}")}</span>
                </div>
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
