use serde::{Deserialize, Serialize};

/// A public holiday. `date` is wire-encoded as `"YYYY-MM-DD"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HolidayDto {
    pub date: String,
    pub name: String,
}

/// Body of `PUT /holidays/{date}`. Maps to `HolidayService::set`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetHolidayRequest {
    pub name: String,
}
