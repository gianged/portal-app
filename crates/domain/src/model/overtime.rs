use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};

use crate::{
    error::TransitionError,
    ids::{OvertimeId, UserId},
};

/// Lifecycle of an overtime request. It always goes
/// `Pending -> LeaderApproved -> Approved`; either stage may reject, and the
/// requester may cancel while non-terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OvertimeStatus {
    Pending,
    LeaderApproved,
    Approved,
    Rejected,
    Cancelled,
}

impl OvertimeStatus {
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

    /// Leader approves the first stage; HR still owes a decision.
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

/// An overtime request mirroring the `attendance.overtime` row. The FSM methods
/// stamp the relevant decider + timestamp and bump `updated_at`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Overtime {
    pub id: OvertimeId,
    pub requester_user_id: UserId,
    pub work_date: Date,
    pub hours: f64,
    pub reason: String,
    pub status: OvertimeStatus,
    pub leader_user_id: Option<UserId>,
    pub leader_decided_at: Option<OffsetDateTime>,
    pub hr_user_id: Option<UserId>,
    pub hr_decided_at: Option<OffsetDateTime>,
    pub decision_note: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl Overtime {
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
        let at_leader_stage = matches!(self.status, OvertimeStatus::Pending);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dual_approval_chain() {
        let s = OvertimeStatus::Pending;
        let s = s.try_leader_approve().unwrap();
        assert_eq!(s, OvertimeStatus::LeaderApproved);
        let s = s.try_hr_approve().unwrap();
        assert_eq!(s, OvertimeStatus::Approved);
    }

    #[test]
    fn hr_cannot_approve_pending() {
        assert!(OvertimeStatus::Pending.try_hr_approve().is_err());
    }

    #[test]
    fn terminal_states_are_final() {
        assert!(OvertimeStatus::Approved.try_cancel().is_err());
        assert!(OvertimeStatus::Rejected.try_reject().is_err());
    }
}
