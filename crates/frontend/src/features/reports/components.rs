//! Shared presentational helpers for the report tabs.

use leptos::prelude::*;

use crate::theme::{self, color, typography};

/// A small label + figure tile used across the report tabs.
#[must_use]
pub(crate) fn metric(label: &str, value: String) -> AnyView {
    let wrap = theme::class("display: flex; flex-direction: column; gap: 2px; min-width: 90px;");
    let value_cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_H3,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    let label_cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_CAPTION,
        c = color::TEXT_MUTED,
    ));
    view! {
        <div class=wrap>
            <div class=value_cls>{value}</div>
            <div class=label_cls>{label.to_owned()}</div>
        </div>
    }
    .into_any()
}

/// A section heading used inside report cards.
#[must_use]
pub(crate) fn section_title(text: &str) -> AnyView {
    let cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c}; margin: 0;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    view! { <div class=cls>{text.to_owned()}</div> }.into_any()
}

#[must_use]
pub(crate) fn month_name(m: u8) -> &'static str {
    match m {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        _ => "December",
    }
}

#[must_use]
pub(crate) fn month_abbr(m: u8) -> &'static str {
    match m {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        _ => "Dec",
    }
}
