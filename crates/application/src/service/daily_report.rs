use std::sync::Arc;

use domain::{
    ids::{DailyReportEntryId, DailyReportId, GroupId, UserId},
    model::{DailyReport, DailyReportEntry, DailyReportEntryKind, DailyReportStatus, GroupRole},
    repository::{DailyReportRepository, GroupRepository},
};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use crate::{
    commands::daily_report::{
        DailyReportEntryInput, ReviewDailyReportCommand, UpsertDailyReportCommand,
    },
    error::{Error, Result},
    events::{DomainEvent, EventBus},
    permissions::Permissions,
    service::request::RequestService,
};

/// Daily reports: staff describe their day as a summary plus typed entries, then
/// submit for their leader to approve or return. `RequestWork` entries can carry
/// a progress hint that bumps the linked request.
pub struct DailyReportService {
    reports: Arc<dyn DailyReportRepository>,
    groups: Arc<dyn GroupRepository>,
    request: Arc<RequestService>,
    perms: Arc<Permissions>,
    events: Arc<EventBus>,
}

impl DailyReportService {
    #[must_use]
    pub fn new(
        reports: Arc<dyn DailyReportRepository>,
        groups: Arc<dyn GroupRepository>,
        request: Arc<RequestService>,
        perms: Arc<Permissions>,
        events: Arc<EventBus>,
    ) -> Self {
        Self {
            reports,
            groups,
            request,
            perms,
            events,
        }
    }

    /// Creates or replaces the actor's draft report for `report_date`. Only a
    /// `Draft` or `Returned` report may be edited; summary and entries are
    /// replaced wholesale.
    ///
    /// Each `RequestWork` entry carrying a `progress` hint best-effort bumps its
    /// linked request (skipping requests the actor doesn't own or that aren't in
    /// progress). The hint is transient: the entries table has no progress
    /// column, so the bump happens here where the command still carries it.
    ///
    /// # Errors
    /// Returns `NotFound` if the actor does not exist, `Forbidden` if the actor is not active, `Conflict` if an existing report is already submitted or approved, or a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor))]
    pub async fn upsert_draft(
        &self,
        actor: UserId,
        cmd: UpsertDailyReportCommand,
    ) -> Result<DailyReport> {
        self.perms.require_active(actor).await?;
        let now = OffsetDateTime::now_utc();
        let mut report = match self
            .reports
            .find_by_user_date(actor, cmd.report_date)
            .await?
        {
            Some(existing) => {
                if !matches!(
                    existing.status,
                    DailyReportStatus::Draft | DailyReportStatus::Returned
                ) {
                    return Err(Error::Conflict("daily_report_not_editable".into()));
                }
                existing
            }
            None => DailyReport {
                id: DailyReportId(Uuid::now_v7()),
                user_id: actor,
                report_date: cmd.report_date,
                status: DailyReportStatus::Draft,
                summary: String::new(),
                entries: Vec::new(),
                submitted_at: None,
                reviewed_by: None,
                reviewed_at: None,
                review_note: String::new(),
                created_at: now,
                updated_at: now,
            },
        };

        report.summary = cmd.summary;
        report.entries = cmd
            .entries
            .iter()
            .map(|e| DailyReportEntry {
                id: DailyReportEntryId(Uuid::now_v7()),
                daily_report_id: report.id,
                kind: e.kind,
                description: e.description.clone(),
                request_id: e.request_id,
                hours: e.hours,
                created_at: now,
            })
            .collect();
        report.updated_at = now;
        self.reports.save(&report).await?;

        self.apply_progress_hints(actor, &cmd.entries).await;
        Ok(report)
    }

    /// Submits the actor's report for review. Owner-only.
    ///
    /// # Errors
    /// Returns `NotFound` if the report does not exist, `Forbidden` if the actor is not the owner, `Transition` if the report is not in a submittable state, or a repository or event error if the datastore or event bus is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, id = ?id))]
    pub async fn submit(&self, actor: UserId, id: DailyReportId) -> Result<DailyReport> {
        let mut report = self.load(id).await?;
        if report.user_id != actor {
            return Err(Error::Forbidden);
        }
        let now = OffsetDateTime::now_utc();
        report.submit(now)?;
        self.reports.save(&report).await?;
        self.events
            .emit(DomainEvent::DailyReportSubmitted {
                report_id: report.id,
                user_id: report.user_id,
                actor,
                at: now,
            })
            .await?;
        Ok(report)
    }

