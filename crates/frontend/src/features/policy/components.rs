//! Attendance policy editor; editable for Director/HR, read-only otherwise (server is the real gate).

use leptos::{prelude::*, task};

use shared::dto::policy::{BalanceExpiryPolicy, PolicyDto, UpdatePolicyRequest};
use shared::dto::user::UserRole;
use shared::validation::policy::validate_policy;

use crate::features::policy::api;
use crate::primitives::button::{Button, ButtonVariant};
use crate::primitives::card::{Card, CardBody, CardHeader};
use crate::primitives::input::{FieldError, FieldLabel, Input};
use crate::primitives::select::Select;
use crate::primitives::stack::{Gap, Stack};
use crate::state::auth::AuthState;
use crate::state::toast::ToastState;
use crate::theme::{self, color, space, typography};
use crate::util::load::{self, Loadable};

/// Loads the current policy, then renders the editor seeded from it.
#[component]
pub fn PolicyForm() -> impl IntoView {
    let current: Loadable<PolicyDto> = RwSignal::new(None);
    Effect::new(move |_| load::load(current, api::get_policy()));

    view! {
        {move || match current.get() {
            None => load::note("Loading policy…"),
            Some(Err(e)) => load::load_error(&e),
            Some(Ok(policy)) => view! { <PolicyEditor policy=policy /> }.into_any(),
        }}
    }
}

fn field(
    label: &'static str,
    id: &'static str,
    value: RwSignal<String>,
    type_: &'static str,
    disabled: bool,
) -> impl IntoView {
    view! {
        <div>
            <FieldLabel for_id=id.to_string()>{label}</FieldLabel>
            <Input
                value=value
                on_input=Callback::new(move |v| value.set(v))
                type_=type_.to_string()
                disabled=disabled
            />
        </div>
    }
}

fn group_title(title: &'static str) -> impl IntoView {
    let cls = theme::class(format!(
        "font-size: {fs}; font-weight: {fw}; color: {c}; text-transform: uppercase; letter-spacing: 0.04em;",
        fs = typography::TEXT_LABEL,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_MUTED,
    ));
    view! { <div class=cls>{title}</div> }
}

