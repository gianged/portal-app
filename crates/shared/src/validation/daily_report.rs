use crate::{
    dto::daily_report::{DailyReportEntryKind, ReviewDailyReportRequest, UpsertDailyReportRequest},
    errors::SharedError,
    validation::common::{self, DESCRIPTION_MAX},
};

/// Upper bound on entries in one report.
const ENTRIES_MAX: usize = 50;
/// A working day cannot exceed 24 hours, per entry and in total.
const HOURS_MAX: f64 = 24.0;

/// Validates a daily-report upsert body.
///
/// # Errors
///
/// Returns [`SharedError::Validation`] when the summary is too long, an entry
/// description is empty, hours are out of `0..=24` (per entry or in total), a
/// `RequestWork` entry is missing its request link, a non-`RequestWork` entry
/// carries one, or a progress hint exceeds 100.
pub fn validate_daily_report(req: &UpsertDailyReportRequest) -> Result<(), SharedError> {
    common::max_len("Summary", &req.summary, DESCRIPTION_MAX)?;
    common::max_items("Entries", req.entries.len(), ENTRIES_MAX)?;

    let mut total_hours = 0.0;
    for entry in &req.entries {
        common::non_empty("Entry description", &entry.description)?;
        common::max_len("Entry description", &entry.description, DESCRIPTION_MAX)?;

        if let Some(hours) = entry.hours {
            common::in_range("Entry hours", hours, 0.0, HOURS_MAX)?;
            total_hours += hours;
        }

        match entry.kind {
            DailyReportEntryKind::RequestWork if entry.request_id.is_none() => {
                return Err(SharedError::Validation(
                    "Request-work entries must link a request".into(),
                ));
            }
            DailyReportEntryKind::Learning | DailyReportEntryKind::Other
                if entry.request_id.is_some() =>
            {
                return Err(SharedError::Validation(
                    "Only request-work entries may link a request".into(),
                ));
            }
            _ => {}
        }

        if let Some(progress) = entry.progress
            && progress > 100
        {
            return Err(SharedError::Validation(
                "Progress must be between 0 and 100".into(),
            ));
        }
    }

    if total_hours > HOURS_MAX {
        return Err(SharedError::Validation(
            "Total hours across entries must not exceed 24".into(),
        ));
    }
    Ok(())
}

/// Validates a daily-report review: a bounded note.
///
/// # Errors
/// Returns [`SharedError::Validation`] when the note is too long.
pub fn validate_review_daily_report(req: &ReviewDailyReportRequest) -> Result<(), SharedError> {
    common::max_len("Note", &req.note, DESCRIPTION_MAX)?;
    Ok(())
}
