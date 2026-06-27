use leptos::prelude::*;

use crate::features::auth::components::LoginForm;
use crate::primitives::card::Card;
use crate::primitives::center::Center;
use crate::primitives::stack::{Gap, Stack};
use crate::theme::{self, color, space, typography};

#[component]
pub fn LoginPage() -> impl IntoView {
    let wrap = theme::class("width: 100%; max-width: 400px;");
    let title = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; \
         color: {c}; margin: 0; letter-spacing: -0.02em;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_H2,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    let subtitle = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c}; margin: 0;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT_MUTED,
    ));

    view! {
        <Center>
            <div class=wrap>
                <Card padding=format!("{} {}", space::D7, space::D6)>
                    <Stack gap=Gap::Xl>
                        <Stack gap=Gap::Xs>
                            <h1 class=title>"Welcome to Portal"</h1>
                            <p class=subtitle>"Sign in with your company account."</p>
                        </Stack>
                        <LoginForm />
                    </Stack>
                </Card>
            </div>
        </Center>
    }
}
