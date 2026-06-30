use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};

use crate::{
    error::TransitionError,
    ids::{DailyReportEntryId, DailyReportId, RequestId, UserId},
};

/// Review lifecycle of a daily report. A draft is editable by its owner; once
/// submitted it awaits a leader decision (approve / return for edits).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DailyReportStatus {
    Draft,
    Submitted,
    Approved,
    Returned,
}

impl DailyReportStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Submitted => "submitted",
            Self::Approved => "approved",
            Self::Returned => "returned",
        }
    }

    /// Owner submits a draft (or a returned report, after editing) for review.
    pub const fn try_submit(self) -> Result<Self, TransitionError> {
        match self {
            Self::Draft | Self::Returned => Ok(Self::Submitted),
            Self::Submitted | Self::Approved => {
                Err(TransitionError::invalid(self.as_str(), "submitted"))
            }
        }
    }

    /// Leader approves a submitted report.
    pub const fn try_approve(self) -> Result<Self, TransitionError> {
        match self {
            Self::Submitted => Ok(Self::Approved),
            Self::Draft | Self::Approved | Self::Returned => {
                Err(TransitionError::invalid(self.as_str(), "approved"))
            }
        }
    }

    /// Leader returns a submitted report to its owner for edits.
    pub const fn try_return(self) -> Result<Self, TransitionError> {
        match self {
            Self::Submitted => Ok(Self::Returned),
            Self::Draft | Self::Approved | Self::Returned => {
                Err(TransitionError::invalid(self.as_str(), "returned"))
            }
        }
    }
}

/// What a single report line describes: work on a linked request, a learning
/// activity, or anything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DailyReportEntryKind {
    RequestWork,
    Learning,
    Other,
}

impl DailyReportEntryKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RequestWork => "request_work",
            Self::Learning => "learning",
            Self::Other => "other",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "request_work" => Some(Self::RequestWork),
            "learning" => Some(Self::Learning),
            "other" => Some(Self::Other),
            _ => None,
        }
    }
}

/// One line of a daily report. `request_id` is required when `kind` is
/// `RequestWork` (enforced by the schema CHECK); `hours` is optional.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyReportEntry {
    pub id: DailyReportEntryId,
    pub daily_report_id: DailyReportId,
    pub kind: DailyReportEntryKind,
    pub description: String,
    pub request_id: Option<RequestId>,
    pub hours: Option<f64>,
    pub created_at: OffsetDateTime,
}

/// A staff member's report for one calendar day: a free-text summary plus typed
/// entries, moving through the review FSM. One per `(user, report_date)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyReport {
    pub id: DailyReportId,
    pub user_id: UserId,
    pub report_date: Date,
    pub status: DailyReportStatus,
    pub summary: String,
    pub entries: Vec<DailyReportEntry>,
    pub submitted_at: Option<OffsetDateTime>,
    pub reviewed_by: Option<UserId>,
    pub reviewed_at: Option<OffsetDateTime>,
    pub review_note: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl DailyReport {
    /// Owner submits for review; stamps `submitted_at`.
    pub fn submit(&mut self, now: OffsetDateTime) -> Result<(), TransitionError> {
        self.status = self.status.try_submit()?;
        self.submitted_at = Some(now);
        self.updated_at = now;
        Ok(())
    }

    /// Leader approves; records reviewer, note, and timestamp.
    pub fn approve(
        &mut self,
        reviewer: UserId,
        note: String,
        now: OffsetDateTime,
    ) -> Result<(), TransitionError> {
        self.status = self.status.try_approve()?;
        self.reviewed_by = Some(reviewer);
        self.reviewed_at = Some(now);
        self.review_note = note;
        self.updated_at = now;
        Ok(())
    }

    /// Leader returns for edits; records reviewer, note, and timestamp.
    pub fn return_for_edits(
        &mut self,
        reviewer: UserId,
        note: String,
        now: OffsetDateTime,
    ) -> Result<(), TransitionError> {
        self.status = self.status.try_return()?;
        self.reviewed_by = Some(reviewer);
        self.reviewed_at = Some(now);
        self.review_note = note;
        self.updated_at = now;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submit_allowed_from_draft_or_returned() {
        assert_eq!(
            DailyReportStatus::Draft.try_submit().unwrap(),
            DailyReportStatus::Submitted
        );
        assert_eq!(
            DailyReportStatus::Returned.try_submit().unwrap(),
            DailyReportStatus::Submitted
        );
        assert!(DailyReportStatus::Submitted.try_submit().is_err());
        assert!(DailyReportStatus::Approved.try_submit().is_err());
    }

    #[test]
    fn approve_and_return_require_submitted() {
        assert_eq!(
            DailyReportStatus::Submitted.try_approve().unwrap(),
            DailyReportStatus::Approved
        );
        assert_eq!(
            DailyReportStatus::Submitted.try_return().unwrap(),
            DailyReportStatus::Returned
        );
        for s in [
            DailyReportStatus::Draft,
            DailyReportStatus::Approved,
            DailyReportStatus::Returned,
        ] {
            assert!(s.try_approve().is_err());
            assert!(s.try_return().is_err());
        }
    }

    #[test]
    fn entry_kind_round_trips() {
        for k in [
            DailyReportEntryKind::RequestWork,
            DailyReportEntryKind::Learning,
            DailyReportEntryKind::Other,
        ] {
            assert_eq!(DailyReportEntryKind::parse(k.as_str()), Some(k));
        }
        assert_eq!(DailyReportEntryKind::parse("nope"), None);
    }
}
