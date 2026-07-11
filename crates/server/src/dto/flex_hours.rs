//! Domain <-> wire projections for flexible-hours requests.

use application::commands::flex_hours::{DecideFlexCommand, RequestFlexCommand};
use domain::model;
use shared::dto::{
    common::UserSummaryDto,
    flex_hours::{
        DecideFlexRequest, FlexHoursDto, FlexSegmentDto, FlexStatus as WireStatus,
        RequestFlexRequest,
    },
};
use shared::errors::SharedError;

#[must_use]
pub fn flex_status_dto(status: model::FlexStatus) -> WireStatus {
    match status {
        model::FlexStatus::Pending => WireStatus::Pending,
        model::FlexStatus::Approved => WireStatus::Approved,
        model::FlexStatus::Rejected => WireStatus::Rejected,
        model::FlexStatus::Cancelled => WireStatus::Cancelled,
    }
}

#[must_use]
pub fn flex_segment_dto(seg: &model::FlexSegment) -> FlexSegmentDto {
    FlexSegmentDto {
        id: super::flex_segment_id(seg.id),
        seq: seg.seq,
        start: super::fmt_time(seg.start),
        end: super::fmt_time(seg.end),
        hours: seg.hours(),
    }
}

#[must_use]
pub fn flex_hours_dto(
    flex: &model::FlexHours,
    user: UserSummaryDto,
    leader: Option<UserSummaryDto>,
) -> FlexHoursDto {
    let daily_hours = flex
        .segments
        .iter()
        .map(model::FlexSegment::hours)
        .sum::<f64>();
    FlexHoursDto {
        id: super::flex_hours_id(flex.id),
        user,
        work_date: flex.work_date,
        segments: flex.segments.iter().map(flex_segment_dto).collect(),
        daily_hours,
        status: flex_status_dto(flex.status),
        leader,
        decided_at: flex.decided_at,
        decision_note: flex.decision_note.clone(),
        created_at: flex.created_at,
        updated_at: flex.updated_at,
    }
}

/// # Errors
/// Returns [`SharedError::Validation`] when a block time is malformed.
pub fn request_flex_command(req: &RequestFlexRequest) -> Result<RequestFlexCommand, SharedError> {
    let mut segments = Vec::with_capacity(req.segments.len());
    for seg in &req.segments {
        let start = super::to_time(&seg.start, "Block start")?;
        let end = super::to_time(&seg.end, "Block end")?;
        segments.push((start, end));
    }
    Ok(RequestFlexCommand {
        work_date: req.work_date,
        segments,
    })
}

#[must_use]
pub fn decide_flex_command(req: DecideFlexRequest) -> DecideFlexCommand {
    DecideFlexCommand {
        approve: req.approve,
        note: req.note,
    }
}
