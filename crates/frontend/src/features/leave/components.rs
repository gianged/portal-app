//! Leave UI: a "My leave" view (available balance, per-year grants, recent
//! transactions) and an HR-only grant admin (set a user's yearly entitlement and
//! post manual adjustments).

use leptos::{prelude::*, task};
use uuid::Uuid;

use shared::dto::ids::UserId;
use shared::dto::leave_balance::{
    AdjustBalanceRequest, LeaveBalanceDto, LeaveStatementDto, SetLeaveGrantRequest,
};
use shared::dto::user::{UserDto, UserRole};
use shared::validation::leave_balance::{validate_adjust, validate_grant};

use crate::features::leave::api;
use crate::features::users::api as users_api;
use crate::primitives::button::{Button, ButtonVariant};
use crate::primitives::card::{Card, CardBody, CardHeader};
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

#[component]
pub fn MyLeave() -> impl IntoView {
    let auth = use_context::<AuthState>().expect("AuthState context");
    let is_hr = auth
        .user
        .with(|u| u.as_ref().is_some_and(|x| matches!(x.role, UserRole::Hr)));

    let balance: Loadable<LeaveBalanceDto> = RwSignal::new(None);
    Effect::new(move |_| load::load(balance, api::my_balance()));

    let statement: Loadable<LeaveStatementDto> = RwSignal::new(None);
    Effect::new(move |_| {
        let from = days_ago_iso(365.0);
        let to = today_iso();
        load::load(statement, async move { api::statement(&from, &to).await });
    });

    view! {
        <Stack gap=Gap::Lg>
            <Card>
                <CardHeader>{section_title("Balance")}</CardHeader>
                <CardBody>
                    {move || match balance.get() {
                        None => load::note("Loading balance…"),
                        Some(Err(e)) => load::load_error(&e),
                        Some(Ok(b)) => view! { <BalanceView balance=b /> }.into_any(),
                    }}
                </CardBody>
            </Card>

            <Card>
                <CardHeader>{section_title("Recent transactions")}</CardHeader>
                <CardBody>
                    {move || match statement.get() {
                        None => load::note("Loading transactions…"),
                        Some(Err(e)) => load::load_error(&e),
                        Some(Ok(s)) if s.transactions.is_empty() => load::note("No transactions in the last year."),
                        Some(Ok(s)) => {
                            let muted = muted_cls();
                            view! {
                                <Stack gap=Gap::Xs>
                                    {s.transactions.into_iter().map(|t| {
                                        let line = format!(
                                            "{}: {:+} day(s){}",
                                            t.kind.label(),
                                            t.delta,
                                            t.work_pct.map(|p| format!(" · work {p:.0}%")).unwrap_or_default(),
                                        );
                                        view! { <div class=muted.clone()>{line}</div> }
                                    }).collect_view()}
                                </Stack>
                            }.into_any()
                        }
                    }}
                </CardBody>
            </Card>

            {move || is_hr.then(|| view! { <GrantAdmin /> })}
        </Stack>
    }
}

#[component]
fn BalanceView(balance: LeaveBalanceDto) -> impl IntoView {
    let strong = strong_cls();
    let muted = muted_cls();
    let avail = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_H1,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    let row = theme::class(format!(
        "display: flex; align-items: center; justify-content: space-between; gap: {g};",
        g = space::D3,
    ));
    view! {
        <Stack gap=Gap::Md>
            <div>
                <div class=avail>{format!("{} days", balance.available)}</div>
                <div class=muted.clone()>"Available now"</div>
            </div>
            {(!balance.grants.is_empty()).then(|| {
                let row = row.clone();
                let strong = strong.clone();
                let muted = muted.clone();
                view! {
                    <Stack gap=Gap::Xs>
                        {balance.grants.into_iter().map(|g| {
                            let row = row.clone();
                            let strong = strong.clone();
                            let muted = muted.clone();
                            view! {
                                <div class=row>
                                    <span class=strong>{format!("{}", g.grant_year)}</span>
                                    <span class=muted>
                                        {format!("{} / {} left · expires {}", g.days_remaining, g.days_granted, g.expires_on)}
                                    </span>
                                </div>
                            }
                        }).collect_view()}
                    </Stack>
                }
            })}
        </Stack>
    }
}

