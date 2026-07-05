use domain::model::{
    AuditAction, BalanceExpiryPolicy, DailyReportEntryKind, DailyReportStatus, DayOffKind,
    DayOffStatus, FlexStatus, GroupKind, GroupRole, LeaveTxnKind, NotificationKind, OvertimeStatus,
    ProjectInviteStatus, ProjectStatus, ReportKind, ReportScope, RequestPriority, RequestStatus,
    SystemRole, TicketCategory, TicketPriority, TicketStatus, UserStatus,
};

#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(type_name = "auth.user_status", rename_all = "snake_case")]
pub(crate) enum SqlUserStatus {
    Pending,
    Active,
    Deactivated,
}

impl From<UserStatus> for SqlUserStatus {
    fn from(v: UserStatus) -> Self {
        match v {
            UserStatus::Pending => Self::Pending,
            UserStatus::Active => Self::Active,
            UserStatus::Deactivated => Self::Deactivated,
        }
    }
}

impl From<SqlUserStatus> for UserStatus {
    fn from(v: SqlUserStatus) -> Self {
        match v {
            SqlUserStatus::Pending => Self::Pending,
            SqlUserStatus::Active => Self::Active,
            SqlUserStatus::Deactivated => Self::Deactivated,
        }
    }
}

#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(type_name = "auth.system_role", rename_all = "snake_case")]
pub(crate) enum SqlSystemRole {
    Director,
    Hr,
}

impl From<SystemRole> for SqlSystemRole {
    fn from(v: SystemRole) -> Self {
        match v {
            SystemRole::Director => Self::Director,
            SystemRole::Hr => Self::Hr,
        }
    }
}

impl From<SqlSystemRole> for SystemRole {
    fn from(v: SqlSystemRole) -> Self {
        match v {
            SqlSystemRole::Director => Self::Director,
            SqlSystemRole::Hr => Self::Hr,
        }
    }
}

#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(
    type_name = "attendance.balance_expiry_policy",
    rename_all = "snake_case"
)]
pub(crate) enum SqlBalanceExpiryPolicy {
    Warn,
    RecordWorkPct,
}

impl From<BalanceExpiryPolicy> for SqlBalanceExpiryPolicy {
    fn from(v: BalanceExpiryPolicy) -> Self {
        match v {
            BalanceExpiryPolicy::Warn => Self::Warn,
            BalanceExpiryPolicy::RecordWorkPct => Self::RecordWorkPct,
        }
    }
}

impl From<SqlBalanceExpiryPolicy> for BalanceExpiryPolicy {
    fn from(v: SqlBalanceExpiryPolicy) -> Self {
        match v {
            SqlBalanceExpiryPolicy::Warn => Self::Warn,
            SqlBalanceExpiryPolicy::RecordWorkPct => Self::RecordWorkPct,
        }
    }
}

#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(
    type_name = "attendance.daily_report_status",
    rename_all = "snake_case"
)]
pub(crate) enum SqlDailyReportStatus {
    Draft,
    Submitted,
    Approved,
    Returned,
}

impl From<DailyReportStatus> for SqlDailyReportStatus {
    fn from(v: DailyReportStatus) -> Self {
        match v {
            DailyReportStatus::Draft => Self::Draft,
            DailyReportStatus::Submitted => Self::Submitted,
            DailyReportStatus::Approved => Self::Approved,
            DailyReportStatus::Returned => Self::Returned,
        }
    }
}

impl From<SqlDailyReportStatus> for DailyReportStatus {
    fn from(v: SqlDailyReportStatus) -> Self {
        match v {
            SqlDailyReportStatus::Draft => Self::Draft,
            SqlDailyReportStatus::Submitted => Self::Submitted,
            SqlDailyReportStatus::Approved => Self::Approved,
            SqlDailyReportStatus::Returned => Self::Returned,
        }
    }
}

