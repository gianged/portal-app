use leptos::prelude::*;

use crate::features::home::shell::AuthedPage;
use crate::features::policy::components::PolicyForm;

#[component]
pub fn PolicyPage() -> impl IntoView {
    view! {
        <AuthedPage title="Attendance policy">
            <PolicyForm />
        </AuthedPage>
    }
}
