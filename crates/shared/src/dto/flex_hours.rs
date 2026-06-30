use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::dto::{
    common::UserSummaryDto,
    ids::{FlexHoursId, FlexSegmentId},
};

/// Mirrors `domain::model::FlexStatus`.
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
    pub fn label(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Approved => "Approved",
            Self::Rejected => "Rejected",
            Self::Cancelled => "Cancelled",
        }
    }
}

/// One work block of a flex day. `start` / `end` are `"HH:MM"`; `hours` is the
/// block length.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlexSegmentDto {
    pub id: FlexSegmentId,
    pub seq: u16,
    pub start: String,
    pub end: String,
    pub hours: f64,
}

/// A flex-hours request with its owner, blocks, and decision metadata.
/// `work_date` is `"YYYY-MM-DD"`; `daily_hours` is the summed block length.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlexHoursDto {
    pub id: FlexHoursId,
    pub user: UserSummaryDto,
    pub work_date: String,
    pub segments: Vec<FlexSegmentDto>,
    pub daily_hours: f64,
    pub status: FlexStatus,
    pub leader: Option<UserSummaryDto>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub decided_at: Option<OffsetDateTime>,
    pub decision_note: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

/// One block of a `POST /flex-hours` body. Times are `"HH:MM"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlexSegmentInput {
    pub start: String,
    pub end: String,
}

/// Body of `POST /flex-hours`. Maps to `RequestFlexCommand`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestFlexRequest {
    pub work_date: String,
    pub segments: Vec<FlexSegmentInput>,
}

/// A leader's decision. Maps to `DecideFlexCommand`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecideFlexRequest {
    pub approve: bool,
    #[serde(default)]
    pub note: String,
}

/// Running monthly settlement: `delta = approved_hours - expected_hours`; zero
/// means the month is reconciled.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlexMonthDeltaDto {
    pub year: i32,
    pub month: u8,
    pub delta: f64,
}
