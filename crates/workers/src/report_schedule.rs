//! Monthly company-report scheduler: on/after the configured day of the month,
//! generate the previous month's report (idempotently) and email the PDF to every
//! Director/HR recipient. Runs on a fixed interval; the durable archive guard
//! inside `generate_and_store_monthly` keeps restarts from double-generating or
//! double-emailing, and the per-recipient email enqueue is best-effort.

use std::sync::Arc;
use std::time::Duration as StdDuration;

use time::OffsetDateTime;

use application::{GeneratedReport, ReportService};
use domain::ports::{
    job_queue::JobQueue,
    mailer::{EmailAttachment, EmailMessage},
};

pub async fn run(
    reports: Arc<ReportService>,
    email_queue: Arc<dyn JobQueue>,
    day_of_month: u8,
    interval: StdDuration,
) {
    let mut ticker = tokio::time::interval(interval);
    loop {
        ticker.tick().await;
        let now = OffsetDateTime::now_utc();
        if now.day() < day_of_month {
            continue;
        }
        let (year, month) = previous_month(now);
        match reports.generate_and_store_monthly(year, month).await {
            Ok(generated) if generated.created => {
                tracing::info!(year, month, "monthly report generated; emailing recipients");
                email_report(&reports, &email_queue, &generated, year, month).await;
            }
            Ok(_) => {
                tracing::debug!(year, month, "monthly report already exists; nothing to do");
            }
            Err(e) => tracing::error!(error = %e, year, month, "monthly report generation failed"),
        }
    }
}

/// The previous calendar month relative to `now`.
fn previous_month(now: OffsetDateTime) -> (i32, u8) {
    let (year, month) = (now.year(), u8::from(now.month()));
    if month == 1 {
        (year - 1, 12)
    } else {
        (year, month - 1)
    }
}

async fn email_report(
    reports: &Arc<ReportService>,
    email_queue: &Arc<dyn JobQueue>,
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
                if let Err(e) = email_queue.enqueue("emails", &bytes).await {
                    tracing::error!(error = %e, to = %message.to, "report email enqueue failed");
                }
            }
            Err(e) => tracing::error!(error = %e, "report email serialization failed"),
        }
    }
}
