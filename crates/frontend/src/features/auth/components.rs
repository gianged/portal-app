use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::NavigateOptions;
use leptos_router::hooks::use_navigate;
use shared::dto::user::LoginRequest;
use shared::validation::user::{validate_email, validate_password};

use crate::features::auth::api;
use crate::primitives::button::{Button, ButtonSize, ButtonVariant};
use crate::primitives::center::Center;
use crate::primitives::input::{FieldError, FieldLabel, Input};
use crate::primitives::stack::{Gap, Stack};
use crate::state::auth::AuthState;
use crate::theme::{class, color, radius, space, typography};

#[component]
pub fn LoginForm() -> impl IntoView {
    let email = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let email_error = RwSignal::new(None::<String>);
    let password_error = RwSignal::new(None::<String>);
    let form_error = RwSignal::new(None::<String>);
    let submitting = RwSignal::new(false);

    let auth = use_context::<AuthState>().expect("AuthState context");
    let navigate = use_navigate();

    let banner_cls = class(format!(
        "background: {bg}; color: {c}; border: 1px solid {b}; \
         border-radius: {r}; padding: {p}; font-size: {fs};",
        bg = color::DANGER_BG,
        c = color::DANGER,
        b = color::DANGER_BORDER,
        r = radius::MD,
        p = space::D3,
        fs = typography::TEXT_SMALL,
    ));

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
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
        if let Err(e) = validate_email(&email_val) {
            email_error.set(Some(e.to_string()));
            has_error = true;
        }
        if let Err(e) = validate_password(&password_val) {
            password_error.set(Some(e.to_string()));
            has_error = true;
        }
        if has_error {
            return;
        }

        submitting.set(true);
        let navigate = navigate.clone();
        spawn_local(async move {
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
                    form_error.set(Some(err.to_string()));
                }
            }
            submitting.set(false);
        });
    };

    view! {
        <form on:submit=on_submit>
            <Stack gap=Gap::Lg>
                {move || form_error.get().map(|msg| view! {
                    <div class=banner_cls.clone() role="alert">{msg}</div>
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
                    disabled=submitting.get()
                    full_width=true
                >
                    {move || if submitting.get() { "Signing in…" } else { "Sign in" }}
                </Button>
            </Stack>
        </form>
    }
}

/// Route guard for authenticated pages. Waits for the session bootstrap to
/// resolve ([`AuthState::loaded`]); once resolved it renders `children` for a
/// signed-in user, otherwise redirects to `/login`.
#[component]
pub fn RequireAuth(children: ChildrenFn) -> impl IntoView {
    let auth = use_context::<AuthState>().expect("AuthState context");
    let navigate = use_navigate();

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
    let cls = class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_BODY,
        c = color::TEXT_MUTED,
    ));
    view! { <Center><span class=cls>"Loading…"</span></Center> }.into_any()
}