#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(
    type_name = "attendance.daily_report_entry_kind",
    rename_all = "snake_case"
)]
pub(crate) enum SqlDailyReportEntryKind {
    RequestWork,
    Learning,
    Other,
}

impl From<DailyReportEntryKind> for SqlDailyReportEntryKind {
    fn from(v: DailyReportEntryKind) -> Self {
        match v {
            DailyReportEntryKind::RequestWork => Self::RequestWork,
            DailyReportEntryKind::Learning => Self::Learning,
            DailyReportEntryKind::Other => Self::Other,
        }
    }
}

impl From<SqlDailyReportEntryKind> for DailyReportEntryKind {
    fn from(v: SqlDailyReportEntryKind) -> Self {
        match v {
            SqlDailyReportEntryKind::RequestWork => Self::RequestWork,
            SqlDailyReportEntryKind::Learning => Self::Learning,
            SqlDailyReportEntryKind::Other => Self::Other,
        }
    }
}

#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(type_name = "attendance.leave_txn_kind", rename_all = "snake_case")]
pub(crate) enum SqlLeaveTxnKind {
    Grant,
    Consume,
    Refund,
    Adjust,
    Expire,
}

impl From<LeaveTxnKind> for SqlLeaveTxnKind {
    fn from(v: LeaveTxnKind) -> Self {
        match v {
            LeaveTxnKind::Grant => Self::Grant,
            LeaveTxnKind::Consume => Self::Consume,
            LeaveTxnKind::Refund => Self::Refund,
            LeaveTxnKind::Adjust => Self::Adjust,
            LeaveTxnKind::Expire => Self::Expire,
        }
    }
}

impl From<SqlLeaveTxnKind> for LeaveTxnKind {
    fn from(v: SqlLeaveTxnKind) -> Self {
        match v {
            SqlLeaveTxnKind::Grant => Self::Grant,
            SqlLeaveTxnKind::Consume => Self::Consume,
            SqlLeaveTxnKind::Refund => Self::Refund,
            SqlLeaveTxnKind::Adjust => Self::Adjust,
            SqlLeaveTxnKind::Expire => Self::Expire,
        }
    }
}

#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(type_name = "attendance.dayoff_kind", rename_all = "snake_case")]
pub(crate) enum SqlDayOffKind {
    AnnualLeave,
    SickLeave,
    UnpaidLeave,
    Remote,
    Other,
}

impl From<DayOffKind> for SqlDayOffKind {
    fn from(v: DayOffKind) -> Self {
        match v {
            DayOffKind::AnnualLeave => Self::AnnualLeave,
            DayOffKind::SickLeave => Self::SickLeave,
            DayOffKind::UnpaidLeave => Self::UnpaidLeave,
            DayOffKind::Remote => Self::Remote,
            DayOffKind::Other => Self::Other,
        }
    }
}

impl From<SqlDayOffKind> for DayOffKind {
    fn from(v: SqlDayOffKind) -> Self {
        match v {
            SqlDayOffKind::AnnualLeave => Self::AnnualLeave,
            SqlDayOffKind::SickLeave => Self::SickLeave,
            SqlDayOffKind::UnpaidLeave => Self::UnpaidLeave,
            SqlDayOffKind::Remote => Self::Remote,
            SqlDayOffKind::Other => Self::Other,
        }
    }
}

#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(type_name = "attendance.dayoff_status", rename_all = "snake_case")]
pub(crate) enum SqlDayOffStatus {
    Pending,
    LeaderApproved,
    Approved,
    Rejected,
    Cancelled,
}

impl From<DayOffStatus> for SqlDayOffStatus {
    fn from(v: DayOffStatus) -> Self {
        match v {
            DayOffStatus::Pending => Self::Pending,
            DayOffStatus::LeaderApproved => Self::LeaderApproved,
            DayOffStatus::Approved => Self::Approved,
            DayOffStatus::Rejected => Self::Rejected,
            DayOffStatus::Cancelled => Self::Cancelled,
        }
    }
}