/// HR grant admin: pick a user, set a year's entitlement, or post an adjustment.
#[component]
fn GrantAdmin() -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");

    let users: RwSignal<Vec<UserDto>> = RwSignal::new(Vec::new());
    Effect::new(move |_| {
        task::spawn_local(async move {
            match users_api::list(None).await {
                Ok(list) => users.set(list),
                Err(e) => toast.error_from(&e),
            }
        });
    });

    let user = RwSignal::new(String::new());
    let target_balance: Loadable<LeaveBalanceDto> = RwSignal::new(None);
    let tick = RwSignal::new(0u32);
    Effect::new(move |_| {
        let _ = tick.get();
        let Ok(id) = Uuid::parse_str(&user.get()) else {
            target_balance.set(None);
            return;
        };
        load::load(target_balance, async move {
            api::user_balance(UserId(id)).await
        });
    });

    let grant_year = RwSignal::new(web_sys::js_sys::Date::new_0().get_full_year().to_string());
    let days = RwSignal::new(String::new());
    let delta = RwSignal::new(String::new());
    let reason = RwSignal::new(String::new());
    let err = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);

    let selected_user = move || Uuid::parse_str(&user.get_untracked()).ok().map(UserId);

    let set_grant = Callback::new(move |_| {
        if busy.get_untracked() {
            return;
        }
        err.set(None);
        let Some(uid) = selected_user() else {
            err.set(Some("Pick a user".into()));
            return;
        };
        let (Ok(year), Ok(d)) = (
            grant_year.get_untracked().trim().parse::<u16>(),
            days.get_untracked().trim().parse::<f64>(),
        ) else {
            err.set(Some("Year and days must be numbers".into()));
            return;
        };
        let req = SetLeaveGrantRequest {
            grant_year: year,
            days_granted: d,
        };
        if let Err(e) = validate_grant(&req) {
            err.set(Some(e.to_string()));
            return;
        }
        busy.set(true);
        task::spawn_local(async move {
            match api::set_grant(uid, &req).await {
                Ok(_) => {
                    toast.success("Grant saved");
                    tick.update(|n| *n += 1);
                }
                Err(e) => {
                    toast.error_from(&e);
                    err.set(Some(e.to_string()));
                }
            }
            busy.set(false);
        });
    });

    let adjust = Callback::new(move |_| {
        if busy.get_untracked() {
            return;
        }
        err.set(None);
        let Some(uid) = selected_user() else {
            err.set(Some("Pick a user".into()));
            return;
        };
        let Ok(d) = delta.get_untracked().trim().parse::<f64>() else {
            err.set(Some("Delta must be a number".into()));
            return;
        };
        let req = AdjustBalanceRequest {
            delta: d,
            reason: reason.get_untracked(),
        };
        if let Err(e) = validate_adjust(&req) {
            err.set(Some(e.to_string()));
            return;
        }
        busy.set(true);
        task::spawn_local(async move {
            match api::adjust(uid, &req).await {
                Ok(_) => {
                    toast.success("Adjustment posted");
                    delta.set(String::new());
                    reason.set(String::new());
                    tick.update(|n| *n += 1);
                }
                Err(e) => {
                    toast.error_from(&e);
                    err.set(Some(e.to_string()));
                }
            }
            busy.set(false);
        });
    });

    let grid = theme::class(format!(
        "display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: {g};",
        g = space::D4,
    ));
    let muted = muted_cls();

    view! {
        <Card>
            <CardHeader>{section_title("Grant admin (HR)")}</CardHeader>
            <CardBody>
                <Stack gap=Gap::Md>
                    <div>
                        <FieldLabel for_id="lv-user".to_string()>"User"</FieldLabel>
                        <Select value=user on_change=Callback::new(move |v| user.set(v))>
                            <option value="">"— select user —"</option>
                            {move || users.get().into_iter().map(|u| {
                                view! { <option value=u.id.0.to_string()>{u.name}</option> }
                            }).collect_view()}
                        </Select>
                    </div>

                    {move || match target_balance.get() {
                        Some(Ok(b)) => {
                            let muted = muted.clone();
                            view! { <div class=muted>{format!("Currently available: {} days", b.available)}</div> }.into_any()
                        }
                        _ => ().into_any(),
                    }}

                    <div class=grid.clone()>
                        <div>
                            <FieldLabel for_id="lv-year".to_string()>"Grant year"</FieldLabel>
                            <Input value=grant_year on_input=Callback::new(move |v| grant_year.set(v)) type_="number".to_string() />
                        </div>
                        <div>
                            <FieldLabel for_id="lv-days".to_string()>"Days granted"</FieldLabel>
                            <Input value=days on_input=Callback::new(move |v| days.set(v)) type_="number".to_string() />
                        </div>
                    </div>
                    <div>
                        <Button variant=ButtonVariant::Primary on_click=set_grant disabled=Signal::derive(move || busy.get())>
                            "Set grant"
                        </Button>
                    </div>

                    <div class=grid.clone()>
                        <div>
                            <FieldLabel for_id="lv-delta".to_string()>"Adjustment (days, +/-)"</FieldLabel>
                            <Input value=delta on_input=Callback::new(move |v| delta.set(v)) type_="number".to_string() />
                        </div>
                        <div>
                            <FieldLabel for_id="lv-reason".to_string()>"Reason"</FieldLabel>
                            <Input value=reason on_input=Callback::new(move |v| reason.set(v)) />
                        </div>
                    </div>
                    <div>
                        <Button variant=ButtonVariant::Secondary on_click=adjust disabled=Signal::derive(move || busy.get())>
                            "Post adjustment"
                        </Button>
                    </div>

                    {move || err.get().map(|m| view! { <FieldError message=m /> })}
                </Stack>
            </CardBody>
        </Card>
    }
}

fn section_title(title: &'static str) -> impl IntoView {
    let cls = theme::class(format!(
        "font-size: 13px; font-weight: {fw}; color: {c}; text-transform: uppercase; letter-spacing: 0.04em;",
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_MUTED,
    ));
    view! { <div class=cls>{title}</div> }
}
