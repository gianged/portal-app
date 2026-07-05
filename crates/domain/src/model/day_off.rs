use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime, Weekday};

use crate::{
    error::TransitionError,
    ids::{DayOffId, UserId},
    model::LEAVE_UNIT,
};

/// The kind of leave. Drives the approval path, backdating, and balance rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DayOffKind {
    AnnualLeave,
    SickLeave,
    UnpaidLeave,
    Remote,
    Other,
}

impl DayOffKind {
    /// Annual leave needs leader then HR; everything else is leader-only.
    #[must_use]
    pub const fn requires_hr_approval(self) -> bool {
        matches!(self, Self::AnnualLeave)
    }

    /// Sick / unpaid leave may be filed for a date in the past.
    #[must_use]
    pub const fn allows_backdate(self) -> bool {
        matches!(self, Self::SickLeave | Self::UnpaidLeave)
    }

    /// Only annual leave draws down the leave balance.
    #[must_use]
    pub const fn consumes_balance(self) -> bool {
        matches!(self, Self::AnnualLeave)
    }
}

/// Lifecycle of a leave request. Leader-only kinds go `Pending -> Approved`;
/// annual leave goes `Pending -> LeaderApproved -> Approved`. Either stage may
/// reject; the requester may cancel while non-terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DayOffStatus {
    Pending,
    LeaderApproved,
    Approved,
    Rejected,
    Cancelled,
}

impl DayOffStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::LeaderApproved => "leader_approved",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Cancelled => "cancelled",
        }
    }

    /// Leader final-approves a leader-only kind.
    pub const fn try_approve(self) -> Result<Self, TransitionError> {
        match self {
            Self::Pending => Ok(Self::Approved),
            _ => Err(TransitionError::invalid(self.as_str(), "approved")),
        }
    }

    /// Leader approves the first stage of an HR-gated kind.
    pub const fn try_leader_approve(self) -> Result<Self, TransitionError> {
        match self {
            Self::Pending => Ok(Self::LeaderApproved),
            _ => Err(TransitionError::invalid(self.as_str(), "leader_approved")),
        }
    }

    /// HR final-approves a leader-approved request.
    pub const fn try_hr_approve(self) -> Result<Self, TransitionError> {
        match self {
            Self::LeaderApproved => Ok(Self::Approved),
            _ => Err(TransitionError::invalid(self.as_str(), "approved")),
        }
    }

    pub const fn try_reject(self) -> Result<Self, TransitionError> {
        match self {
            Self::Pending | Self::LeaderApproved => Ok(Self::Rejected),
            _ => Err(TransitionError::invalid(self.as_str(), "rejected")),
        }
    }

    pub const fn try_cancel(self) -> Result<Self, TransitionError> {
        match self {
            Self::Pending | Self::LeaderApproved => Ok(Self::Cancelled),
            _ => Err(TransitionError::invalid(self.as_str(), "cancelled")),
        }
    }
}

