//! Domain -> wire projections for the reporting feature.

use domain::model::{
    GroupKind, GroupReportRow, GrowthPoint, GrowthSeries, MonthlyReportData, Report, ReportKind,
    StaffSummary, TicketCategory, TicketStatus, TicketSummary, YearlyReportData,
};
use shared::dto::report::{
    GroupHeadcountDto, GroupReportRowDto, GrowthPointDto, GrowthSeriesDto, LabeledCountDto,
    MonthlyReportDto, ReportKindDto, ReportSummaryDto, StaffSummaryDto, TicketSummaryDto,
    YearlyReportDto, YearlyTotalsDto,
};

use super::{group_id, report_id};

fn report_kind_dto(kind: ReportKind) -> ReportKindDto {
    match kind {
        ReportKind::Monthly => ReportKindDto::Monthly,
        ReportKind::Yearly => ReportKindDto::Yearly,
    }
}

fn ticket_status_label(s: TicketStatus) -> &'static str {
    match s {
        TicketStatus::Open => "Open",
        TicketStatus::Triaged => "Triaged",
        TicketStatus::Assigned => "Assigned",
        TicketStatus::InProgress => "In Progress",
        TicketStatus::Resolved => "Resolved",
        TicketStatus::Closed => "Closed",
        TicketStatus::Reopened => "Reopened",
    }
}

fn ticket_category_label(c: TicketCategory) -> &'static str {
    match c {
        TicketCategory::Hardware => "Hardware",
        TicketCategory::Software => "Software",
        TicketCategory::Access => "Access",
        TicketCategory::Other => "Other",
    }
}

fn group_row_dto(row: &GroupReportRow) -> GroupReportRowDto {
    GroupReportRowDto {
        group_id: group_id(row.group_id),
        group_name: row.group_name.clone(),
        is_it: matches!(row.group_kind, GroupKind::It),
        projects_total: row.projects_total,
        projects_completed: row.projects_completed,
        projects_active: row.projects_active,
        projects_on_hold: row.projects_on_hold,
        projects_stuck: row.projects_stuck,
        avg_project_progress: row.avg_project_progress,
        requests_total: row.requests_total,
        requests_completed: row.requests_completed,
        requests_open: row.requests_open,
        request_completion_pct: row.request_completion_pct,
        headcount: row.headcount,
    }
}

fn ticket_summary_dto(t: &TicketSummary) -> TicketSummaryDto {
    TicketSummaryDto {
        created_in_period: t.created_in_period,
        resolved_in_period: t.resolved_in_period,
        avg_resolve_hours: t.avg_resolve_hours,
        by_status: t
            .by_status
            .iter()
            .map(|(s, n)| LabeledCountDto {
                label: ticket_status_label(*s).to_owned(),
                count: *n,
            })
            .collect(),
        by_category: t
            .by_category
            .iter()
            .map(|(c, n)| LabeledCountDto {
                label: ticket_category_label(*c).to_owned(),
                count: *n,
            })
            .collect(),
    }
}

fn staff_summary_dto(s: &StaffSummary) -> StaffSummaryDto {
    StaffSummaryDto {
        company_headcount: s.company_headcount,
        new_joiners: s.new_joiners,
        deactivations: s.deactivations,
        per_group: s
            .per_group
            .iter()
            .map(|(gid, name, headcount)| GroupHeadcountDto {
                group_id: group_id(*gid),
                group_name: name.clone(),
                headcount: *headcount,
            })
            .collect(),
    }
}

#[must_use]
pub fn monthly_report_dto(data: &MonthlyReportData) -> MonthlyReportDto {
    MonthlyReportDto {
        year: data.period.start.year(),
        month: u8::from(data.period.start.month()),
        groups: data.groups.iter().map(group_row_dto).collect(),
        tickets: ticket_summary_dto(&data.tickets),
        staff: staff_summary_dto(&data.staff),
    }
}

fn growth_points(points: &[GrowthPoint]) -> Vec<GrowthPointDto> {
    points
        .iter()
        .map(|p| GrowthPointDto {
            year: p.year,
            month: p.month,
            value: p.value,
        })
        .collect()
}

fn growth_series_dto(g: &GrowthSeries) -> GrowthSeriesDto {
    GrowthSeriesDto {
        headcount: growth_points(&g.headcount),
        new_joiners: growth_points(&g.new_joiners),
        tickets_created: growth_points(&g.tickets_created),
        projects_completed: growth_points(&g.projects_completed),
        requests_completed: growth_points(&g.requests_completed),
    }
}

#[must_use]
pub fn yearly_report_dto(data: &YearlyReportData) -> YearlyReportDto {
    YearlyReportDto {
        year: data.year,
        growth: growth_series_dto(&data.growth),
        totals: YearlyTotalsDto {
            company_headcount: data.totals.company_headcount,
            net_headcount_change: data.totals.net_headcount_change,
            new_hires: data.totals.new_hires,
            departures: data.totals.departures,
            tickets_created: data.totals.tickets_created,
            projects_completed: data.totals.projects_completed,
            requests_completed: data.totals.requests_completed,
        },
    }
}

/// Archive item; `download_url` is the signed URL the handler mints per viewer.
#[must_use]
pub fn report_summary_dto(report: &Report, download_url: String) -> ReportSummaryDto {
    ReportSummaryDto {
        id: report_id(report.id),
        kind: report_kind_dto(report.kind),
        period_start: report.period_start,
        period_end: report.period_end,
        generated_at: report.generated_at,
        size_bytes: report.size_bytes,
        download_url,
    }
}
