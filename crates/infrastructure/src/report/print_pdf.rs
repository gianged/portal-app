use std::mem;

use printpdf::{
    BuiltinFont, Color, Line, LinePoint, Mm, Op, PaintMode, PdfDocument, PdfFontHandle, PdfPage,
    PdfSaveOptions, Point, Polygon, PolygonRing, Pt, Rgb, TextItem, WindingOrder,
};
use time::{Month, OffsetDateTime};

use domain::{
    error::RenderError,
    model::{
        DayOffKind, GroupReportRow, MonthlyReportData, StaffMonthlyReport, StaffSummary,
        TicketCategory, TicketStats, TicketStatus, YearlyReportData,
    },
    ports::report_renderer::ReportRenderer,
};

// A4 in millimetres, origin bottom-left.
const PAGE_W: f64 = 210.0;
const PAGE_H: f64 = 297.0;
const MARGIN: f64 = 18.0;
const CONTENT_W: f64 = PAGE_W - 2.0 * MARGIN;
const BOTTOM: f64 = MARGIN;
const TOP: f64 = PAGE_H - MARGIN;

fn rgb(r: f32, g: f32, b: f32) -> Rgb {
    Rgb {
        r,
        g,
        b,
        icc_profile: None,
    }
}

// printpdf's `Mm`/`Pt` wrap `f32`; our layout math is `f64`, so convert at the edge.
fn mm(v: f64) -> Mm {
    Mm(v as f32)
}

fn pt(v: f64) -> Pt {
    Pt(v as f32)
}

fn ink() -> Rgb {
    rgb(0.13, 0.15, 0.18)
}
fn muted() -> Rgb {
    rgb(0.45, 0.48, 0.52)
}
fn accent() -> Rgb {
    rgb(0.21, 0.45, 0.85)
}
fn success() -> Rgb {
    rgb(0.18, 0.6, 0.36)
}
fn warning() -> Rgb {
    rgb(0.86, 0.6, 0.12)
}
fn danger() -> Rgb {
    rgb(0.83, 0.27, 0.27)
}
fn hairline() -> Rgb {
    rgb(0.82, 0.84, 0.87)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

const MONTHS: [Month; 12] = [
    Month::January,
    Month::February,
    Month::March,
    Month::April,
    Month::May,
    Month::June,
    Month::July,
    Month::August,
    Month::September,
    Month::October,
    Month::November,
    Month::December,
];

fn month_name(m: Month) -> &'static str {
    match m {
        Month::January => "January",
        Month::February => "February",
        Month::March => "March",
        Month::April => "April",
        Month::May => "May",
        Month::June => "June",
        Month::July => "July",
        Month::August => "August",
        Month::September => "September",
        Month::October => "October",
        Month::November => "November",
        Month::December => "December",
    }
}

fn month_abbr(m: Month) -> &'static str {
    match m {
        Month::January => "Jan",
        Month::February => "Feb",
        Month::March => "Mar",
        Month::April => "Apr",
        Month::May => "May",
        Month::June => "Jun",
        Month::July => "Jul",
        Month::August => "Aug",
        Month::September => "Sep",
        Month::October => "Oct",
        Month::November => "Nov",
        Month::December => "Dec",
    }
}

/// Accumulates page operations with a top-down flow cursor and simple pagination.
struct Canvas {
    pages: Vec<Vec<Op>>,
    ops: Vec<Op>,
    y: f64,
}

impl Canvas {
    fn new() -> Self {
        Self {
            pages: Vec::new(),
            ops: Vec::new(),
            y: TOP,
        }
    }

    fn lp(x: f64, y: f64) -> LinePoint {
        LinePoint {
            p: Point::new(mm(x), mm(y)),
            bezier: false,
        }
    }

