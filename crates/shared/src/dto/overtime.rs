use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::dto::{common::UserSummaryDto, ids::OvertimeId};

/// Mirrors `domain::model::OvertimeStatus`.
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
    pub fn label(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::LeaderApproved => "Leader approved",
            Self::Approved => "Approved",
            Self::Rejected => "Rejected",
            Self::Cancelled => "Cancelled",
        }
    }
}

/// An overtime request with its requester and decision metadata. `work_date` is
/// `"YYYY-MM-DD"`; `hours` is the requested extra-hours amount.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OvertimeDto {
    pub id: OvertimeId,
    pub requester: UserSummaryDto,
    pub work_date: String,
    pub hours: f64,
    pub reason: String,
    pub status: OvertimeStatus,
    pub leader: Option<UserSummaryDto>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub leader_decided_at: Option<OffsetDateTime>,
    pub hr: Option<UserSummaryDto>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub hr_decided_at: Option<OffsetDateTime>,
    pub decision_note: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

/// Body of `POST /overtime`. Maps to `CreateOvertimeCommand`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOvertimeRequest {
    pub work_date: String,
    pub hours: f64,
    #[serde(default)]
    pub reason: String,
}

/// A leader's or HR's decision. Maps to `DecideOvertimeCommand`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecideOvertimeRequest {
    pub approve: bool,
    #[serde(default)]
    pub note: String,
}
