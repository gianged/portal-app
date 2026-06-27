//! The reusable [`UserPicker`]: a `<select>` of active users for choosing a person (assign a request/ticket, add a group member, start a DM).

use leptos::prelude::*;
use uuid::Uuid;

use shared::dto::ids::UserId;
use shared::dto::user::UserDto;

use crate::features::users::api;
use crate::primitives::select::Select;
use crate::util::load::{Loadable, load};

/// A `<select>` of active users; loads the directory once and yields the chosen [`UserId`] via `on_select`.
#[component]
pub fn UserPicker(
    #[prop(into)] selected: Signal<Option<UserId>>,
    on_select: Callback<UserId>,
    #[prop(optional, into)] placeholder: Option<String>,
) -> impl IntoView {
    let placeholder = placeholder.unwrap_or_else(|| "Select a person…".to_owned());
    let users: Loadable<Vec<UserDto>> = RwSignal::new(None);
    load(users, api::list(None));

    let value = Signal::derive(move || selected.get().map(|u| u.0.to_string()).unwrap_or_default());
    let handle = Callback::new(move |s: String| {
        if let Ok(uuid) = Uuid::parse_str(&s) {
            on_select.run(UserId(uuid));
        }
    });

    view! {
        <Select value=value on_change=handle>
            <option value="">{placeholder}</option>
            {move || {
                users.get().and_then(Result::ok).map(|list| {
                    list.into_iter()
                        .map(|u| {
                            let id = u.id.0.to_string();
                            view! { <option value=id>{u.name}</option> }
                        })
                        .collect_view()
                })
            }}
        </Select>
    }
}
