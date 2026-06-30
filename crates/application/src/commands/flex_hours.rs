use time::{Date, Time};

/// Request a flexible-hours day built from ordered `(start, end)` blocks.
#[derive(Debug, Clone)]
pub struct RequestFlexCommand {
    pub work_date: Date,
    pub segments: Vec<(Time, Time)>,
}

/// A leader's decision on a flex request.
#[derive(Debug, Clone)]
pub struct DecideFlexCommand {
    pub approve: bool,
    pub note: String,
}
