//! Domain <-> wire projection for holidays.

use domain::model::Holiday;
use shared::dto::holiday::HolidayDto;

use super::daily_report::fmt_date;

#[must_use]
pub fn holiday_dto(holiday: &Holiday) -> HolidayDto {
    HolidayDto {
        date: fmt_date(holiday.date),
        name: holiday.name.clone(),
    }
}
