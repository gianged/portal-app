use serde::{Deserialize, Serialize};
use time::Date;

/// A public holiday in the HR-maintained calendar. The date is the natural key;
/// weekends and holidays are excluded from leave day-counting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Holiday {
    pub date: Date,
    pub name: String,
}
