use leptos::prelude::*;

use crate::features::holidays::components::HolidayCalendar;
use crate::state::title;

#[component]
pub fn HolidaysPage() -> impl IntoView {
    title::set_page_title("Holidays");
    view! { <HolidayCalendar /> }
}