    /// Leader approves or returns a submitted report.
    ///
    /// # Errors
    /// Returns `NotFound` if the report does not exist, `Forbidden` if the actor does not lead the owner's group, `Transition` if the report is not under review, or a repository or event error if the datastore or event bus is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, id = ?id))]
    pub async fn review(
        &self,
        actor: UserId,
        id: DailyReportId,
        cmd: ReviewDailyReportCommand,
    ) -> Result<DailyReport> {
        let mut report = self.load(id).await?;
        self.perms
            .require_leader_of_member(actor, report.user_id)
            .await?;
        let now = OffsetDateTime::now_utc();
        if cmd.approve {
            report.approve(actor, cmd.note, now)?;
        } else {
            report.return_for_edits(actor, cmd.note, now)?;
        }
        self.reports.save(&report).await?;
        self.events
            .emit(DomainEvent::DailyReportReviewed {
                report_id: report.id,
                user_id: report.user_id,
                approved: cmd.approve,
                actor,
                at: now,
            })
            .await?;
        Ok(report)
    }

    /// Single report, readable by its owner, a leader of the owner's group, or HR.
    ///
    /// # Errors
    /// Returns `NotFound` if the report does not exist, `Forbidden` if the actor is none of owner / leader / HR, or a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, id = ?id))]
    pub async fn get(&self, actor: UserId, id: DailyReportId) -> Result<DailyReport> {
        let report = self.load(id).await?;
        if report.user_id == actor {
            return Ok(report);
        }
        if self
            .perms
            .is_leader_of_member(actor, report.user_id)
            .await?
            || self.perms.is_hr(actor).await?
        {
            return Ok(report);
        }
        Err(Error::Forbidden)
    }

    /// The actor's own reports within `[from, to]`.
    ///
    /// # Errors
    /// Returns `NotFound` if the actor does not exist, `Forbidden` if the actor is not active, or a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor))]
    pub async fn list_mine(&self, actor: UserId, from: Date, to: Date) -> Result<Vec<DailyReport>> {
        self.perms.require_active(actor).await?;
        Ok(self.reports.list_for_user(actor, from, to).await?)
    }

    /// Every active member's reports in `group` within `[from, to]`. Gated to the
    /// group's leader or HR.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is neither the group's leader nor HR, `NotFound` if the actor does not exist, or a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, group = ?group))]
    pub async fn list_for_group(
        &self,
        actor: UserId,
        group: GroupId,
        from: Date,
        to: Date,
    ) -> Result<Vec<DailyReport>> {
        if self.groups.find_group(group).await?.is_none() {
            return Err(Error::NotFound("group"));
        }
        let is_leader = matches!(
            self.perms.group_role(actor, group).await?,
            Some(GroupRole::Leader)
        );
        if !is_leader {
            self.perms.require_hr(actor).await?;
        }
        Ok(self.reports.list_for_group(group, from, to).await?)
    }

    /// Best-effort request-progress bumps for `RequestWork` entries that carry a
    /// hint. A non-assignee, missing, or non-in-progress request is silently
    /// skipped; only unexpected errors propagate.
    async fn apply_progress_hints(&self, actor: UserId, entries: &[DailyReportEntryInput]) {
        for entry in entries {
            if entry.kind != DailyReportEntryKind::RequestWork {
                continue;
            }
            let (Some(request_id), Some(progress)) = (entry.request_id, entry.progress) else {
                continue;
            };
            match self.request.set_progress(actor, request_id, progress).await {
                Ok(_) => {}
                Err(Error::Forbidden | Error::NotFound(_) | Error::Transition(_)) => {}
                Err(e) => tracing::warn!(error = %e, "daily report progress hint failed"),
            }
        }
    }

    async fn load(&self, id: DailyReportId) -> Result<DailyReport> {
        self.reports
            .find_by_id(id)
            .await?
            .ok_or(Error::NotFound("daily_report"))
    }
}
