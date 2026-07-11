//! Domain <-> wire projection for holidays.

use domain::model::Holiday;
use shared::dto::holiday::HolidayDto;

#[must_use]
pub fn holiday_dto(holiday: &Holiday) -> HolidayDto {
    HolidayDto {
        date: holiday.date,
        name: holiday.name.clone(),
    }
}