impl From<SqlDayOffStatus> for DayOffStatus {
    fn from(v: SqlDayOffStatus) -> Self {
        match v {
            SqlDayOffStatus::Pending => Self::Pending,
            SqlDayOffStatus::LeaderApproved => Self::LeaderApproved,
            SqlDayOffStatus::Approved => Self::Approved,
            SqlDayOffStatus::Rejected => Self::Rejected,
            SqlDayOffStatus::Cancelled => Self::Cancelled,
        }
    }
}

#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(type_name = "attendance.overtime_status", rename_all = "snake_case")]
pub(crate) enum SqlOvertimeStatus {
    Pending,
    LeaderApproved,
    Approved,
    Rejected,
    Cancelled,
}

impl From<OvertimeStatus> for SqlOvertimeStatus {
    fn from(v: OvertimeStatus) -> Self {
        match v {
            OvertimeStatus::Pending => Self::Pending,
            OvertimeStatus::LeaderApproved => Self::LeaderApproved,
            OvertimeStatus::Approved => Self::Approved,
            OvertimeStatus::Rejected => Self::Rejected,
            OvertimeStatus::Cancelled => Self::Cancelled,
        }
    }
}

impl From<SqlOvertimeStatus> for OvertimeStatus {
    fn from(v: SqlOvertimeStatus) -> Self {
        match v {
            SqlOvertimeStatus::Pending => Self::Pending,
            SqlOvertimeStatus::LeaderApproved => Self::LeaderApproved,
            SqlOvertimeStatus::Approved => Self::Approved,
            SqlOvertimeStatus::Rejected => Self::Rejected,
            SqlOvertimeStatus::Cancelled => Self::Cancelled,
        }
    }
}

#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(type_name = "attendance.flex_status", rename_all = "snake_case")]
pub(crate) enum SqlFlexStatus {
    Pending,
    Approved,
    Rejected,
    Cancelled,
}

impl From<FlexStatus> for SqlFlexStatus {
    fn from(v: FlexStatus) -> Self {
        match v {
            FlexStatus::Pending => Self::Pending,
            FlexStatus::Approved => Self::Approved,
            FlexStatus::Rejected => Self::Rejected,
            FlexStatus::Cancelled => Self::Cancelled,
        }
    }
}

impl From<SqlFlexStatus> for FlexStatus {
    fn from(v: SqlFlexStatus) -> Self {
        match v {
            SqlFlexStatus::Pending => Self::Pending,
            SqlFlexStatus::Approved => Self::Approved,
            SqlFlexStatus::Rejected => Self::Rejected,
            SqlFlexStatus::Cancelled => Self::Cancelled,
        }
    }
}

#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(type_name = "audit.audit_action", rename_all = "snake_case")]
pub(crate) enum SqlAuditAction {
    Create,
    Update,
    Delete,
    StatusChange,
    Assign,
    Transfer,
}

impl From<AuditAction> for SqlAuditAction {
    fn from(v: AuditAction) -> Self {
        match v {
            AuditAction::Create => Self::Create,
            AuditAction::Update => Self::Update,
            AuditAction::Delete => Self::Delete,
            AuditAction::StatusChange => Self::StatusChange,
            AuditAction::Assign => Self::Assign,
            AuditAction::Transfer => Self::Transfer,
        }
    }
}

impl From<SqlAuditAction> for AuditAction {
    fn from(v: SqlAuditAction) -> Self {
        match v {
            SqlAuditAction::Create => Self::Create,
            SqlAuditAction::Update => Self::Update,
            SqlAuditAction::Delete => Self::Delete,
            SqlAuditAction::StatusChange => Self::StatusChange,
            SqlAuditAction::Assign => Self::Assign,
            SqlAuditAction::Transfer => Self::Transfer,
        }
    }
}

