use leptos::prelude::*;

use crate::features::flex_hours::components::{Approvals, FlexHours};
use crate::state::title;

#[component]
pub fn FlexHoursPage() -> impl IntoView {
    title::set_page_title("Flexible hours");
    view! { <FlexHours /> }
}

#[component]
pub fn FlexApprovalsPage() -> impl IntoView {
    title::set_page_title("Flex approvals");
    view! { <Approvals /> }
}
