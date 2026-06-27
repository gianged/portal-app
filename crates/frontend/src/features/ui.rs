//! Small presentational helpers shared across feature pages: headings, muted captions, and the detail-page back link; style-only scoped classes over the design tokens.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::primitives::icon::{Icon, IconName};
use crate::theme::{self, color, typography};

/// Card / section heading (16px semibold strong).
#[must_use]
pub fn section_heading(text: &str) -> AnyView {
    let cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c}; margin: 0;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_BODY,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    view! { <h3 class=cls>{text.to_owned()}</h3> }.into_any()
}

/// Page title (20px semibold strong).
#[must_use]
pub fn page_title(text: &str) -> AnyView {
    let cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c}; margin: 0; \
         letter-spacing: -0.015em;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_H2,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    view! { <h2 class=cls>{text.to_owned()}</h2> }.into_any()
}

/// Muted caption / secondary line.
#[must_use]
pub fn subtle(text: &str) -> AnyView {
    let cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_CAPTION,
        c = color::TEXT_MUTED,
    ));
    view! { <div class=cls>{text.to_owned()}</div> }.into_any()
}

/// Back link with a leading chevron, for detail pages.
#[must_use]
pub fn back_link(href: &'static str, label: &str) -> AnyView {
    let cls = theme::class(format!(
        "display: inline-flex; align-items: center; gap: 4px; font-family: {ff}; \
         font-size: {fs}; color: {c}; text-decoration: none; &:hover {{ color: {a}; }}",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT_MUTED,
        a = color::ACCENT,
    ));
    let label = label.to_owned();
    view! {
        <A href=href attr:class=cls>
            <Icon name=IconName::ChevronLeft size=14 /> {label}
        </A>
    }
    .into_any()
}
