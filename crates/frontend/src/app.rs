use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

use crate::features::auth::routes::LoginPage;
use crate::features::home::routes::{DashboardPage, LandingPage};
use crate::state::auth::AuthState;
use crate::theme::{color, typography};

#[component]
pub fn App() -> impl IntoView {
    provide_context(AuthState::new());

    let global_css = format!(
        "html, body {{ margin: 0; padding: 0; background: {bg}; color: {fg}; \
         font-family: {ff}; font-size: 14px; line-height: 1.5; \
         -webkit-font-smoothing: antialiased; -moz-osx-font-smoothing: grayscale; }} \
         * {{ box-sizing: border-box; }} \
         a {{ color: {accent}; text-decoration: none; }} \
         a:hover {{ color: {ah}; }} \
         button, input, select, textarea {{ font: inherit; color: inherit; }}",
        bg = color::BG,
        fg = color::TEXT,
        ff = typography::FONT_SANS,
        accent = color::ACCENT,
        ah = color::ACCENT_HOVER,
    );

    view! {
        <style>{global_css}</style>
        <Router>
            <Routes fallback=NotFound>
                <Route path=path!("/") view=LandingPage />
                <Route path=path!("/login") view=LoginPage />
                <Route path=path!("/dashboard") view=DashboardPage />
            </Routes>
        </Router>
    }
}

#[component]
fn NotFound() -> impl IntoView {
    view! {
        <div style="padding: 48px; text-align: center;">
            <h1>"404"</h1>
            <p>"That page doesn't exist."</p>
            <a href="/">"Go home"</a>
        </div>
    }
}