// Write-only: `kind` is never read back (the JSONB `payload` carries the tag), so no reverse `From` impl by design.
#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(
    type_name = "notification.notification_kind",
    rename_all = "snake_case"
)]
pub(crate) enum SqlNotificationKind {
    Announcement,
    Mention,
    TicketUrgent,
    RequestAssigned,
    RequestStatusChange,
    ProjectInvite,
    TicketAssigned,
    TicketStatusChange,
    ProjectInviteResponse,
    TicketRaised,
    RequestComment,
    TicketComment,
    System,
}

impl From<NotificationKind> for SqlNotificationKind {
    fn from(v: NotificationKind) -> Self {
        match v {
            NotificationKind::Announcement => Self::Announcement,
            NotificationKind::Mention => Self::Mention,
            NotificationKind::TicketUrgent => Self::TicketUrgent,
            NotificationKind::RequestAssigned => Self::RequestAssigned,
            NotificationKind::RequestStatusChange => Self::RequestStatusChange,
            NotificationKind::ProjectInvite => Self::ProjectInvite,
            NotificationKind::TicketAssigned => Self::TicketAssigned,
            NotificationKind::TicketStatusChange => Self::TicketStatusChange,
            NotificationKind::ProjectInviteResponse => Self::ProjectInviteResponse,
            NotificationKind::TicketRaised => Self::TicketRaised,
            NotificationKind::RequestComment => Self::RequestComment,
            NotificationKind::TicketComment => Self::TicketComment,
            NotificationKind::System => Self::System,
        }
    }
}

#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(type_name = "org.group_kind", rename_all = "snake_case")]
pub(crate) enum SqlGroupKind {
    Standard,
    It,
}

impl From<GroupKind> for SqlGroupKind {
    fn from(v: GroupKind) -> Self {
        match v {
            GroupKind::Standard => Self::Standard,
            GroupKind::It => Self::It,
        }
    }
}

impl From<SqlGroupKind> for GroupKind {
    fn from(v: SqlGroupKind) -> Self {
        match v {
            SqlGroupKind::Standard => Self::Standard,
            SqlGroupKind::It => Self::It,
        }
    }
}

#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(type_name = "org.group_role", rename_all = "snake_case")]
pub(crate) enum SqlGroupRole {
    Leader,
    SubLeader,
    Member,
}

impl From<GroupRole> for SqlGroupRole {
    fn from(v: GroupRole) -> Self {
        match v {
            GroupRole::Leader => Self::Leader,
            GroupRole::SubLeader => Self::SubLeader,
            GroupRole::Member => Self::Member,
        }
    }
}

impl From<SqlGroupRole> for GroupRole {
    fn from(v: SqlGroupRole) -> Self {
        match v {
            SqlGroupRole::Leader => Self::Leader,
            SqlGroupRole::SubLeader => Self::SubLeader,
            SqlGroupRole::Member => Self::Member,
        }
    }
}

#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(type_name = "project.invite_status", rename_all = "snake_case")]
pub(crate) enum SqlInviteStatus {
    Pending,
    Accepted,
    Declined,
    Revoked,
}

impl From<ProjectInviteStatus> for SqlInviteStatus {
    fn from(v: ProjectInviteStatus) -> Self {
        match v {
            ProjectInviteStatus::Pending => Self::Pending,
            ProjectInviteStatus::Accepted => Self::Accepted,
            ProjectInviteStatus::Declined => Self::Declined,
            ProjectInviteStatus::Revoked => Self::Revoked,
        }
    }
}

impl From<SqlInviteStatus> for ProjectInviteStatus {
    fn from(v: SqlInviteStatus) -> Self {
        match v {
            SqlInviteStatus::Pending => Self::Pending,
            SqlInviteStatus::Accepted => Self::Accepted,
            SqlInviteStatus::Declined => Self::Declined,
            SqlInviteStatus::Revoked => Self::Revoked,
        }
    }
}

