use serde::{Deserialize, Serialize};
use time::Date;

/// A public holiday. `date` is wire-encoded as `"YYYY-MM-DD"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HolidayDto {
    pub date: Date,
    pub name: String,
}

/// Body of `PUT /holidays/{date}`. Maps to `HolidayService::set`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetHolidayRequest {
    pub name: String,
}

#[cfg(test)]
mod tests {
    use time::{Date, Month};

    use super::HolidayDto;

    // Locks the calendar-date wire format for every Date-carrying DTO.
    #[test]
    fn date_serializes_as_iso_ymd() {
        let dto = HolidayDto {
            date: Date::from_calendar_date(2026, Month::July, 4).unwrap(),
            name: "X".to_owned(),
        };
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("\"date\":\"2026-07-04\""), "got {json}");
        let back: HolidayDto = serde_json::from_str(&json).unwrap();
        assert_eq!(back.date, dto.date);
    }
}
