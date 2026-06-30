//! Domain <-> wire projections for day-off (leave) requests.

use application::commands::day_off::{CreateDayOffCommand, DecideDayOffCommand};
use domain::model;
use shared::dto::{
    common::UserSummaryDto,
    day_off::{
        CreateDayOffRequest, DayOffDto, DayOffKind as WireKind, DayOffStatus as WireStatus,
        DecideDayOffRequest,
    },
};
use time::Date;

use super::{daily_report::fmt_date, day_off_id};

// --- enums ---

#[must_use]
pub fn day_off_kind_dto(kind: model::DayOffKind) -> WireKind {
    match kind {
        model::DayOffKind::AnnualLeave => WireKind::AnnualLeave,
        model::DayOffKind::SickLeave => WireKind::SickLeave,
        model::DayOffKind::UnpaidLeave => WireKind::UnpaidLeave,
        model::DayOffKind::Remote => WireKind::Remote,
        model::DayOffKind::Other => WireKind::Other,
    }
}

#[must_use]
pub fn day_off_kind_domain(kind: WireKind) -> model::DayOffKind {
    match kind {
        WireKind::AnnualLeave => model::DayOffKind::AnnualLeave,
        WireKind::SickLeave => model::DayOffKind::SickLeave,
        WireKind::UnpaidLeave => model::DayOffKind::UnpaidLeave,
        WireKind::Remote => model::DayOffKind::Remote,
        WireKind::Other => model::DayOffKind::Other,
    }
}

#[must_use]
pub fn day_off_status_dto(status: model::DayOffStatus) -> WireStatus {
    match status {
        model::DayOffStatus::Pending => WireStatus::Pending,
        model::DayOffStatus::LeaderApproved => WireStatus::LeaderApproved,
        model::DayOffStatus::Approved => WireStatus::Approved,
        model::DayOffStatus::Rejected => WireStatus::Rejected,
        model::DayOffStatus::Cancelled => WireStatus::Cancelled,
    }
}

// --- views ---

#[must_use]
pub fn day_off_dto(
    day_off: &model::DayOff,
    requester: UserSummaryDto,
    leader: Option<UserSummaryDto>,
    hr: Option<UserSummaryDto>,
) -> DayOffDto {
    DayOffDto {
        id: day_off_id(day_off.id),
        requester,
        kind: day_off_kind_dto(day_off.kind),
        start_date: fmt_date(day_off.start_date),
        end_date: fmt_date(day_off.end_date),
        start_half: day_off.start_half,
        end_half: day_off.end_half,
        days: day_off.days,
        reason: day_off.reason.clone(),
        status: day_off_status_dto(day_off.status),
        leader,
        leader_decided_at: day_off.leader_decided_at,
        hr,
        hr_decided_at: day_off.hr_decided_at,
        decision_note: day_off.decision_note.clone(),
        created_at: day_off.created_at,
        updated_at: day_off.updated_at,
    }
}

// --- commands ---

#[must_use]
pub fn create_day_off_command(
    start: Date,
    end: Date,
    req: CreateDayOffRequest,
) -> CreateDayOffCommand {
    CreateDayOffCommand {
        kind: day_off_kind_domain(req.kind),
        start_date: start,
        end_date: end,
        start_half: req.start_half,
        end_half: req.end_half,
        reason: req.reason,
    }
}

#[must_use]
pub fn decide_day_off_command(req: DecideDayOffRequest) -> DecideDayOffCommand {
    DecideDayOffCommand {
        approve: req.approve,
        note: req.note,
    }
}
