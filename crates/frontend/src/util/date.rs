//! Date helpers backed by the browser clock for the wasm frontend.

use time::{format_description::BorrowedFormatItem, macros::format_description};
use wasm_bindgen::JsValue;
use web_sys::js_sys::Date;

const ISO_DATE: &[BorrowedFormatItem<'static>] = format_description!("[year]-[month]-[day]");

fn iso_from_js(d: &Date) -> String {
    format!(
        "{:04}-{:02}-{:02}",
        d.get_full_year().cast_signed(),
        d.get_month() + 1,
        d.get_date(),
    )
}

/// Today's date as `YYYY-MM-DD`.
pub fn today_iso() -> String {
    iso_from_js(&Date::new_0())
}

/// `YYYY-MM-DD` offset by `days` from now; negative looks forward.
pub fn days_ago_iso(days: f64) -> String {
    let now = Date::new_0();
    let ms = now.get_time() - days * 86_400_000.0;
    iso_from_js(&Date::new(&JsValue::from_f64(ms)))
}

/// List window for the attendance my-items views: 120 days back, one year forward.
pub fn attendance_window() -> (String, String) {
    (days_ago_iso(120.0), days_ago_iso(-365.0))
}

/// Formats a calendar date as `YYYY-MM-DD` (the wire format).
pub fn to_iso(date: time::Date) -> String {
    date.format(ISO_DATE).unwrap_or_default()
}

/// Parses a `YYYY-MM-DD` string (e.g. an `<input type="date">` value).
pub fn from_iso(s: &str) -> Option<time::Date> {
    time::Date::parse(s, ISO_DATE).ok()
}