    fn text(&mut self, x: f64, y: f64, size: f64, color: Rgb, s: &str) {
        self.ops.push(Op::StartTextSection);
        self.ops.push(Op::SetTextCursor {
            pos: Point::new(mm(x), mm(y)),
        });
        self.ops.push(Op::SetFont {
            font: PdfFontHandle::Builtin(BuiltinFont::Helvetica),
            size: pt(size),
        });
        self.ops.push(Op::SetLineHeight { lh: pt(size) });
        self.ops.push(Op::SetFillColor {
            col: Color::Rgb(color),
        });
        self.ops.push(Op::ShowText {
            items: vec![TextItem::Text(s.to_owned())],
        });
        self.ops.push(Op::EndTextSection);
    }

    fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, fill: Rgb) {
        self.ops.push(Op::SetFillColor {
            col: Color::Rgb(fill),
        });
        self.ops.push(Op::DrawPolygon {
            polygon: Polygon {
                rings: vec![PolygonRing {
                    points: vec![
                        Self::lp(x, y),
                        Self::lp(x + w, y),
                        Self::lp(x + w, y + h),
                        Self::lp(x, y + h),
                    ],
                }],
                mode: PaintMode::Fill,
                winding_order: WindingOrder::NonZero,
            },
        });
    }

    fn polyline(&mut self, pts: &[(f64, f64)], color: Rgb, thickness: f64) {
        if pts.len() < 2 {
            return;
        }
        self.ops.push(Op::SetOutlineColor {
            col: Color::Rgb(color),
        });
        self.ops.push(Op::SetOutlineThickness { pt: pt(thickness) });
        self.ops.push(Op::DrawLine {
            line: Line {
                points: pts.iter().map(|&(x, y)| Self::lp(x, y)).collect(),
                is_closed: false,
            },
        });
    }

    /// Push the current page and start a fresh one.
    fn page_break(&mut self) {
        let ops = mem::take(&mut self.ops);
        self.pages.push(ops);
        self.y = TOP;
    }

    /// Ensure `needed` mm of vertical space remain, else start a new page.
    /// Returns `true` when a page break fired.
    fn ensure(&mut self, needed: f64) -> bool {
        if self.y - needed < BOTTOM {
            self.page_break();
            return true;
        }
        false
    }

    /// Drop the flow cursor by `dy` mm.
    fn advance(&mut self, dy: f64) {
        self.y -= dy;
    }

    fn finish(mut self, title: &str) -> Vec<u8> {
        if !self.ops.is_empty() {
            self.page_break();
        }
        let pages: Vec<PdfPage> = self
            .pages
            .into_iter()
            .map(|ops| PdfPage::new(mm(PAGE_W), mm(PAGE_H), ops))
            .collect();
        let mut doc = PdfDocument::new(title);
        doc.with_pages(pages)
            .save(&PdfSaveOptions::default(), &mut Vec::new())
    }

    fn heading(&mut self, s: &str) {
        self.ensure(12.0);
        self.text(MARGIN, self.y, 20.0, ink(), s);
        self.advance(10.0);
    }

    fn subheading(&mut self, s: &str) {
        self.ensure(10.0);
        self.advance(2.0);
        self.text(MARGIN, self.y, 13.0, accent(), s);
        self.advance(6.0);
        self.polyline(
            &[(MARGIN, self.y + 1.0), (MARGIN + CONTENT_W, self.y + 1.0)],
            hairline(),
            0.4,
        );
        self.advance(4.0);
    }

    fn caption(&mut self, s: &str) {
        self.ensure(6.0);
        self.text(MARGIN, self.y, 9.0, muted(), s);
        self.advance(5.5);
    }

    /// A simple bar chart inside a fixed box; bars scaled to the max value.
    fn bar_chart(&mut self, data: &[(String, f64)], color: Rgb, box_w: f64, box_h: f64) {
        self.ensure(box_h + 8.0);
        let x0 = MARGIN;
        let base = self.y - box_h;
        self.polyline(&[(x0, base), (x0 + box_w, base)], hairline(), 0.5);
        let max = data
            .iter()
            .map(|(_, v)| *v)
            .fold(0.0_f64, f64::max)
            .max(1.0);
        let n = data.len().max(1) as f64;
        let slot = box_w / n;
        let bar_w = slot * 0.55;
        for (i, (label, v)) in data.iter().enumerate() {
            let bx = x0 + i as f64 * slot + (slot - bar_w) / 2.0;
            let bh = box_h * (v / max);
            self.rect(bx, base, bar_w, bh, color.clone());
            self.text(bx, base + bh + 1.5, 7.0, ink(), &fmt_int(*v));
            self.text(
                x0 + i as f64 * slot + 1.0,
                base - 4.0,
                6.5,
                muted(),
                &truncate(label, 9),
            );
        }
        self.advance(box_h + 8.0);
    }

    /// Multi-series line chart inside a fixed box. `series` is (label, points, color).
    fn line_chart(
        &mut self,
        series: &[(&str, Vec<f64>, Rgb)],
        x_labels: &[String],
        box_w: f64,
        box_h: f64,
    ) {
        self.ensure(box_h + 12.0);
        let x0 = MARGIN;
        let base = self.y - box_h;
        self.polyline(&[(x0, base), (x0 + box_w, base)], hairline(), 0.5);
        self.polyline(&[(x0, base), (x0, base + box_h)], hairline(), 0.5);
        let max = series
            .iter()
            .flat_map(|(_, pts, _)| pts.iter().copied())
            .fold(0.0_f64, f64::max)
            .max(1.0);
        let n = x_labels.len().max(2);
        let step = box_w / (n - 1) as f64;
        for (_, pts, color) in series {
            let mapped: Vec<(f64, f64)> = pts
                .iter()
                .enumerate()
                .map(|(i, v)| (x0 + i as f64 * step, base + box_h * (v.max(0.0) / max)))
                .collect();
            self.polyline(&mapped, color.clone(), 1.2);
        }
        for (i, label) in x_labels.iter().enumerate() {
            if i % 2 == 0 {
                self.text(x0 + i as f64 * step - 2.0, base - 4.0, 6.0, muted(), label);
            }
        }
        self.advance(box_h + 12.0);
        let mut lx = x0;
        for (name, _, color) in series {
            self.rect(lx, self.y, 3.0, 3.0, color.clone());
            self.text(lx + 4.5, self.y, 7.5, muted(), name);
            lx += 4.5 + (name.len() as f64) * 1.7 + 6.0;
        }
        self.advance(6.0);
    }
}

