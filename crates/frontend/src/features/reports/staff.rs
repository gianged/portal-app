use leptos::prelude::*;
use time::OffsetDateTime;
use uuid::Uuid;

use shared::dto::report::StaffMonthlyReportDto;
use shared::dto::user::UserRole;

use crate::features::reports::api;
use crate::features::reports::routes::{metric, month_name, section_title};
use crate::primitives::card::Card;
use crate::primitives::chart::{BarChart, ProgressBar};
use crate::primitives::cluster::Cluster;
use crate::primitives::input::{FieldLabel, Input};
use crate::primitives::select::Select;
use crate::primitives::stack::{Gap, Stack};
use crate::state::auth::AuthState;
use crate::theme;
use crate::util::load::{self, Loadable};

#[component]
pub fn StaffMonthlyTab() -> impl IntoView {
    let auth = use_context::<AuthState>().expect("AuthState context");
    let now = OffsetDateTime::now_utc();
    let year = RwSignal::new(now.year());
    let month = RwSignal::new(u8::from(now.month()));

    // Default to the viewer's own report.
    let self_id = auth
        .user
        .with_untracked(|u| u.as_ref().map(|u| u.id.0.to_string()))
        .unwrap_or_default();
    let user_input = RwSignal::new(self_id);
    let report: Loadable<StaffMonthlyReportDto> = RwSignal::new(None);

    // Leaders / HR / Directors may pull another user's report; the server gates it regardless.
    let can_pick = Signal::derive(move || {
        auth.user.with(|u| {
            u.as_ref().is_some_and(|u| {
                matches!(
                    u.role,
                    UserRole::Director
                        | UserRole::Hr
                        | UserRole::GroupLeader
                        | UserRole::GroupSubLeader
                )
            })
        })
    });

    Effect::new(move |_| {
        let (y, m) = (year.get(), month.get());
        match Uuid::parse_str(user_input.get().trim()) {
            Ok(uid) => load::load(report, api::staff_monthly(uid, y, m)),
            Err(_) => report.set(None),
        }
    });

    let month_value = Signal::derive(move || month.get().to_string());
    let on_month = Callback::new(move |s: String| {
        if let Ok(m) = s.parse::<u8>() {
            month.set(m);
        }
    });
    let year_value = Signal::derive(move || year.get().to_string());
    let on_year = Callback::new(move |s: String| {
        if let Ok(y) = s.parse::<i32>() {
            year.set(y);
        }
    });
    let on_user = Callback::new(move |s: String| user_input.set(s));
    let years: Vec<i32> = {
        let cur = now.year();
        (cur - 5..=cur).rev().collect()
    };

    let month_wrap = theme::class("width: 150px;");
    let year_wrap = theme::class("width: 120px;");
    let user_wrap = theme::class("width: 320px;");

    view! {
        <Stack gap=Gap::Lg>
            <Cluster gap=Gap::Sm>
                <div class=month_wrap>
                    <Select value=month_value on_change=on_month>
                        {(1..=12u8)
                            .map(|m| view! { <option value=m.to_string()>{month_name(m)}</option> })
                            .collect_view()}
                    </Select>
                </div>
                <div class=year_wrap>
                    <Select value=year_value on_change=on_year>
                        {years
                            .into_iter()
                            .map(|y| view! { <option value=y.to_string()>{y}</option> })
                            .collect_view()}
                    </Select>
                </div>
                {move || {
                    can_pick
                        .get()
                        .then(|| {
                            view! {
                                <div class=user_wrap.clone()>
                                    <FieldLabel for_id="staff-report-user">"User ID"</FieldLabel>
                                    <Input
                                        value=Signal::derive(move || user_input.get())
                                        on_input=on_user
                                        placeholder="user id (UUID)"
                                    />
                                </div>
                            }
                        })
                }}
            </Cluster>

            {move || match report.get() {
                None => load::note("Enter a valid user and pick a month."),
                Some(Err(e)) => load::load_error(&e),
                Some(Ok(data)) => staff_view(data),
            }}
        </Stack>
    }
}

fn staff_view(data: StaffMonthlyReportDto) -> AnyView {
    let hours = vec![
        ("Request work".to_owned(), data.hours_request_work),
        ("Learning".to_owned(), data.hours_learning),
        ("Other".to_owned(), data.hours_other),
    ];
    let leave = data.leave_days_by_kind.clone();
    let progress = data.avg_request_progress;
    let flex_delta = format!("{:+.1}h", data.flex_month_delta);

    view! {
        <Stack gap=Gap::Lg>
            <Card>
                <Stack gap=Gap::Md>
                    {section_title("Hours by category")}
                    <Cluster gap=Gap::Xl>
                        {metric("Days reported", data.days_reported.to_string())}
                        {metric("Request work (h)", format!("{:.1}", data.hours_request_work))}
                        {metric("Learning (h)", format!("{:.1}", data.hours_learning))}
                        {metric("Other (h)", format!("{:.1}", data.hours_other))}
                    </Cluster>
                    <BarChart data=hours height=160 />
                </Stack>
            </Card>

            <Card>
                <Stack gap=Gap::Md>
                    {section_title("Attendance")}
                    <Cluster gap=Gap::Xl>
                        {metric("Work %", format!("{}%", data.work_percentage))}
                        {metric("Overtime (h)", format!("{:.1}", data.overtime_hours))}
                        {metric("Flex days", data.flex_days.to_string())}
                        {metric("Flex delta", flex_delta)}
                    </Cluster>
                    {(!leave.is_empty())
                        .then(|| {
                            view! {
                                <Cluster gap=Gap::Xl>
                                    {leave
                                        .into_iter()
                                        .map(|l| metric(l.kind.label(), format!("{:.1} d", l.days)))
                                        .collect_view()}
                                </Cluster>
                            }
                        })}
                </Stack>
            </Card>

            <Card>
                <Stack gap=Gap::Md>
                    {section_title("Leave balance")}
                    <Cluster gap=Gap::Xl>
                        {metric("Remaining", format!("{:.1} d", data.balance_remaining))}
                        {metric("Expiring soon", format!("{:.1} d", data.balance_expiring_soon))}
                    </Cluster>
                </Stack>
            </Card>

            <Card>
                <Stack gap=Gap::Md>
                    {section_title("Requests")}
                    <Cluster gap=Gap::Xl>
                        {metric("Completed", data.requests_completed.to_string())}
                        {metric("Open", data.requests_open.to_string())}
                        {metric("Avg progress", format!("{progress}%"))}
                    </Cluster>
                    <ProgressBar value=Signal::derive(move || progress) />
                </Stack>
            </Card>
        </Stack>
    }
    .into_any()
}
