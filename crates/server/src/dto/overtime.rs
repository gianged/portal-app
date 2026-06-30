//! Domain <-> wire projections for overtime requests.

use application::commands::overtime::{CreateOvertimeCommand, DecideOvertimeCommand};
use domain::model;
use shared::dto::{
    common::UserSummaryDto,
    overtime::{
        CreateOvertimeRequest, DecideOvertimeRequest, OvertimeDto, OvertimeStatus as WireStatus,
    },
};
use time::Date;

use super::{daily_report::fmt_date, overtime_id};

// --- enums ---

#[must_use]
pub fn overtime_status_dto(status: model::OvertimeStatus) -> WireStatus {
    match status {
        model::OvertimeStatus::Pending => WireStatus::Pending,
        model::OvertimeStatus::LeaderApproved => WireStatus::LeaderApproved,
        model::OvertimeStatus::Approved => WireStatus::Approved,
        model::OvertimeStatus::Rejected => WireStatus::Rejected,
        model::OvertimeStatus::Cancelled => WireStatus::Cancelled,
    }
}

// --- views ---

#[must_use]
pub fn overtime_dto(
    overtime: &model::Overtime,
    requester: UserSummaryDto,
    leader: Option<UserSummaryDto>,
    hr: Option<UserSummaryDto>,
) -> OvertimeDto {
    OvertimeDto {
        id: overtime_id(overtime.id),
        requester,
        work_date: fmt_date(overtime.work_date),
        hours: overtime.hours,
        reason: overtime.reason.clone(),
        status: overtime_status_dto(overtime.status),
        leader,
        leader_decided_at: overtime.leader_decided_at,
        hr,
        hr_decided_at: overtime.hr_decided_at,
        decision_note: overtime.decision_note.clone(),
        created_at: overtime.created_at,
        updated_at: overtime.updated_at,
    }
}

// --- commands ---

#[must_use]
pub fn create_overtime_command(
    work_date: Date,
    req: CreateOvertimeRequest,
) -> CreateOvertimeCommand {
    CreateOvertimeCommand {
        work_date,
        hours: req.hours,
        reason: req.reason,
    }
}

#[must_use]
pub fn decide_overtime_command(req: DecideOvertimeRequest) -> DecideOvertimeCommand {
    DecideOvertimeCommand {
        approve: req.approve,
        note: req.note,
    }
}
