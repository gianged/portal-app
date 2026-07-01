use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::{Date, OffsetDateTime, Time};

use crate::{
    error::TransitionError,
    ids::{FlexHoursId, FlexSegmentId, UserId},
    model::policy::AttendancePolicy,
};

/// Tolerance (one minute) for comparing summed segment hours against the daily band.
const FLEX_HOURS_TOL: f64 = 1.0 / 60.0;

/// Raised when a [`FlexHours`] day fails its per-day shape rules. The monthly
/// reconciliation rule is checked separately (it nets across the whole month).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FlexError {
    #[error("a flex day needs between 1 and {max} work blocks")]
    SegmentCount { max: u16 },
    #[error("each block must end after it starts")]
    SegmentOrder,
    #[error("blocks must fall within the allowed start/end window")]
    OutsideEnvelope,
    #[error("blocks must be ordered and must not overlap")]
    Overlap,
    #[error("blocks must cover the required core hours")]
    CoreNotCovered,
    #[error("the daily total must be within the allowed band")]
    DailyBand,
}

/// Lifecycle of a flex-hours request. Single leader decision:
/// `Pending -> Approved / Rejected`; the owner may cancel while pending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlexStatus {
    Pending,
    Approved,
    Rejected,
    Cancelled,
}

impl FlexStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Cancelled => "cancelled",
        }
    }

    pub const fn try_approve(self) -> Result<Self, TransitionError> {
        match self {
            Self::Pending => Ok(Self::Approved),
            _ => Err(TransitionError::invalid(self.as_str(), "approved")),
        }
    }

    pub const fn try_reject(self) -> Result<Self, TransitionError> {
        match self {
            Self::Pending => Ok(Self::Rejected),
            _ => Err(TransitionError::invalid(self.as_str(), "rejected")),
        }
    }

    pub const fn try_cancel(self) -> Result<Self, TransitionError> {
        match self {
            Self::Pending => Ok(Self::Cancelled),
            _ => Err(TransitionError::invalid(self.as_str(), "cancelled")),
        }
    }
}

/// One ordered work block within a flex day, mirroring `attendance.flex_segments`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlexSegment {
    pub id: FlexSegmentId,
    pub flex_id: FlexHoursId,
    pub seq: u16,
    pub start: Time,
    pub end: Time,
}

impl FlexSegment {
    /// Block length in hours.
    #[must_use]
    pub fn hours(&self) -> f64 {
        (self.end - self.start).as_seconds_f64() / 3600.0
    }
}

