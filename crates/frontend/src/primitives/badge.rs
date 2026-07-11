use leptos::prelude::*;

use crate::theme::{self, color, radius, space, typography};

/// Status / category pill. Variants map to the semantic color tokens, so a badge
/// reskins correctly in dark mode. Use [`crate::util::format`] mappers to derive a
/// variant from a domain enum (request/ticket/project status, priority).
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum BadgeVariant {
    #[default]
    Neutral,
    Accent,
    Success,
    Warning,
    Danger,
}

impl BadgeVariant {
    /// `(background, foreground, border)`.
    fn colors(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Neutral => (color::BG_SUNKEN, color::TEXT_MUTED, color::BORDER),
            Self::Accent => (color::ACCENT_BG, color::ACCENT, color::ACCENT_BORDER),
            Self::Success => (color::SUCCESS_BG, color::SUCCESS, color::SUCCESS_BORDER),
            Self::Warning => (color::WARNING_BG, color::WARNING, color::WARNING_BORDER),
            Self::Danger => (color::DANGER_BG, color::DANGER, color::DANGER_BORDER),
        }
    }
}

#[component]
pub fn Badge(
    #[prop(optional)] variant: BadgeVariant,
    #[prop(optional)] dot: bool,
    children: Children,
) -> impl IntoView {
    let (bg, fg, border) = variant.colors();
    let cls = theme::class(format!(
        "display: inline-flex; align-items: center; gap: {g}; height: 22px; \
         padding: 0 {px}; font-family: {ff}; font-size: {fs}; font-weight: {fw}; \
         border-radius: {r}; background: {bg}; color: {fg}; border: 1px solid {border}; \
         line-height: 1; letter-spacing: 0.005em; white-space: nowrap;",
        g = space::D1,
        px = space::D2,
        ff = typography::FONT_SANS,
        fs = typography::TEXT_BADGE,
        fw = typography::WEIGHT_MEDIUM,
        r = radius::PILL,
    ));
    let dot_cls = theme::class(
        "width: 6px; height: 6px; border-radius: 50%; background: currentColor; flex-shrink: 0;",
    );
    view! {
        <span class=cls>
            {dot.then(|| view! { <span class=dot_cls.clone()></span> })}
            {children()}
        </span>
    }
}