fn fmt_int(v: f64) -> String {
    format!("{}", v.round() as i64)
}

/// printpdf renderer for the company reports.
pub struct PrintPdfReportRenderer;

impl PrintPdfReportRenderer {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for PrintPdfReportRenderer {
    fn default() -> Self {
        Self::new()
    }
}

// Monthly per-group table column x-offsets (mm from left edge).
const COL_GROUP: f64 = MARGIN;
const COL_PROJ: f64 = MARGIN + 52.0;
const COL_PROG: f64 = MARGIN + 78.0;
const COL_STUCK: f64 = MARGIN + 96.0;
const COL_REQ: f64 = MARGIN + 116.0;
const COL_STAFF: f64 = MARGIN + 150.0;

fn group_table_header(c: &mut Canvas) {
    c.ensure(8.0);
    c.text(COL_GROUP, c.y, 8.0, muted(), "Group");
    c.text(COL_PROJ, c.y, 8.0, muted(), "Projects");
    c.text(COL_PROG, c.y, 8.0, muted(), "Avg %");
    c.text(COL_STUCK, c.y, 8.0, muted(), "Stuck");
    c.text(COL_REQ, c.y, 8.0, muted(), "Requests");
    c.text(COL_STAFF, c.y, 8.0, muted(), "Staff");
    c.advance(2.0);
    c.polyline(
        &[(MARGIN, c.y + 0.5), (MARGIN + CONTENT_W, c.y + 0.5)],
        hairline(),
        0.4,
    );
    c.advance(5.0);
}

fn group_table(c: &mut Canvas, groups: &[GroupReportRow]) {
    group_table_header(c);
    for g in groups {
        if c.ensure(6.5) {
            group_table_header(c);
        }
        c.text(COL_GROUP, c.y, 8.5, ink(), &truncate(&g.group_name, 26));
        c.text(
            COL_PROJ,
            c.y,
            8.5,
            ink(),
            &format!("{}/{}", g.projects_completed, g.projects_total),
        );
        c.text(
            COL_PROG,
            c.y,
            8.5,
            ink(),
            &format!("{}%", g.avg_project_progress),
        );
        let stuck_color = if g.projects_stuck > 0 {
            danger()
        } else {
            muted()
        };
        c.text(
            COL_STUCK,
            c.y,
            8.5,
            stuck_color,
            &g.projects_stuck.to_string(),
        );
        c.text(
            COL_REQ,
            c.y,
            8.5,
            ink(),
            &format!(
                "{}/{} ({}%)",
                g.requests_completed, g.requests_total, g.request_completion_pct
            ),
        );
        c.text(COL_STAFF, c.y, 8.5, ink(), &g.headcount.to_string());
        c.advance(6.5);
    }
}

fn tickets_section(c: &mut Canvas, t: &TicketStats) {
    c.subheading("IT Tickets");
    c.caption(&format!(
        "Created in period: {}    Resolved in period: {}{}",
        t.created_in_period,
        t.resolved_in_period,
        t.avg_resolve_hours
            .map(|h| format!("    Avg resolve: {h:.1}h"))
            .unwrap_or_default()
    ));
    let by_cat: Vec<(String, f64)> = t
        .by_category
        .iter()
        .map(|(cat, n)| (ticket_category_label(*cat).to_owned(), f64::from(*n)))
        .collect();
    c.bar_chart(&by_cat, accent(), CONTENT_W, 38.0);

    let status_line = t
        .by_status
        .iter()
        .map(|(s, n)| format!("{}: {n}", ticket_status_label(*s)))
        .collect::<Vec<_>>()
        .join("    ");
    c.caption(&status_line);
}

fn staff_section(c: &mut Canvas, s: &StaffSummary) {
    c.subheading("Staff");
    c.caption(&format!(
        "Company headcount: {}    New joiners: {}    Deactivations: {}",
        s.company_headcount, s.new_joiners, s.deactivations
    ));
}

impl ReportRenderer for PrintPdfReportRenderer {
    fn render_monthly(&self, data: &MonthlyReportData) -> Result<Vec<u8>, RenderError> {
        let mut c = Canvas::new();
        let (y, m) = period_label(data.period.start);
        c.heading(&format!("Monthly Report — {} {y}", month_name(m)));
        c.caption("Company-wide activity across all groups");
        c.advance(2.0);

        c.subheading("Groups");
        group_table(&mut c, &data.groups);

        tickets_section(&mut c, &data.tickets);
        staff_section(&mut c, &data.staff);

        Ok(c.finish("Monthly Report"))
    }

