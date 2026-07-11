use leptos::{ev::SubmitEvent, prelude::*, task};
use leptos_router::{NavigateOptions, hooks};
use shared::dto::user::LoginRequest;
use shared::validation::user;

use crate::api::display::ErrorDisplay;
use crate::features::auth::api;
use crate::primitives::button::{Button, ButtonSize, ButtonVariant};
use crate::primitives::center::Center;
use crate::primitives::dots::Dots;
use crate::primitives::error::ErrorCallout;
use crate::primitives::input::{FieldError, FieldLabel, Input};
use crate::primitives::stack::{Gap, Stack};
use crate::state::auth::AuthState;
use crate::theme::{self, color, typography};

#[component]
pub fn LoginForm() -> impl IntoView {
    let email = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let email_error = RwSignal::new(None::<String>);
    let password_error = RwSignal::new(None::<String>);
    let form_error = RwSignal::new(None::<ErrorDisplay>);
    let submitting = RwSignal::new(false);

    let auth = use_context::<AuthState>().expect("AuthState context");
    let navigate = hooks::use_navigate();

    let on_submit = move |ev: SubmitEvent| {
        ev.prevent_default();
        if submitting.get() {
            return;
        }

        email_error.set(None);
        password_error.set(None);
        form_error.set(None);

        let email_val = email.get();
        let password_val = password.get();

        let mut has_error = false;
        if let Err(e) = user::validate_email(&email_val) {
            email_error.set(Some(e.to_string()));
            has_error = true;
        }
        if let Err(e) = user::validate_password(&password_val) {
            password_error.set(Some(e.to_string()));
            has_error = true;
        }
        if has_error {
            return;
        }

        submitting.set(true);
        let navigate = navigate.clone();
        task::spawn_local(async move {
            let req = LoginRequest {
                email: email_val,
                password: password_val,
            };
            match api::login(req).await {
                Ok(resp) => {
                    auth.set_user(resp.user);
                    navigate("/dashboard", NavigateOptions::default());
                }
                Err(err) => {
                    form_error.set(Some(ErrorDisplay::from(&err)));
                }
            }
            submitting.set(false);
        });
    };

    view! {
        <form on:submit=on_submit>
            <Stack gap=Gap::Lg>
                {move || form_error.get().map(|d| view! {
                    <ErrorCallout display=d />
                })}

                <div>
                    <FieldLabel for_id="email">"Email"</FieldLabel>
                    <Input
                        value=email
                        on_input=Callback::new(move |v| email.set(v))
                        type_="email"
                        placeholder="you@company.com"
                        autocomplete="email"
                    />
                    {move || email_error.get().map(|msg| view! {
                        <FieldError message=msg />
                    })}
                </div>

                <div>
                    <FieldLabel for_id="password">"Password"</FieldLabel>
                    <Input
                        value=password
                        on_input=Callback::new(move |v| password.set(v))
                        type_="password"
                        placeholder="••••••••"
                        autocomplete="current-password"
                    />
                    {move || password_error.get().map(|msg| view! {
                        <FieldError message=msg />
                    })}
                </div>

                <Button
                    variant=ButtonVariant::Primary
                    size=ButtonSize::Lg
                    type_="submit"
                    disabled=submitting
                    full_width=true
                >
                    {move || if submitting.get() {
                        view! { "Signing in"<Dots/> }.into_any()
                    } else {
                        view! { "Sign in" }.into_any()
                    }}
                </Button>
            </Stack>
        </form>
    }
}

/// Route guard: waits for session bootstrap ([`AuthState::loaded`]), then renders children for a signed-in user or redirects to `/login`.
#[component]
pub fn RequireAuth(children: ChildrenFn) -> impl IntoView {
    let auth = use_context::<AuthState>().expect("AuthState context");
    let navigate = hooks::use_navigate();

    Effect::new(move |_| {
        if auth.loaded.get() && !auth.is_authenticated() {
            navigate("/login", NavigateOptions::default());
        }
    });

    view! {
        {move || {
            if !auth.loaded.get() {
                auth_loader()
            } else if auth.is_authenticated() {
                children().into_any()
            } else {
                ().into_any()
            }
        }}
    }
}

fn auth_loader() -> AnyView {
    let cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_BODY,
        c = color::TEXT_MUTED,
    ));
    view! { <Center><span class=cls>"Loading…"</span></Center> }.into_any()
}
