use leptos::prelude::*;
use leptos::task::spawn_local;
use time::OffsetDateTime;

use shared::dto::report::{GroupReportRowDto, MonthlyReportDto};

use crate::features::reports::api;
use crate::features::reports::routes::{metric, month_name, section_title};
use crate::primitives::button::{Button, ButtonSize, ButtonVariant};
use crate::primitives::card::Card;
use crate::primitives::chart::{BarChart, ProgressBar};
use crate::primitives::cluster::Cluster;
use crate::primitives::icon::{Icon, IconName};
use crate::primitives::select::Select;
use crate::primitives::stack::{Gap, Stack};
use crate::state::toast::ToastState;
use crate::theme::{class, color, space, typography};
use crate::util::load::{Loadable, load, load_error, note};

#[component]
pub fn MonthlyTab() -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let now = OffsetDateTime::now_utc();
    let year = RwSignal::new(now.year());
    let month = RwSignal::new(u8::from(now.month()));
    let report: Loadable<MonthlyReportDto> = RwSignal::new(None);
    let download = RwSignal::new(None::<String>);
    let generating = RwSignal::new(false);

    Effect::new(move |_| {
        download.set(None);
        load(report, api::monthly(year.get(), month.get()));
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
    let years: Vec<i32> = {
        let cur = now.year();
        (cur - 5..=cur).rev().collect()
    };

    let on_download = Callback::new(move |_| {
        if generating.get_untracked() {
            return;
        }
        generating.set(true);
        let (y, m) = (year.get_untracked(), month.get_untracked());
        spawn_local(async move {
            match api::generate_monthly(y, m).await {
                Ok(summary) => download.set(Some(summary.download_url)),
                Err(e) => toast.error_from(&e),
            }
            generating.set(false);
        });
    });

    let month_wrap = class("width: 150px;");
    let year_wrap = class("width: 120px;");
    let link_cls = class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c}; font-weight: {fw};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::ACCENT,
        fw = typography::WEIGHT_MEDIUM,
    ));

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
                <Button
                    variant=ButtonVariant::Primary
                    size=ButtonSize::Sm
                    on_click=on_download
                    disabled=generating.get()
                >
                    <Icon name=IconName::Doc size=14 />
                    {move || if generating.get() { " Generating…" } else { " Generate PDF" }}
                </Button>
                {move || {
                    download
                        .get()
                        .map(|url| view! { <a class=link_cls.clone() href=url target="_blank" rel="noopener">"Download PDF"</a> })
                }}
            </Cluster>

            {move || match report.get() {
                None => note("Loading report…"),
                Some(Err(e)) => load_error(&e),
                Some(Ok(data)) => monthly_view(data),
            }}
        </Stack>
    }
}

fn monthly_view(data: MonthlyReportDto) -> AnyView {
    let staff = data.staff;
    let tickets = data.tickets;
    let groups = data.groups;
    let by_cat: Vec<(String, f64)> = tickets
        .by_category
        .iter()
        .map(|c| (c.label.clone(), f64::from(c.count)))
        .collect();
    let avg_resolve = tickets
        .avg_resolve_hours
        .map_or_else(|| "—".to_owned(), |h| format!("{h:.1}"));

    view! {
        <Stack gap=Gap::Lg>
            <Card>
                <Stack gap=Gap::Md>
                    {section_title("Staff")}
                    <Cluster gap=Gap::Xl>
                        {metric("Company headcount", staff.company_headcount.to_string())}
                        {metric("New joiners", staff.new_joiners.to_string())}
                        {metric("Deactivations", staff.deactivations.to_string())}
                    </Cluster>
                </Stack>
            </Card>
            <Card>
                <Stack gap=Gap::Md>
                    {section_title("IT tickets")}
                    <Cluster gap=Gap::Xl>
                        {metric("Created", tickets.created_in_period.to_string())}
                        {metric("Resolved", tickets.resolved_in_period.to_string())}
                        {metric("Avg resolve (h)", avg_resolve)}
                    </Cluster>
                    <BarChart data=by_cat height=160 />
                </Stack>
            </Card>
            <Card>
                <Stack gap=Gap::Sm>
                    {section_title("Groups")}
                    {groups.into_iter().map(group_row).collect_view()}
                </Stack>
            </Card>
        </Stack>
    }
    .into_any()
}

fn group_row(g: GroupReportRowDto) -> impl IntoView {
    let row = class(format!(
        "display: grid; grid-template-columns: 1.4fr 1fr 1.1fr 0.8fr 1.3fr 0.8fr; \
         gap: {gap}; align-items: center; padding: {p} 0; border-bottom: 1px solid {b};",
        gap = space::D3,
        p = space::D2,
        b = color::BORDER,
    ));
    let name = class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    let cell = class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT_MUTED,
    ));
    let progress = g.avg_project_progress;
    let bar_wrap = class("display: flex; align-items: center; gap: 6px;");

    view! {
        <div class=row>
            <span class=name>{g.group_name}</span>
            <span class=cell.clone()>{format!("{}/{} done", g.projects_completed, g.projects_total)}</span>
            <div class=bar_wrap>
                <ProgressBar value=Signal::derive(move || progress) />
                <span class=cell.clone()>{format!("{progress}%")}</span>
            </div>
            <span class=cell.clone()>{format!("{} stuck", g.projects_stuck)}</span>
            <span class=cell.clone()>
                {format!("{}/{} ({}%)", g.requests_completed, g.requests_total, g.request_completion_pct)}
            </span>
            <span class=cell>{format!("{} staff", g.headcount)}</span>
        </div>
    }
}
