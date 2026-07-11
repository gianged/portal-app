#![allow(dead_code)]

use leptos::prelude::*;

use crate::primitives::avatar::{Avatar, AvatarSize};
use crate::theme::{self, color, typography};
use crate::util::format;

/// A row of overlapping [`Avatar`]s with a trailing `+N` chip once the list
/// exceeds `max`. Tones are derived per-name so a person keeps a stable color.
#[component]
pub fn AvatarStack(
    names: Vec<String>,
    #[prop(optional)] max: Option<usize>,
    #[prop(optional)] size: AvatarSize,
) -> impl IntoView {
    let max = max.unwrap_or(4);
    let total = names.len();
    let overflow = total.saturating_sub(max);

    let wrap = theme::class(format!(
        "display: inline-flex; \
         & > span {{ margin-left: -6px; box-shadow: 0 0 0 2px {bg}; border-radius: 50%; }} \
         & > span:first-child {{ margin-left: 0; }}",
        bg = color::BG,
    ));
    let visible = names.into_iter().take(max).map(move |name| {
        let tone = format::tone_for(&name);
        view! { <Avatar name=name tone=tone size=size /> }
    });

    let overflow_chip = (overflow > 0).then(|| {
        let dim = size.dimension();
        let fs = size.font_size();
        let cls = theme::class(format!(
            "display: inline-flex; align-items: center; justify-content: center; \
             width: {dim}; height: {dim}; background: {bg}; color: {c}; \
             font-family: {ff}; font-size: {fs}; font-weight: {fw};",
            bg = color::BG_SUNKEN,
            c = color::TEXT_MUTED,
            ff = typography::FONT_SANS,
            fw = typography::WEIGHT_SEMIBOLD,
        ));
        view! { <span class=cls>{format!("+{overflow}")}</span> }
    });

    view! {
        <span class=wrap>
            {visible.collect_view()}
            {overflow_chip}
        </span>
    }
}
