//! Holiday calendar: a year view with HR add/remove. Non-HR users see a read-only
//! list (the server is the real gate).

use leptos::{prelude::*, task};

use shared::dto::holiday::{HolidayDto, SetHolidayRequest};
use shared::dto::user::UserRole;
use shared::validation::holiday::validate_holiday;

use crate::features::holidays::api;
use crate::primitives::button::{Button, ButtonSize, ButtonVariant};
use crate::primitives::card::Card;
use crate::primitives::input::{FieldError, FieldLabel, Input};
use crate::primitives::stack::{Gap, Stack};
use crate::state::auth::AuthState;
use crate::state::toast::ToastState;
use crate::theme::{self, color, space, typography};
use crate::util::load::{self, Loadable};

fn current_year() -> i32 {
    web_sys::js_sys::Date::new_0().get_full_year() as i32
}

#[component]
pub fn HolidayCalendar() -> impl IntoView {
    let auth = use_context::<AuthState>().expect("AuthState context");
    let toast = use_context::<ToastState>().expect("ToastState context");
    let editable = auth
        .user
        .with(|u| u.as_ref().is_some_and(|x| matches!(x.role, UserRole::Hr)));

    let year = RwSignal::new(current_year());
    let holidays: Loadable<Vec<HolidayDto>> = RwSignal::new(None);
    let tick = RwSignal::new(0u32);

    Effect::new(move |_| {
        let _ = tick.get();
        let y = year.get();
        load::load(holidays, async move { api::list_year(y).await });
    });

    let new_date = RwSignal::new(String::new());
    let new_name = RwSignal::new(String::new());
    let err = RwSignal::new(None::<String>);
    let saving = RwSignal::new(false);

    let add = Callback::new(move |_| {
        if saving.get_untracked() {
            return;
        }
        err.set(None);
        let date = new_date.get_untracked();
        if date.trim().is_empty() {
            err.set(Some("Pick a date".into()));
            return;
        }
        let req = SetHolidayRequest {
            name: new_name.get_untracked(),
        };
        if let Err(e) = validate_holiday(&req) {
            err.set(Some(e.to_string()));
            return;
        }
        saving.set(true);
        task::spawn_local(async move {
            match api::set(&date, &req).await {
                Ok(_) => {
                    toast.success("Holiday saved");
                    new_date.set(String::new());
                    new_name.set(String::new());
                    tick.update(|n| *n += 1);
                }
                Err(e) => {
                    toast.error_from(&e);
                    err.set(Some(e.to_string()));
                }
            }
            saving.set(false);
        });
    });

    let remove = move |date: String| {
        task::spawn_local(async move {
            match api::remove(&date).await {
                Ok(()) => {
                    toast.success("Holiday removed");
                    tick.update(|n| *n += 1);
                }
                Err(e) => toast.error_from(&e),
            }
        });
    };

    let head = theme::class(format!(
        "display: flex; align-items: flex-end; gap: {g}; flex-wrap: wrap;",
        g = space::D4,
    ));
    let small = theme::class("max-width: 140px;");
    let row = theme::class(format!(
        "display: flex; align-items: center; justify-content: space-between; gap: {g};",
        g = space::D3,
    ));
    let muted = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT_MUTED,
    ));
    let strong = theme::class(format!(
        "font-family: {ff}; font-weight: {fw}; color: {c};",
        ff = typography::FONT_SANS,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));

    view! {
        <Stack gap=Gap::Lg>
            <div class=head>
                <div class=small.clone()>
                    <FieldLabel for_id="hol-year".to_string()>"Year"</FieldLabel>
                    <Input
                        value=Signal::derive(move || year.get().to_string())
                        on_input=Callback::new(move |v: String| {
                            if let Ok(y) = v.trim().parse::<i32>() { year.set(y); }
                        })
                        type_="number".to_string()
                    />
                </div>
            </div>

            {
                let muted = muted.clone();
                move || editable.then(|| {
                let muted = muted.clone();
                view! {
                    <Card>
                        <Stack gap=Gap::Md>
                            <div class=muted.clone()>"Add a public holiday"</div>
                            <div class=theme::class(format!(
                                "display: grid; grid-template-columns: 160px 1fr auto; gap: {g}; align-items: end;",
                                g = space::D3,
                            ))>
                                <div>
                                    <FieldLabel for_id="hol-date".to_string()>"Date"</FieldLabel>
                                    <Input value=new_date on_input=Callback::new(move |v| new_date.set(v)) type_="date".to_string() />
                                </div>
                                <div>
                                    <FieldLabel for_id="hol-name".to_string()>"Name"</FieldLabel>
                                    <Input value=new_name on_input=Callback::new(move |v| new_name.set(v)) placeholder="New Year's Day".to_string() />
                                </div>
                                <Button variant=ButtonVariant::Primary on_click=add disabled=Signal::derive(move || saving.get())>
                                    "Add"
                                </Button>
                            </div>
                            {move || err.get().map(|m| view! { <FieldError message=m /> })}
                        </Stack>
                    </Card>
                }
            })}

            {move || match holidays.get() {
                None => load::note("Loading holidays…"),
                Some(Err(e)) => load::load_error(&e),
                Some(Ok(list)) if list.is_empty() => load::note("No holidays for this year."),
                Some(Ok(list)) => {
                    let row = row.clone();
                    let muted = muted.clone();
                    let strong = strong.clone();
                    view! {
                        <Stack gap=Gap::Sm>
                            {list.into_iter().map(|h| {
                                let date = h.date.clone();
                                let row = row.clone();
                                let muted = muted.clone();
                                let strong = strong.clone();
                                view! {
                                    <Card>
                                        <div class=row>
                                            <div>
                                                <span class=strong>{h.name}</span>
                                                <span class=muted>{format!("  ·  {}", h.date)}</span>
                                            </div>
                                            {editable.then(|| {
                                                let date = date.clone();
                                                view! {
                                                    <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm
                                                        on_click=Callback::new(move |_| remove(date.clone()))>
                                                        "Remove"
                                                    </Button>
                                                }
                                            })}
                                        </div>
                                    </Card>
                                }
                            }).collect_view()}
                        </Stack>
                    }.into_any()
                }
            }}
        </Stack>
    }
}
