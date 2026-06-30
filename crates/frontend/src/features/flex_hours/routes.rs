use leptos::prelude::*;

use crate::features::flex_hours::components::{Approvals, FlexHours};
use crate::features::home::shell::AuthedPage;

#[component]
pub fn FlexHoursPage() -> impl IntoView {
    view! {
        <AuthedPage title="Flexible hours">
            <FlexHours />
        </AuthedPage>
    }
}

#[component]
pub fn FlexApprovalsPage() -> impl IntoView {
    view! {
        <AuthedPage title="Flex approvals">
            <Approvals />
        </AuthedPage>
    }
}
