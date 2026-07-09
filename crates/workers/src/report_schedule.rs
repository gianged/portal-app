//! Monthly report scheduler: on/after the configured day, generates the previous
//! month's company report (idempotently), emails the PDF to Director/HR
//! recipients, and archives a per-staff PDF for every active user.

use std::sync::Arc;

use time::OffsetDateTime;

use application::{GeneratedReport, ReportService};
use domain::ports::{
    job_queue::{JobQueue, QUEUE_EMAILS},
    mailer::{EmailAttachment, EmailMessage},
};

pub async fn run(
    reports: Arc<ReportService>,
    email_queue: Arc<dyn JobQueue>,
    day_of_month: u8,
    interval: std::time::Duration,
) {
    let mut ticker = tokio::time::interval(interval);
    loop {
        ticker.tick().await;
        let now = OffsetDateTime::now_utc();
        // Clamp so months shorter than the configured day still trigger on their last day.
        let due = day_of_month.min(now.month().length(now.year()));
        if now.day() < due {
            continue;
        }
        let (year, month) = previous_month(now);
        match reports.generate_and_store_monthly(year, month).await {
            Ok(generated) if generated.created => {
                tracing::info!(year, month, "monthly report generated; emailing recipients");
                email_report(&reports, &*email_queue, &generated, year, month).await;
            }
            Ok(_) => {
                tracing::debug!(year, month, "monthly report already exists; nothing to do");
            }
            Err(e) => tracing::error!(error = %e, year, month, "monthly report generation failed"),
        }
        // Per-staff archival is idempotent, so re-running on every tick only
        // fills gaps (users who failed last time or activated since).
        match reports.archive_staff_monthly_reports(year, month).await {
            Ok(outcome) if outcome.created > 0 || outcome.failed > 0 => {
                tracing::info!(
                    year,
                    month,
                    created = outcome.created,
                    skipped = outcome.skipped,
                    failed = outcome.failed,
                    "staff report archival sweep finished"
                );
            }
            Ok(_) => {
                tracing::debug!(year, month, "staff report archive already complete");
            }
            Err(e) => {
                tracing::error!(error = %e, year, month, "staff report archival sweep failed");
            }
        }
    }
}

fn previous_month(now: OffsetDateTime) -> (i32, u8) {
    let (year, month) = (now.year(), u8::from(now.month()));
    if month == 1 {
        (year - 1, 12)
    } else {
        (year, month - 1)
    }
}

async fn email_report(
    reports: &ReportService,
    email_queue: &dyn JobQueue,
    generated: &GeneratedReport,
    year: i32,
    month: u8,
) {
    let recipients = match reports.list_admin_recipients().await {
        Ok(recipients) => recipients,
        Err(e) => {
            tracing::error!(error = %e, "report email: recipient lookup failed");
            return;
        }
    };
    let filename = format!("company-report-{year:04}-{month:02}.pdf");
    for (email, _user_id) in recipients {
        let message = EmailMessage {
            to: email,
            subject: format!("[Portal] Monthly company report — {year:04}-{month:02}"),
            body: "Attached is the monthly company report.".to_owned(),
            attachments: vec![EmailAttachment {
                filename: filename.clone(),
                content_type: "application/pdf".to_owned(),
                bytes: generated.bytes.clone(),
            }],
        };
        match serde_json::to_vec(&message) {
            Ok(bytes) => {
                if let Err(e) = email_queue.enqueue(QUEUE_EMAILS, &bytes).await {
                    tracing::error!(error = %e, to = %message.to, "report email enqueue failed");
                }
            }
            Err(e) => tracing::error!(error = %e, "report email serialization failed"),
        }
    }
}
