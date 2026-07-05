use leptos::{prelude::*, task};
use time::OffsetDateTime;

use shared::dto::report::YearlyReportDto;

use crate::features::reports::api;
use crate::features::reports::routes::{metric, month_abbr, section_title};
use crate::primitives::button::{Button, ButtonSize, ButtonVariant};
use crate::primitives::card::Card;
use crate::primitives::chart::{LineChart, Series, series_color};
use crate::primitives::cluster::Cluster;
use crate::primitives::icon::{Icon, IconName};
use crate::primitives::select::Select;
use crate::primitives::stack::{Gap, Stack};
use crate::state::toast::ToastState;
use crate::theme::{self, color, typography};
use crate::util::load::{self, Loadable};

#[component]
pub fn YearlyTab() -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let now = OffsetDateTime::now_utc();
    let year = RwSignal::new(now.year());
    let report: Loadable<YearlyReportDto> = RwSignal::new(None);
    let download = RwSignal::new(None::<String>);
    let generating = RwSignal::new(false);

    Effect::new(move |_| {
        download.set(None);
        load::load(report, api::yearly(year.get()));
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
        let y = year.get_untracked();
        task::spawn_local(async move {
            match api::generate_yearly(y).await {
                Ok(summary) => download.set(Some(summary.download_url)),
                Err(e) => toast.error_from(&e),
            }
            generating.set(false);
        });
    });

    let year_wrap = theme::class("width: 120px;");
    let link_cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c}; font-weight: {fw};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::ACCENT,
        fw = typography::WEIGHT_MEDIUM,
    ));

    view! {
        <Stack gap=Gap::Lg>
            <Cluster gap=Gap::Sm>
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
                    disabled=Signal::derive(move || generating.get())
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
                None => load::note("Loading report…"),
                Some(Err(e)) => load::load_error(&e),
                Some(Ok(data)) => yearly_view(&data),
            }}
        </Stack>
    }
}

fn month_labels() -> Vec<String> {
    (1..=12u8).map(|m| month_abbr(m).to_owned()).collect()
}

fn yearly_view(data: &YearlyReportDto) -> AnyView {
    let totals = data.totals;
    let labels = month_labels();
    let labels_activity = labels.clone();

    let headcount: Vec<f64> = data
        .growth
        .headcount
        .iter()
        .map(|p| p.value as f64)
        .collect();
    let tickets: Vec<f64> = data
        .growth
        .tickets_created
        .iter()
        .map(|p| p.value as f64)
        .collect();
    let projects: Vec<f64> = data
        .growth
        .projects_completed
        .iter()
        .map(|p| p.value as f64)
        .collect();
    let requests: Vec<f64> = data
        .growth
        .requests_completed
        .iter()
        .map(|p| p.value as f64)
        .collect();

    let headcount_series = vec![Series {
        label: "Headcount".to_owned(),
        points: headcount,
        color: series_color(0),
    }];
    let activity_series = vec![
        Series {
            label: "Tickets".to_owned(),
            points: tickets,
            color: series_color(2),
        },
        Series {
            label: "Projects done".to_owned(),
            points: projects,
            color: series_color(1),
        },
        Series {
            label: "Requests done".to_owned(),
            points: requests,
            color: series_color(0),
        },
    ];

    view! {
        <Stack gap=Gap::Lg>
            <Card>
                <Stack gap=Gap::Md>
                    {section_title("Headline")}
                    <Cluster gap=Gap::Xl>
                        {metric("Headcount", totals.company_headcount.to_string())}
                        {metric("Net change", format!("{:+}", totals.net_headcount_change))}
                        {metric("New hires", totals.new_hires.to_string())}
                        {metric("Departures", totals.departures.to_string())}
                        {metric("Tickets", totals.tickets_created.to_string())}
                        {metric("Projects done", totals.projects_completed.to_string())}
                        {metric("Requests done", totals.requests_completed.to_string())}
                    </Cluster>
                </Stack>
            </Card>
            <Card>
                <Stack gap=Gap::Md>
                    {section_title("Headcount growth (cumulative net)")}
                    <LineChart series=headcount_series x_labels=labels height=240 />
                </Stack>
            </Card>
            <Card>
                <Stack gap=Gap::Md>
                    {section_title("Activity over the year")}
                    <LineChart series=activity_series x_labels=labels_activity height=240 />
                </Stack>
            </Card>
        </Stack>
    }
    .into_any()
}
