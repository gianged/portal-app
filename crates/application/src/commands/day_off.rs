use domain::model::DayOffKind;
use time::Date;

/// Create a leave request. `days` is computed server-side from the range and the
/// holiday calendar, not taken from the client.
#[derive(Debug, Clone)]
pub struct CreateDayOffCommand {
    pub kind: DayOffKind,
    pub start_date: Date,
    pub end_date: Date,
    pub start_half: bool,
    pub end_half: bool,
    pub reason: String,
}

/// A leader's or HR's decision on a request.
#[derive(Debug, Clone)]
pub struct DecideDayOffCommand {
    pub approve: bool,
    pub note: String,
}
