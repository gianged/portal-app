//! A themed error block: severity icon + Title / code / message / reference.
//! Fed an [`ErrorDisplay`]; used for inline load failures and form banners.

use leptos::prelude::*;

use crate::api::display::{ErrorDisplay, Severity};
use crate::primitives::icon::{Icon, IconName};
use crate::theme::{class, color, radius, space, typography};

#[component]
pub fn ErrorCallout(display: ErrorDisplay) -> impl IntoView {
    let (fg, bg, border, icon) = match display.severity {
        Severity::Danger => (
            color::DANGER,
            color::DANGER_BG,
            color::DANGER_BORDER,
            IconName::AlertCircle,
        ),
        Severity::Warning => (
            color::WARNING,
            color::WARNING_BG,
            color::WARNING_BORDER,
            IconName::AlertTriangle,
        ),
        Severity::Info => (
            color::INFO,
            color::INFO_BG,
            color::INFO_BORDER,
            IconName::Info,
        ),
    };
    let wrap = class(format!(
        "display: flex; align-items: flex-start; gap: {g}; padding: {py} {px}; \
         background: {bg}; border: 1px solid {border}; border-radius: {r}; \
         font-family: {ff}; font-size: {fs}; line-height: 1.45; color: {fg};",
        g = space::D3,
        py = space::D3,
        px = space::D4,
        r = radius::MD,
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
    ));
    let body = class("flex: 1; min-width: 0;");
    let title = class(format!(
        "font-weight: {fw};",
        fw = typography::WEIGHT_SEMIBOLD
    ));
    let chip = class(format!(
        "display: inline-block; margin: {mt} 0; font-family: {ff}; font-size: {fs}; \
         opacity: 0.85;",
        mt = space::D1,
        ff = typography::FONT_MONO,
        fs = typography::TEXT_CAPTION,
    ));
    let msg = class(format!("color: {c};", c = color::TEXT));
    let reference = class(format!(
        "margin-top: {mt}; font-family: {ff}; font-size: {fs}; color: {c};",
        mt = space::D1,
        ff = typography::FONT_MONO,
        fs = typography::TEXT_CAPTION,
        c = color::TEXT_FAINT,
    ));
    view! {
        <div class=wrap role="alert">
            <Icon name=icon size=16 />
            <div class=body>
                <div class=title>{display.title}</div>
                {display.code.map(|c| view! { <div class=chip>{c}</div> })}
                <div class=msg>{display.message}</div>
                {display
                    .request_id
                    .map(|id| view! { <div class=reference>{format!("Reference: {id}")}</div> })}
            </div>
        </div>
    }
}
