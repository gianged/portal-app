//! Leaf helpers shared by the requests index ([`super::list`]) and detail
//! ([`super::detail`]) surfaces.

use leptos::prelude::*;

use crate::theme::{class, color, typography};

pub(crate) fn heading(text: &str) -> AnyView {
    let cls = class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c}; margin: 0;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_BODY,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    view! { <h3 class=cls>{text.to_owned()}</h3> }.into_any()
}

pub(crate) fn subtle(text: &str) -> AnyView {
    let cls = class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_CAPTION,
        c = color::TEXT_MUTED,
    ));
    view! { <div class=cls>{text.to_owned()}</div> }.into_any()
}