    fn render_yearly(&self, data: &YearlyReportData) -> Result<Vec<u8>, RenderError> {
        let mut c = Canvas::new();
        c.heading(&format!("Yearly Report — {}", data.year));
        c.caption("Year-over-year growth across the company");
        c.advance(2.0);

        let t = &data.totals;
        c.subheading("Headline");
        c.caption(&format!(
            "Headcount: {}  (net {:+})    New hires: {}    Departures: {}",
            t.company_headcount, t.net_headcount_change, t.new_hires, t.departures
        ));
        c.caption(&format!(
            "Tickets: {}    Projects completed: {}    Requests completed: {}",
            t.tickets_created, t.projects_completed, t.requests_completed
        ));

        let labels: Vec<String> = MONTHS.iter().map(|m| month_abbr(*m).to_owned()).collect();
        let headcount: Vec<f64> = data
            .growth
            .headcount
            .iter()
            .map(|p| p.value as f64)
            .collect();
        let tickets: Vec<f64> = data
            .growth
            .tickets_created
            .iter()
            .map(|p| p.value as f64)
            .collect();

        c.subheading("Headcount growth (cumulative net)");
        c.line_chart(
            &[("Headcount", headcount, accent())],
            &labels,
            CONTENT_W,
            42.0,
        );

        c.subheading("Activity over the year");
        let projects: Vec<f64> = data
            .growth
            .projects_completed
            .iter()
            .map(|p| p.value as f64)
            .collect();
        let requests: Vec<f64> = data
            .growth
            .requests_completed
            .iter()
            .map(|p| p.value as f64)
            .collect();
        c.line_chart(
            &[
                ("Tickets", tickets, warning()),
                ("Projects done", projects, success()),
                ("Requests done", requests, accent()),
            ],
            &labels,
            CONTENT_W,
            42.0,
        );

        Ok(c.finish("Yearly Report"))
    }