#[component]
fn PolicyEditor(policy: PolicyDto) -> impl IntoView {
    let auth = use_context::<AuthState>().expect("AuthState context");
    let toast = use_context::<ToastState>().expect("ToastState context");
    let editable = auth.user.with(|u| {
        u.as_ref()
            .is_some_and(|x| matches!(x.role, UserRole::Director | UserRole::Hr))
    });
    let read_only = !editable;

    let workday_start = RwSignal::new(policy.workday_start.clone());
    let work_hours_per_day = RwSignal::new(policy.work_hours_per_day.to_string());
    let flex_core_start = RwSignal::new(policy.flex_core_start.clone());
    let flex_core_end = RwSignal::new(policy.flex_core_end.clone());
    let flex_daily_min = RwSignal::new(policy.flex_daily_min.to_string());
    let flex_daily_max = RwSignal::new(policy.flex_daily_max.to_string());
    let flex_earliest_start = RwSignal::new(policy.flex_earliest_start.clone());
    let flex_latest_end = RwSignal::new(policy.flex_latest_end.clone());
    let flex_max_segments = RwSignal::new(policy.flex_max_segments.to_string());
    let flex_max_per_month = RwSignal::new(policy.flex_max_per_month.to_string());
    let overtime_max = RwSignal::new(policy.overtime_max_hours_per_month.to_string());
    let carry_years = RwSignal::new(policy.balance_carry_years.to_string());
    let expiry_policy = RwSignal::new(
        match policy.balance_expiry_policy {
            BalanceExpiryPolicy::Warn => "warn",
            BalanceExpiryPolicy::RecordWorkPct => "record_work_pct",
        }
        .to_string(),
    );
    let warn_days = RwSignal::new(policy.balance_expiry_warn_days.to_string());

    let err = RwSignal::new(None::<String>);
    let saving = RwSignal::new(false);

    let submit = Callback::new(move |_| {
        if saving.get_untracked() || read_only {
            return;
        }
        err.set(None);
        let parse_f64 = |s: String, field: &str| {
            s.trim()
                .parse::<f64>()
                .map_err(|_| format!("{field} must be a number"))
        };
        let parse_u16 = |s: String, field: &str| {
            s.trim()
                .parse::<u16>()
                .map_err(|_| format!("{field} must be a whole number"))
        };
        let built = (|| {
            Ok::<UpdatePolicyRequest, String>(UpdatePolicyRequest {
                workday_start: workday_start.get_untracked(),
                work_hours_per_day: parse_f64(
                    work_hours_per_day.get_untracked(),
                    "Work hours per day",
                )?,
                flex_core_start: flex_core_start.get_untracked(),
                flex_core_end: flex_core_end.get_untracked(),
                flex_daily_min: parse_f64(flex_daily_min.get_untracked(), "Flex daily minimum")?,
                flex_daily_max: parse_f64(flex_daily_max.get_untracked(), "Flex daily maximum")?,
                flex_earliest_start: flex_earliest_start.get_untracked(),
                flex_latest_end: flex_latest_end.get_untracked(),
                flex_max_segments: parse_u16(
                    flex_max_segments.get_untracked(),
                    "Flex max segments",
                )?,
                flex_max_per_month: parse_u16(
                    flex_max_per_month.get_untracked(),
                    "Flex max per month",
                )?,
                overtime_max_hours_per_month: parse_f64(
                    overtime_max.get_untracked(),
                    "Overtime monthly cap",
                )?,
                balance_carry_years: parse_u16(carry_years.get_untracked(), "Balance carry years")?,
                balance_expiry_policy: if expiry_policy.get_untracked() == "record_work_pct" {
                    BalanceExpiryPolicy::RecordWorkPct
                } else {
                    BalanceExpiryPolicy::Warn
                },
                balance_expiry_warn_days: parse_u16(
                    warn_days.get_untracked(),
                    "Expiry warning days",
                )?,
            })
        })();
        let req = match built {
            Ok(req) => req,
            Err(message) => {
                err.set(Some(message));
                return;
            }
        };
        if let Err(e) = validate_policy(&req) {
            err.set(Some(e.to_string()));
            return;
        }
        saving.set(true);
        task::spawn_local(async move {
            match api::update_policy(&req).await {
                Ok(_) => toast.success("Attendance policy saved"),
                Err(e) => {
                    toast.error_from(&e);
                    err.set(Some(e.to_string()));
                }
            }
            saving.set(false);
        });
    });

    let grid = || {
        theme::class(format!(
            "display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: {g};",
            g = space::D4,
        ))
    };

    view! {
        <Stack gap=Gap::Lg>
            {move || read_only.then(|| view! {
                <div class=theme::class(format!("color: {c};", c = color::TEXT_MUTED))>
                    "Read-only: only HR and Director can change these limits."
                </div>
            })}
            <Card>
                <CardHeader>{group_title("General")}</CardHeader>
                <CardBody>
                    <div class=grid()>
                        {field("Workday start", "p-workday", workday_start, "time", read_only)}
                        {field("Work hours per day", "p-wh", work_hours_per_day, "number", read_only)}
                    </div>
                </CardBody>
            </Card>
            <Card>
                <CardHeader>{group_title("Flexible hours")}</CardHeader>
                <CardBody>
                    <div class=grid()>
                        {field("Core start", "p-cs", flex_core_start, "time", read_only)}
                        {field("Core end", "p-ce", flex_core_end, "time", read_only)}
                        {field("Earliest start", "p-es", flex_earliest_start, "time", read_only)}
                        {field("Latest end", "p-le", flex_latest_end, "time", read_only)}
                        {field("Daily minimum hours", "p-dmin", flex_daily_min, "number", read_only)}
                        {field("Daily maximum hours", "p-dmax", flex_daily_max, "number", read_only)}
                        {field("Max segments/day", "p-seg", flex_max_segments, "number", read_only)}
                        {field("Max flex days/month", "p-fpm", flex_max_per_month, "number", read_only)}
                    </div>
                </CardBody>
            </Card>
            <Card>
                <CardHeader>{group_title("Overtime")}</CardHeader>
                <CardBody>
                    <div class=grid()>
                        {field("Max overtime hours/month", "p-ot", overtime_max, "number", read_only)}
                    </div>
                </CardBody>
            </Card>
            <Card>
                <CardHeader>{group_title("Leave balance")}</CardHeader>
                <CardBody>
                    <div class=grid()>
                        {field("Carry years", "p-cy", carry_years, "number", read_only)}
                        {field("Expiry warning (days)", "p-wd", warn_days, "number", read_only)}
                        <div>
                            <FieldLabel for_id="p-exp".to_string()>"On expiry"</FieldLabel>
                            <Select
                                value=expiry_policy
                                on_change=Callback::new(move |v| expiry_policy.set(v))
                                disabled=read_only
                            >
                                <option value="warn">"Warn only"</option>
                                <option value="record_work_pct">"Record work %"</option>
                            </Select>
                        </div>
                    </div>
                </CardBody>
            </Card>
            {move || err.get().map(|m| view! { <FieldError message=m /> })}
            {move || (!read_only).then(|| view! {
                <Button variant=ButtonVariant::Primary on_click=submit disabled=Signal::derive(move || saving.get())>
                    {move || if saving.get() { "Saving…" } else { "Save policy" }}
                </Button>
            })}
        </Stack>
    }
}
