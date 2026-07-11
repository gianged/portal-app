//! Leave UI: a "My leave" view (available balance, per-year grants, recent
//! transactions) and an HR-only grant admin (set a user's yearly entitlement and
//! post manual adjustments).

use leptos::{prelude::*, task};
use web_sys::js_sys::Date;

use shared::dto::ids::UserId;
use shared::dto::leave_balance::{
    AdjustBalanceRequest, LeaveBalanceDto, LeaveStatementDto, SetLeaveGrantRequest,
};
use shared::dto::user::UserRole;
use shared::validation::leave_balance;

use crate::features::leave::api;
use crate::features::ui;
use crate::features::users::picker::UserPicker;
use crate::primitives::button::{Button, ButtonVariant};
use crate::primitives::card::{Card, CardBody, CardHeader};
use crate::primitives::input::{FieldError, FieldLabel, Input};
use crate::primitives::stack::{Gap, Stack};
use crate::state::auth::AuthState;
use crate::state::toast::ToastState;
use crate::theme::{self, color, space, typography};
use crate::util::date;
use crate::util::load::{self, Loadable};

#[component]
pub fn MyLeave() -> impl IntoView {
    let auth = use_context::<AuthState>().expect("AuthState context");
    let is_hr = auth
        .user
        .with(|u| u.as_ref().is_some_and(|x| matches!(x.role, UserRole::Hr)));

    let balance: Loadable<LeaveBalanceDto> = Loadable::new();
    Effect::new(move |_| load::load(balance, api::my_balance()));

    let statement: Loadable<LeaveStatementDto> = Loadable::new();
    Effect::new(move |_| {
        let from = date::days_ago_iso(365.0);
        let to = date::today_iso();
        load::load(statement, async move { api::statement(&from, &to).await });
    });

    view! {
        <Stack gap=Gap::Lg>
            <Card>
                <CardHeader>{ui::eyebrow_title("Balance")}</CardHeader>
                <CardBody>
                    {move || match balance.get() {
                        None => load::note("Loading balance…"),
                        Some(Err(e)) => load::load_error(&e),
                        Some(Ok(b)) => view! { <BalanceView balance=b /> }.into_any(),
                    }}
                </CardBody>
            </Card>

            <Card>
                <CardHeader>{ui::eyebrow_title("Recent transactions")}</CardHeader>
                <CardBody>
                    {move || match statement.get() {
                        None => load::note("Loading transactions…"),
                        Some(Err(e)) => load::load_error(&e),
                        Some(Ok(s)) if s.transactions.is_empty() => load::note("No transactions in the last year."),
                        Some(Ok(s)) => {
                            let muted = ui::muted_class();
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
    let strong = ui::strong_class();
    let muted = ui::muted_class();
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
                                        {format!("{} / {} left · expires {}", g.days_remaining, g.days_granted, date::to_iso(g.expires_on))}
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

    let user = RwSignal::new(None::<UserId>);
    let target_balance: Loadable<LeaveBalanceDto> = Loadable::new();
    let tick = RwSignal::new(0u32);
    Effect::new(move |_| {
        let _ = tick.get();
        let Some(uid) = user.get() else {
            target_balance.set(None);
            return;
        };
        load::load(target_balance, async move { api::user_balance(uid).await });
    });

    let grant_year = RwSignal::new(Date::new_0().get_full_year().to_string());
    let days = RwSignal::new(String::new());
    let delta = RwSignal::new(String::new());
    let reason = RwSignal::new(String::new());
    let err = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);

    let set_grant = Callback::new(move |_| {
        if busy.get_untracked() {
            return;
        }
        err.set(None);
        let Some(uid) = user.get_untracked() else {
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
        if let Err(e) = leave_balance::validate_grant(&req) {
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
        let Some(uid) = user.get_untracked() else {
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
        if let Err(e) = leave_balance::validate_adjust(&req) {
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
    let muted = ui::muted_class();

    view! {
        <Card>
            <CardHeader>{ui::eyebrow_title("Grant admin (HR)")}</CardHeader>
            <CardBody>
                <Stack gap=Gap::Md>
                    <div>
                        <FieldLabel for_id="lv-user">"User"</FieldLabel>
                        <UserPicker selected=user on_select=Callback::new(move |u| user.set(Some(u))) />
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
                            <FieldLabel for_id="lv-year">"Grant year"</FieldLabel>
                            <Input value=grant_year on_input=Callback::new(move |v| grant_year.set(v)) type_="number" />
                        </div>
                        <div>
                            <FieldLabel for_id="lv-days">"Days granted"</FieldLabel>
                            <Input value=days on_input=Callback::new(move |v| days.set(v)) type_="number" />
                        </div>
                    </div>
                    <div>
                        <Button variant=ButtonVariant::Primary on_click=set_grant disabled=busy>
                            "Set grant"
                        </Button>
                    </div>

                    <div class=grid.clone()>
                        <div>
                            <FieldLabel for_id="lv-delta">"Adjustment (days, +/-)"</FieldLabel>
                            <Input value=delta on_input=Callback::new(move |v| delta.set(v)) type_="number" />
                        </div>
                        <div>
                            <FieldLabel for_id="lv-reason">"Reason"</FieldLabel>
                            <Input value=reason on_input=Callback::new(move |v| reason.set(v)) />
                        </div>
                    </div>
                    <div>
                        <Button variant=ButtonVariant::Secondary on_click=adjust disabled=busy>
                            "Post adjustment"
                        </Button>
                    </div>

                    {move || err.get().map(|m| view! { <FieldError message=m /> })}
                </Stack>
            </CardBody>
        </Card>
    }
}
