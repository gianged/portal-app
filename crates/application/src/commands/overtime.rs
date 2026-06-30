use time::Date;

/// Create an overtime request for a single `work_date`.
#[derive(Debug, Clone)]
pub struct CreateOvertimeCommand {
    pub work_date: Date,
    pub hours: f64,
    pub reason: String,
}

/// A leader's or HR's decision on a request.
#[derive(Debug, Clone)]
pub struct DecideOvertimeCommand {
    pub approve: bool,
    pub note: String,
}
