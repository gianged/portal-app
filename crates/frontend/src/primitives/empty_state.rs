use leptos::prelude::*;

use crate::primitives::icon::{Icon, IconName};
use crate::theme::{self, color, space, typography};

/// Centered placeholder for empty lists, not-yet-built areas, and zero-result
/// states: an icon, a title, a line of guidance, and an optional action slot.
#[component]
pub fn EmptyState(
    #[prop(optional)] icon: Option<IconName>,
    #[prop(into)] title: String,
    #[prop(optional, into)] description: Option<String>,
    #[prop(optional)] action: Option<AnyView>,
) -> impl IntoView {
    let wrap = theme::class(format!(
        "display: flex; flex-direction: column; align-items: center; justify-content: center; \
         text-align: center; gap: {g}; padding: {p}; color: {c};",
        g = space::D3,
        p = space::D8,
        c = color::TEXT_MUTED,
    ));
    let icon_wrap = theme::class(format!(
        "display: inline-flex; align-items: center; justify-content: center; \
         width: 44px; height: 44px; border-radius: 50%; background: {bg}; color: {c};",
        bg = color::BG_SUNKEN,
        c = color::TEXT_FAINT,
    ));
    let title_cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c}; margin: 0;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_H3,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    let desc_cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c}; margin: 0; max-width: 420px;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT_MUTED,
    ));
    view! {
        <div class=wrap>
            {icon.map(|name| view! { <span class=icon_wrap.clone()><Icon name=name size=22 /></span> })}
            <h3 class=title_cls>{title}</h3>
            {description.map(|d| view! { <p class=desc_cls>{d}</p> })}
            {action}
        </div>
    }
}
