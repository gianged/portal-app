//! Holiday-calendar HTTP wrappers. Dates are `"YYYY-MM-DD"`; the list is scoped to
//! a calendar year.

use shared::dto::holiday::{HolidayDto, SetHolidayRequest};

use crate::api::client;
use crate::api::error::FrontendError;

pub async fn list_year(year: i32) -> Result<Vec<HolidayDto>, FrontendError> {
    let q = client::query(&[("year", &year.to_string())]);
    client::get_json(&format!("/holidays{q}")).await
}

pub async fn set(date: &str, req: &SetHolidayRequest) -> Result<HolidayDto, FrontendError> {
    client::put_json(&format!("/holidays/{date}"), req).await
}

pub async fn remove(date: &str) -> Result<(), FrontendError> {
    client::del(&format!("/holidays/{date}")).await
}
