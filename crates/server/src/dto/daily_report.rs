//! Domain <-> wire projections for daily reports.

use application::commands::daily_report::{
    DailyReportEntryInput, ReviewDailyReportCommand, UpsertDailyReportCommand,
};
use domain::model;
use shared::dto::{
    common::UserSummaryDto,
    daily_report::{
        DailyReportDto, DailyReportEntryDto, DailyReportEntryKind as WireKind,
        DailyReportStatus as WireStatus, ReviewDailyReportRequest, UpsertDailyReportRequest,
    },
};
use time::Date;

use super::{daily_report_entry_id, daily_report_id};

#[must_use]
pub fn daily_report_status_dto(status: model::DailyReportStatus) -> WireStatus {
    match status {
        model::DailyReportStatus::Draft => WireStatus::Draft,
        model::DailyReportStatus::Submitted => WireStatus::Submitted,
        model::DailyReportStatus::Approved => WireStatus::Approved,
        model::DailyReportStatus::Returned => WireStatus::Returned,
    }
}

#[must_use]
pub fn daily_report_entry_kind_dto(kind: model::DailyReportEntryKind) -> WireKind {
    match kind {
        model::DailyReportEntryKind::RequestWork => WireKind::RequestWork,
        model::DailyReportEntryKind::Learning => WireKind::Learning,
        model::DailyReportEntryKind::Other => WireKind::Other,
    }
}

#[must_use]
pub fn daily_report_entry_kind_domain(kind: WireKind) -> model::DailyReportEntryKind {
    match kind {
        WireKind::RequestWork => model::DailyReportEntryKind::RequestWork,
        WireKind::Learning => model::DailyReportEntryKind::Learning,
        WireKind::Other => model::DailyReportEntryKind::Other,
    }
}

#[must_use]
pub fn daily_report_entry_dto(entry: &model::DailyReportEntry) -> DailyReportEntryDto {
    DailyReportEntryDto {
        id: daily_report_entry_id(entry.id),
        kind: daily_report_entry_kind_dto(entry.kind),
        description: entry.description.clone(),
        request_id: entry.request_id.map(super::request_id),
        hours: entry.hours,
        created_at: entry.created_at,
    }
}

/// Formats a calendar date as the wire `"YYYY-MM-DD"`.
#[must_use]
pub fn fmt_date(date: Date) -> String {
    format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        u8::from(date.month()),
        date.day()
    )
}

#[must_use]
pub fn daily_report_dto(
    report: &model::DailyReport,
    user: UserSummaryDto,
    reviewed_by: Option<UserSummaryDto>,
) -> DailyReportDto {
    DailyReportDto {
        id: daily_report_id(report.id),
        user,
        report_date: fmt_date(report.report_date),
        status: daily_report_status_dto(report.status),
        summary: report.summary.clone(),
        entries: report.entries.iter().map(daily_report_entry_dto).collect(),
        submitted_at: report.submitted_at,
        reviewed_by,
        reviewed_at: report.reviewed_at,
        review_note: report.review_note.clone(),
        created_at: report.created_at,
        updated_at: report.updated_at,
    }
}

#[must_use]
pub fn upsert_daily_report_command(
    report_date: Date,
    req: UpsertDailyReportRequest,
) -> UpsertDailyReportCommand {
    UpsertDailyReportCommand {
        report_date,
        summary: req.summary,
        entries: req
            .entries
            .into_iter()
            .map(|e| DailyReportEntryInput {
                kind: daily_report_entry_kind_domain(e.kind),
                description: e.description,
                request_id: e.request_id.map(|r| domain::ids::RequestId(r.0)),
                hours: e.hours,
                progress: e.progress,
            })
            .collect(),
    }
}

#[must_use]
pub fn review_daily_report_command(req: ReviewDailyReportRequest) -> ReviewDailyReportCommand {
    ReviewDailyReportCommand {
        approve: req.approve,
        note: req.note,
    }
}
