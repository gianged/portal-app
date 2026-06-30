use leptos::prelude::*;

use crate::features::holidays::components::HolidayCalendar;
use crate::features::home::shell::AuthedPage;

#[component]
pub fn HolidaysPage() -> impl IntoView {
    view! {
        <AuthedPage title="Holidays">
            <HolidayCalendar />
        </AuthedPage>
    }
}