/// A leave request mirroring the `attendance.dayoff` row. The FSM methods stamp
/// the relevant decider + timestamp and bump `updated_at`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayOff {
    pub id: DayOffId,
    pub requester_user_id: UserId,
    pub kind: DayOffKind,
    pub start_date: Date,
    pub end_date: Date,
    pub start_half: bool,
    pub end_half: bool,
    pub days: f64,
    pub reason: String,
    pub status: DayOffStatus,
    pub leader_user_id: Option<UserId>,
    pub leader_decided_at: Option<OffsetDateTime>,
    pub hr_user_id: Option<UserId>,
    pub hr_decided_at: Option<OffsetDateTime>,
    pub decision_note: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl DayOff {
    /// Leader final-approves a leader-only kind.
    pub fn approve(
        &mut self,
        leader: UserId,
        note: String,
        now: OffsetDateTime,
    ) -> Result<(), TransitionError> {
        // HR-gated kinds must route through leader_approve then hr_approve.
        if self.kind.requires_hr_approval() {
            return Err(TransitionError::invalid(self.status.as_str(), "approved"));
        }
        self.status = self.status.try_approve()?;
        self.leader_user_id = Some(leader);
        self.leader_decided_at = Some(now);
        self.decision_note = note;
        self.updated_at = now;
        Ok(())
    }

    /// Leader approves the first stage; HR still owes a decision.
    pub fn leader_approve(
        &mut self,
        leader: UserId,
        note: String,
        now: OffsetDateTime,
    ) -> Result<(), TransitionError> {
        self.status = self.status.try_leader_approve()?;
        self.leader_user_id = Some(leader);
        self.leader_decided_at = Some(now);
        self.decision_note = note;
        self.updated_at = now;
        Ok(())
    }

    /// HR final-approves a leader-approved request.
    pub fn hr_approve(
        &mut self,
        hr: UserId,
        note: String,
        now: OffsetDateTime,
    ) -> Result<(), TransitionError> {
        self.status = self.status.try_hr_approve()?;
        self.hr_user_id = Some(hr);
        self.hr_decided_at = Some(now);
        self.decision_note = note;
        self.updated_at = now;
        Ok(())
    }

    /// Rejects the request, stamping the decider at whichever stage rejected it.
    pub fn reject(
        &mut self,
        decider: UserId,
        note: String,
        now: OffsetDateTime,
    ) -> Result<(), TransitionError> {
        let at_leader_stage = matches!(self.status, DayOffStatus::Pending);
        self.status = self.status.try_reject()?;
        if at_leader_stage {
            self.leader_user_id = Some(decider);
            self.leader_decided_at = Some(now);
        } else {
            self.hr_user_id = Some(decider);
            self.hr_decided_at = Some(now);
        }
        self.decision_note = note;
        self.updated_at = now;
        Ok(())
    }

    /// Requester cancels a non-terminal request.
    pub fn cancel(&mut self, now: OffsetDateTime) -> Result<(), TransitionError> {
        self.status = self.status.try_cancel()?;
        self.updated_at = now;
        Ok(())
    }
}

/// Counts working days in the inclusive `[start, end]` range, excluding weekends
/// and `holidays`, then subtracts [`crate::model::LEAVE_UNIT`] per half flag when
/// that boundary day is itself a counted working day.
#[must_use]
pub fn working_days(
    start: Date,
    end: Date,
    start_half: bool,
    end_half: bool,
    holidays: &[Date],
) -> f64 {
    if end < start {
        return 0.0;
    }
    let is_working = |d: Date| {
        !matches!(d.weekday(), Weekday::Saturday | Weekday::Sunday) && !holidays.contains(&d)
    };

    let mut count = 0u32;
    let mut day = start;
    loop {
        if is_working(day) {
            count += 1;
        }
        if day == end {
            break;
        }
        let Some(next) = day.next_day() else { break };
        day = next;
    }

    let half = LEAVE_UNIT;
    let mut total = f64::from(count);
    if start == end {
        if (start_half || end_half) && is_working(start) {
            total -= half;
        }
    } else {
        if start_half && is_working(start) {
            total -= half;
        }
        if end_half && is_working(end) {
            total -= half;
        }
    }
    total.max(0.0)
}

#[cfg(test)]
mod tests {
    use time::Month;

    use super::*;

    fn d(y: i32, m: Month, day: u8) -> Date {
        Date::from_calendar_date(y, m, day).unwrap()
    }

    #[test]
    fn counts_weekdays_only() {
        // Mon 2026-06-29 .. Fri 2026-07-03 => 5 working days.
        let start = d(2026, Month::June, 29);
        let end = d(2026, Month::July, 3);
        assert_eq!(working_days(start, end, false, false, &[]), 5.0);
    }

    #[test]
    fn skips_holidays_and_subtracts_halves() {
        let start = d(2026, Month::June, 29);
        let end = d(2026, Month::July, 3);
        let holidays = [d(2026, Month::July, 1)]; // one mid-week holiday
        // 4 working days, minus 0.5 start, minus 0.5 end = 3.0
        assert_eq!(working_days(start, end, true, true, &holidays), 3.0);
    }

    #[test]
    fn single_half_day() {
        let day = d(2026, Month::June, 29); // Monday
        assert_eq!(working_days(day, day, true, false, &[]), 0.5);
    }

    #[test]
    fn weekend_only_is_zero() {
        let sat = d(2026, Month::July, 4);
        let sun = d(2026, Month::July, 5);
        assert_eq!(working_days(sat, sun, false, false, &[]), 0.0);
    }
}
