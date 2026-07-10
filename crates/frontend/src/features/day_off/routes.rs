use leptos::prelude::*;

use crate::features::day_off::components::{Approvals, TimeOff};
use crate::state::title;

#[component]
pub fn TimeOffPage() -> impl IntoView {
    title::set_page_title("Time off");
    view! { <TimeOff /> }
}

#[component]
pub fn LeaveApprovalsPage() -> impl IntoView {
    title::set_page_title("Leave approvals");
    view! { <Approvals /> }
}