#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(type_name = "project.project_status", rename_all = "snake_case")]
pub(crate) enum SqlProjectStatus {
    Planning,
    Active,
    OnHold,
    Completed,
    Cancelled,
}

impl From<ProjectStatus> for SqlProjectStatus {
    fn from(v: ProjectStatus) -> Self {
        match v {
            ProjectStatus::Planning => Self::Planning,
            ProjectStatus::Active => Self::Active,
            ProjectStatus::OnHold => Self::OnHold,
            ProjectStatus::Completed => Self::Completed,
            ProjectStatus::Cancelled => Self::Cancelled,
        }
    }
}

impl From<SqlProjectStatus> for ProjectStatus {
    fn from(v: SqlProjectStatus) -> Self {
        match v {
            SqlProjectStatus::Planning => Self::Planning,
            SqlProjectStatus::Active => Self::Active,
            SqlProjectStatus::OnHold => Self::OnHold,
            SqlProjectStatus::Completed => Self::Completed,
            SqlProjectStatus::Cancelled => Self::Cancelled,
        }
    }
}

#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(type_name = "project.request_priority", rename_all = "snake_case")]
pub(crate) enum SqlRequestPriority {
    Low,
    Normal,
    High,
    Urgent,
}

impl From<RequestPriority> for SqlRequestPriority {
    fn from(v: RequestPriority) -> Self {
        match v {
            RequestPriority::Low => Self::Low,
            RequestPriority::Normal => Self::Normal,
            RequestPriority::High => Self::High,
            RequestPriority::Urgent => Self::Urgent,
        }
    }
}

impl From<SqlRequestPriority> for RequestPriority {
    fn from(v: SqlRequestPriority) -> Self {
        match v {
            SqlRequestPriority::Low => Self::Low,
            SqlRequestPriority::Normal => Self::Normal,
            SqlRequestPriority::High => Self::High,
            SqlRequestPriority::Urgent => Self::Urgent,
        }
    }
}

#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(type_name = "project.request_status", rename_all = "snake_case")]
pub(crate) enum SqlRequestStatus {
    Draft,
    Submitted,
    Assigned,
    InProgress,
    Review,
    Completed,
    Cancelled,
}

impl From<RequestStatus> for SqlRequestStatus {
    fn from(v: RequestStatus) -> Self {
        match v {
            RequestStatus::Draft => Self::Draft,
            RequestStatus::Submitted => Self::Submitted,
            RequestStatus::Assigned => Self::Assigned,
            RequestStatus::InProgress => Self::InProgress,
            RequestStatus::Review => Self::Review,
            RequestStatus::Completed => Self::Completed,
            RequestStatus::Cancelled => Self::Cancelled,
        }
    }
}

impl From<SqlRequestStatus> for RequestStatus {
    fn from(v: SqlRequestStatus) -> Self {
        match v {
            SqlRequestStatus::Draft => Self::Draft,
            SqlRequestStatus::Submitted => Self::Submitted,
            SqlRequestStatus::Assigned => Self::Assigned,
            SqlRequestStatus::InProgress => Self::InProgress,
            SqlRequestStatus::Review => Self::Review,
            SqlRequestStatus::Completed => Self::Completed,
            SqlRequestStatus::Cancelled => Self::Cancelled,
        }
    }
}

#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(type_name = "ticket.ticket_category", rename_all = "snake_case")]
pub(crate) enum SqlTicketCategory {
    Hardware,
    Software,
    Access,
    Other,
}

impl From<TicketCategory> for SqlTicketCategory {
    fn from(v: TicketCategory) -> Self {
        match v {
            TicketCategory::Hardware => Self::Hardware,
            TicketCategory::Software => Self::Software,
            TicketCategory::Access => Self::Access,
            TicketCategory::Other => Self::Other,
        }
    }
}

