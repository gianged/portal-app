use leptos::prelude::*;

use crate::features::overtime::components::{Approvals, Overtime};
use crate::state::title;

#[component]
pub fn OvertimePage() -> impl IntoView {
    title::set_page_title("Overtime");
    view! { <Overtime /> }
}

#[component]
pub fn OvertimeApprovalsPage() -> impl IntoView {
    title::set_page_title("Overtime approvals");
    view! { <Approvals /> }
}
