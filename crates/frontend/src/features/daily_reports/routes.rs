use leptos::prelude::*;

use crate::features::daily_reports::components::{MyDay, TeamReports};
use crate::state::title;

#[component]
pub fn DailyReportPage() -> impl IntoView {
    title::set_page_title("Daily report");
    view! { <MyDay /> }
}

#[component]
pub fn TeamReportsPage() -> impl IntoView {
    title::set_page_title("Team daily reports");
    view! { <TeamReports /> }
}