impl From<SqlTicketCategory> for TicketCategory {
    fn from(v: SqlTicketCategory) -> Self {
        match v {
            SqlTicketCategory::Hardware => Self::Hardware,
            SqlTicketCategory::Software => Self::Software,
            SqlTicketCategory::Access => Self::Access,
            SqlTicketCategory::Other => Self::Other,
        }
    }
}

#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(type_name = "ticket.ticket_priority", rename_all = "snake_case")]
pub(crate) enum SqlTicketPriority {
    Low,
    Normal,
    High,
    Urgent,
}

impl From<TicketPriority> for SqlTicketPriority {
    fn from(v: TicketPriority) -> Self {
        match v {
            TicketPriority::Low => Self::Low,
            TicketPriority::Normal => Self::Normal,
            TicketPriority::High => Self::High,
            TicketPriority::Urgent => Self::Urgent,
        }
    }
}

impl From<SqlTicketPriority> for TicketPriority {
    fn from(v: SqlTicketPriority) -> Self {
        match v {
            SqlTicketPriority::Low => Self::Low,
            SqlTicketPriority::Normal => Self::Normal,
            SqlTicketPriority::High => Self::High,
            SqlTicketPriority::Urgent => Self::Urgent,
        }
    }
}

#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(type_name = "ticket.ticket_status", rename_all = "snake_case")]
pub(crate) enum SqlTicketStatus {
    Open,
    Triaged,
    Assigned,
    InProgress,
    Resolved,
    Closed,
    Reopened,
}

impl From<TicketStatus> for SqlTicketStatus {
    fn from(v: TicketStatus) -> Self {
        match v {
            TicketStatus::Open => Self::Open,
            TicketStatus::Triaged => Self::Triaged,
            TicketStatus::Assigned => Self::Assigned,
            TicketStatus::InProgress => Self::InProgress,
            TicketStatus::Resolved => Self::Resolved,
            TicketStatus::Closed => Self::Closed,
            TicketStatus::Reopened => Self::Reopened,
        }
    }
}

impl From<SqlTicketStatus> for TicketStatus {
    fn from(v: SqlTicketStatus) -> Self {
        match v {
            SqlTicketStatus::Open => Self::Open,
            SqlTicketStatus::Triaged => Self::Triaged,
            SqlTicketStatus::Assigned => Self::Assigned,
            SqlTicketStatus::InProgress => Self::InProgress,
            SqlTicketStatus::Resolved => Self::Resolved,
            SqlTicketStatus::Closed => Self::Closed,
            SqlTicketStatus::Reopened => Self::Reopened,
        }
    }
}

#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(type_name = "reporting.report_kind", rename_all = "snake_case")]
pub(crate) enum SqlReportKind {
    Monthly,
    Yearly,
}

impl From<ReportKind> for SqlReportKind {
    fn from(v: ReportKind) -> Self {
        match v {
            ReportKind::Monthly => Self::Monthly,
            ReportKind::Yearly => Self::Yearly,
        }
    }
}

impl From<SqlReportKind> for ReportKind {
    fn from(v: SqlReportKind) -> Self {
        match v {
            SqlReportKind::Monthly => Self::Monthly,
            SqlReportKind::Yearly => Self::Yearly,
        }
    }
}

#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(type_name = "reporting.report_scope", rename_all = "snake_case")]
pub(crate) enum SqlReportScope {
    Company,
    Group,
    Staff,
}

impl From<ReportScope> for SqlReportScope {
    fn from(v: ReportScope) -> Self {
        match v {
            ReportScope::Company => Self::Company,
            ReportScope::Group => Self::Group,
            ReportScope::Staff => Self::Staff,
        }
    }
}

impl From<SqlReportScope> for ReportScope {
    fn from(v: SqlReportScope) -> Self {
        match v {
            SqlReportScope::Company => Self::Company,
            SqlReportScope::Group => Self::Group,
            SqlReportScope::Staff => Self::Staff,
        }
    }
}