    fn render_staff_monthly(
        &self,
        subject_name: &str,
        data: &StaffMonthlyReport,
    ) -> Result<Vec<u8>, RenderError> {
        let mut c = Canvas::new();
        let (y, m) = period_label(data.period.start);
        c.heading(&format!("Staff Report — {} {y}", month_name(m)));
        c.caption(&format!(
            "{} — monthly attendance and workload",
            truncate(subject_name, 60)
        ));
        c.advance(2.0);

        c.subheading("Attendance");
        c.caption(&format!(
            "Days reported: {}    Work percentage: {}%    Overtime: {:.1}h",
            data.days_reported, data.work_percentage, data.overtime_hours
        ));
        c.caption(&format!(
            "Hours — request work: {:.1}    Learning: {:.1}    Other: {:.1}",
            data.hours_request_work, data.hours_learning, data.hours_other
        ));

        c.subheading("Leave");
        if data.leave_days_by_kind.is_empty() {
            c.caption("No leave taken this month");
        } else {
            let by_kind = data
                .leave_days_by_kind
                .iter()
                .map(|(kind, days)| format!("{}: {days:.1}", leave_kind_label(*kind)))
                .collect::<Vec<_>>()
                .join("    ");
            c.caption(&by_kind);
        }
        c.caption(&format!(
            "Balance remaining: {:.1}    Expiring soon: {:.1}",
            data.balance_remaining, data.balance_expiring_soon
        ));

        c.subheading("Flexible hours");
        c.caption(&format!(
            "Flex days: {}    Month delta: {:+.1}h",
            data.flex_days, data.flex_month_delta
        ));

        c.subheading("Requests");
        c.caption(&format!(
            "Completed: {}    Open: {}    Avg progress: {}%",
            data.requests_completed, data.requests_open, data.avg_request_progress
        ));

        Ok(c.finish("Staff Monthly Report"))
    }
}

fn period_label(start: OffsetDateTime) -> (i32, Month) {
    (start.year(), start.month())
}

fn ticket_status_label(s: TicketStatus) -> &'static str {
    match s {
        TicketStatus::Open => "Open",
        TicketStatus::Triaged => "Triaged",
        TicketStatus::Assigned => "Assigned",
        TicketStatus::InProgress => "In progress",
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

fn leave_kind_label(k: DayOffKind) -> &'static str {
    match k {
        DayOffKind::AnnualLeave => "Annual leave",
        DayOffKind::SickLeave => "Sick leave",
        DayOffKind::UnpaidLeave => "Unpaid leave",
        DayOffKind::Remote => "Remote",
        DayOffKind::Other => "Other",
    }
}
