//! Date helpers backed by the browser clock for the wasm frontend.

use wasm_bindgen::JsValue;
use web_sys::js_sys::Date;

fn iso_from_js(d: &Date) -> String {
    format!(
        "{:04}-{:02}-{:02}",
        d.get_full_year() as i32,
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