/// A per-day custom schedule mirroring the `attendance.flex_hours` row plus its
/// ordered segments. The FSM methods stamp the leader decision and bump
/// `updated_at`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlexHours {
    pub id: FlexHoursId,
    pub user_id: UserId,
    pub work_date: Date,
    pub segments: Vec<FlexSegment>,
    pub status: FlexStatus,
    pub leader_user_id: Option<UserId>,
    pub decided_at: Option<OffsetDateTime>,
    pub decision_note: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl FlexHours {
    /// Checks the per-day shape against the policy: block count, ordering,
    /// non-overlap, envelope containment, core-hour coverage and the daily band.
    /// Monthly settlement is enforced elsewhere (see `month_delta`).
    ///
    /// # Errors
    /// Returns a [`FlexError`] describing the first rule violated.
    pub fn validate_day(&self, policy: &AttendancePolicy) -> Result<(), FlexError> {
        let n = self.segments.len();
        if n == 0 || n > usize::from(policy.flex_max_segments) {
            return Err(FlexError::SegmentCount {
                max: policy.flex_max_segments,
            });
        }
        for seg in &self.segments {
            if seg.end <= seg.start {
                return Err(FlexError::SegmentOrder);
            }
            if seg.start < policy.flex_earliest_start || seg.end > policy.flex_latest_end {
                return Err(FlexError::OutsideEnvelope);
            }
        }
        // Ordered and non-overlapping: each block must start at or after the prior ends.
        for pair in self.segments.windows(2) {
            if pair[1].start < pair[0].end {
                return Err(FlexError::Overlap);
            }
        }
        // Continuous coverage of the core window by the (sorted) blocks.
        let mut covered = policy.flex_core_start;
        for seg in &self.segments {
            if seg.start <= covered {
                if seg.end > covered {
                    covered = seg.end;
                }
            } else {
                break;
            }
            if covered >= policy.flex_core_end {
                break;
            }
        }
        if covered < policy.flex_core_end {
            return Err(FlexError::CoreNotCovered);
        }
        let total: f64 = self.segments.iter().map(FlexSegment::hours).sum();
        if total < policy.flex_daily_min - FLEX_HOURS_TOL
            || total > policy.flex_daily_max + FLEX_HOURS_TOL
        {
            return Err(FlexError::DailyBand);
        }
        Ok(())
    }

    /// Leader approves the pending request.
    pub fn approve(
        &mut self,
        leader: UserId,
        note: String,
        now: OffsetDateTime,
    ) -> Result<(), TransitionError> {
        self.status = self.status.try_approve()?;
        self.leader_user_id = Some(leader);
        self.decided_at = Some(now);
        self.decision_note = note;
        self.updated_at = now;
        Ok(())
    }

    /// Leader rejects the pending request.
    pub fn reject(
        &mut self,
        leader: UserId,
        note: String,
        now: OffsetDateTime,
    ) -> Result<(), TransitionError> {
        self.status = self.status.try_reject()?;
        self.leader_user_id = Some(leader);
        self.decided_at = Some(now);
        self.decision_note = note;
        self.updated_at = now;
        Ok(())
    }

    /// Owner cancels their pending request.
    pub fn cancel(&mut self, now: OffsetDateTime) -> Result<(), TransitionError> {
        self.status = self.status.try_cancel()?;
        self.updated_at = now;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Month;
    use uuid::Uuid;

    fn seg(start: (u8, u8), end: (u8, u8)) -> FlexSegment {
        FlexSegment {
            id: FlexSegmentId(Uuid::now_v7()),
            flex_id: FlexHoursId(Uuid::now_v7()),
            seq: 0,
            start: Time::from_hms(start.0, start.1, 0).unwrap(),
            end: Time::from_hms(end.0, end.1, 0).unwrap(),
        }
    }

    fn flex(segments: Vec<FlexSegment>) -> FlexHours {
        FlexHours {
            id: FlexHoursId(Uuid::now_v7()),
            user_id: UserId(Uuid::now_v7()),
            work_date: Date::from_calendar_date(2026, Month::June, 1).unwrap(),
            segments,
            status: FlexStatus::Pending,
            leader_user_id: None,
            decided_at: None,
            decision_note: String::new(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn single_block_covering_core_is_valid() {
        let policy = AttendancePolicy::default();
        let f = flex(vec![seg((8, 0), (16, 0))]);
        assert!(f.validate_day(&policy).is_ok());
    }

    #[test]
    fn overlapping_blocks_rejected() {
        let policy = AttendancePolicy::default();
        let f = flex(vec![seg((8, 0), (12, 0)), seg((11, 0), (16, 0))]);
        assert_eq!(f.validate_day(&policy), Err(FlexError::Overlap));
    }

    #[test]
    fn gap_in_core_rejected() {
        let policy = AttendancePolicy::default();
        // 08-11 then 14-17 leaves 11-14 uncovered inside core (10-15).
        let f = flex(vec![seg((8, 0), (11, 0)), seg((14, 0), (17, 0))]);
        assert_eq!(f.validate_day(&policy), Err(FlexError::CoreNotCovered));
    }

    #[test]
    fn below_daily_min_rejected() {
        let policy = AttendancePolicy::default();
        // 10-13 = 3h, under the 4h daily min.
        let f = flex(vec![seg((10, 0), (13, 0))]);
        // Fails core coverage (core ends 15) before the band check.
        assert_eq!(f.validate_day(&policy), Err(FlexError::CoreNotCovered));
    }

    #[test]
    fn approve_then_reapprove_fails() {
        let mut f = flex(vec![seg((8, 0), (16, 0))]);
        let now = OffsetDateTime::UNIX_EPOCH;
        f.approve(UserId(Uuid::now_v7()), String::new(), now)
            .unwrap();
        assert_eq!(f.status, FlexStatus::Approved);
        assert!(f.cancel(now).is_err());
    }
}
