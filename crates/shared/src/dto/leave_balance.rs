use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::dto::ids::{DayOffId, LeaveGrantId, LeaveTransactionId};

/// Mirrors `domain::model::LeaveTxnKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaveTxnKind {
    Grant,
    Consume,
    Refund,
    Adjust,
    Expire,
}

impl LeaveTxnKind {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Grant => "Grant",
            Self::Consume => "Consume",
            Self::Refund => "Refund",
            Self::Adjust => "Adjustment",
            Self::Expire => "Expiry",
        }
    }
}

/// One year's leave grant. `expires_on` is wire-encoded as `"YYYY-MM-DD"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaveGrantDto {
    pub id: LeaveGrantId,
    pub grant_year: u16,
    pub days_granted: f64,
    pub days_remaining: f64,
    pub expires_on: String,
}

/// A user's current balance: available days plus the per-year breakdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaveBalanceDto {
    pub available: f64,
    pub grants: Vec<LeaveGrantDto>,
}

/// One ledger entry. `work_pct` is present only on `Expire` rows under the
/// record-work-pct policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaveTransactionDto {
    pub id: LeaveTransactionId,
    pub kind: LeaveTxnKind,
    pub delta: f64,
    pub dayoff_id: Option<DayOffId>,
    pub work_pct: Option<f64>,
    pub reason: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// Grants plus the ledger entries in a date range.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaveStatementDto {
    pub grants: Vec<LeaveGrantDto>,
    pub transactions: Vec<LeaveTransactionDto>,
}

/// Body of `PUT /users/{id}/leave/grant`. Maps to `SetLeaveGrantCommand`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetLeaveGrantRequest {
    pub grant_year: u16,
    pub days_granted: f64,
}

/// Body of `POST /users/{id}/leave/adjust`. Maps to `AdjustBalanceCommand`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdjustBalanceRequest {
    pub delta: f64,
    pub reason: String,
}
