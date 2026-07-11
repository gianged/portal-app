use domain::model::{
    AuditAction, BalanceExpiryPolicy, DailyReportEntryKind, DailyReportStatus, DayOffKind,
    DayOffStatus, FlexStatus, GroupKind, GroupRole, LeaveTxnKind, NotificationKind, OvertimeStatus,
    ProjectInviteStatus, ProjectStatus, ReportKind, ReportScope, RequestPriority, RequestStatus,
    ServiceAccountStatus, SystemRole, TicketCategory, TicketPriority, TicketStatus, UserStatus,
};

// Mirrors a domain enum onto its Postgres type: the sqlx::Type enum plus the
// identity From impls in both directions.
macro_rules! sql_enum {
    ($sql:ident, $domain:ident, $type_name:literal, { $($variant:ident),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, sqlx::Type)]
        #[sqlx(type_name = $type_name, rename_all = "snake_case")]
        pub(crate) enum $sql {
            $($variant,)+
        }

        impl From<$domain> for $sql {
            fn from(v: $domain) -> Self {
                match v {
                    $($domain::$variant => Self::$variant,)+
                }
            }
        }

        impl From<$sql> for $domain {
            fn from(v: $sql) -> Self {
                match v {
                    $($sql::$variant => Self::$variant,)+
                }
            }
        }
    };
}

sql_enum!(SqlServiceAccountStatus, ServiceAccountStatus, "auth.service_account_status", {
    Active,
    Revoked,
});

sql_enum!(SqlUserStatus, UserStatus, "auth.user_status", {
    Pending,
    Active,
    Deactivated,
});

sql_enum!(SqlSystemRole, SystemRole, "auth.system_role", {
    Director,
    Hr,
});

sql_enum!(SqlBalanceExpiryPolicy, BalanceExpiryPolicy, "attendance.balance_expiry_policy", {
    Warn,
    RecordWorkPct,
});

sql_enum!(SqlDailyReportStatus, DailyReportStatus, "attendance.daily_report_status", {
    Draft,
    Submitted,
    Approved,
    Returned,
});

sql_enum!(SqlDailyReportEntryKind, DailyReportEntryKind, "attendance.daily_report_entry_kind", {
    RequestWork,
    Learning,
    Other,
});

sql_enum!(SqlLeaveTxnKind, LeaveTxnKind, "attendance.leave_txn_kind", {
    Grant,
    Consume,
    Refund,
    Adjust,
    Expire,
});

sql_enum!(SqlDayOffKind, DayOffKind, "attendance.dayoff_kind", {
    AnnualLeave,
    SickLeave,
    UnpaidLeave,
    Remote,
    Other,
});

sql_enum!(SqlDayOffStatus, DayOffStatus, "attendance.dayoff_status", {
    Pending,
    LeaderApproved,
    Approved,
    Rejected,
    Cancelled,
});

sql_enum!(SqlOvertimeStatus, OvertimeStatus, "attendance.overtime_status", {
    Pending,
    LeaderApproved,
    Approved,
    Rejected,
    Cancelled,
});

sql_enum!(SqlFlexStatus, FlexStatus, "attendance.flex_status", {
    Pending,
    Approved,
    Rejected,
    Cancelled,
});

sql_enum!(SqlAuditAction, AuditAction, "audit.audit_action", {
    Create,
    Update,
    Delete,
    StatusChange,
    Assign,
    Transfer,
});

sql_enum!(SqlGroupKind, GroupKind, "org.group_kind", {
    Standard,
    It,
});

sql_enum!(SqlGroupRole, GroupRole, "org.group_role", {
    Leader,
    SubLeader,
    Member,
});

sql_enum!(SqlInviteStatus, ProjectInviteStatus, "project.invite_status", {
    Pending,
    Accepted,
    Declined,
    Revoked,
});

sql_enum!(SqlProjectStatus, ProjectStatus, "project.project_status", {
    Planning,
    Active,
    OnHold,
    Completed,
    Cancelled,
});

sql_enum!(SqlRequestPriority, RequestPriority, "project.request_priority", {
    Low,
    Normal,
    High,
    Urgent,
});

sql_enum!(SqlRequestStatus, RequestStatus, "project.request_status", {
    Draft,
    Submitted,
    Assigned,
    InProgress,
    Review,
    Completed,
    Cancelled,
});

sql_enum!(SqlTicketCategory, TicketCategory, "ticket.ticket_category", {
    Hardware,
    Software,
    Access,
    Other,
});

sql_enum!(SqlTicketPriority, TicketPriority, "ticket.ticket_priority", {
    Low,
    Normal,
    High,
    Urgent,
});

sql_enum!(SqlTicketStatus, TicketStatus, "ticket.ticket_status", {
    Open,
    Triaged,
    Assigned,
    InProgress,
    Resolved,
    Closed,
    Reopened,
});

sql_enum!(SqlReportKind, ReportKind, "reporting.report_kind", {
    Monthly,
    Yearly,
});

sql_enum!(SqlReportScope, ReportScope, "reporting.report_scope", {
    Company,
    Group,
    Staff,
});

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
