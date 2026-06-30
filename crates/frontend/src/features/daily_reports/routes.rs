use leptos::prelude::*;

use crate::features::daily_reports::components::{MyDay, TeamReports};
use crate::features::home::shell::AuthedPage;

#[component]
pub fn DailyReportPage() -> impl IntoView {
    view! {
        <AuthedPage title="Daily report">
            <MyDay />
        </AuthedPage>
    }
}

#[component]
pub fn TeamReportsPage() -> impl IntoView {
    view! {
        <AuthedPage title="Team daily reports">
            <TeamReports />
        </AuthedPage>
    }
}
