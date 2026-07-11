use std::fmt;

use domain::error::{
    AuthzError, EventError, JobError, PresenceError, RateLimitError, RenderError, RepositoryError,
    StorageError, TokenRevocationError, TransitionError,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("entity not found: {0}")]
    NotFound(&'static str),

    #[error("validation failed: {0}")]
    Validation(String),

    #[error("forbidden")]
    Forbidden,

    #[error("conflict: {0}")]
    Conflict(ConflictCode),

    #[error(transparent)]
    Transition(#[from] TransitionError),

    #[error(transparent)]
    Repository(#[from] RepositoryError),

    #[error(transparent)]
    Storage(#[from] StorageError),

    #[error(transparent)]
    Event(#[from] EventError),
    #[error(transparent)]
    Job(#[from] JobError),
    #[error(transparent)]
    Render(#[from] RenderError),
    #[error(transparent)]
    Presence(#[from] PresenceError),
    #[error(transparent)]
    RateLimit(#[from] RateLimitError),
    #[error(transparent)]
    TokenRevocation(#[from] TokenRevocationError),

    /// Authz backend fault, kept distinct from datastore faults for triage.
    #[error("authz backend error: {0}")]
    Authz(String),

    /// In-process fault (password hashing, blocking-task join), kept distinct
    /// from backend faults for triage.
    #[error("internal error: {0}")]
    Internal(String),
}

impl From<AuthzError> for Error {
    fn from(err: AuthzError) -> Self {
        match err {
            AuthzError::Denied => Self::Forbidden,
            AuthzError::Backend(msg) => Self::Authz(msg),
        }
    }
}

/// Machine-readable business-rule conflict codes; the wire code is the
/// `snake_case` string clients match on, surfaced verbatim in 409 bodies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictCode {
    AssigneeNotActive,
    AssigneeNotEligible,
    AssigneeNotIt,
    AuthzWriteDenied,
    CannotDemoteLastLeader,
    CannotEditAnnouncement,
    ChatOverloaded,
    ChatUnavailable,
    DailyReportNotEditable,
    EmailAlreadyInUse,
    FlexAlreadyExistsForDate,
    FlexMonthlyCapReached,
    FromUserNotLeader,
    GroupAlreadyCollaborator,
    GroupAlreadyHasLeader,
    GroupHasActiveProjects,
    InsufficientLeaveBalance,
    InviteAlreadyPending,
    MembershipInactive,
    MessageDeleted,
    NoLeaveGrant,
    NotAwaitingHr,
    OvertimeMonthlyCapExceeded,
    ProjectNotActive,
    ReassignOpenRequests,
    RecipientNotActive,
    RequestNotEditable,
    ToUserInactive,
    TransferLeadershipFirst,
    UserAlreadyMember,
}

impl ConflictCode {
    /// The stable `snake_case` wire code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AssigneeNotActive => "assignee_not_active",
            Self::AssigneeNotEligible => "assignee_not_eligible",
            Self::AssigneeNotIt => "assignee_not_it",
            Self::AuthzWriteDenied => "authz_write_denied",
            Self::CannotDemoteLastLeader => "cannot_demote_last_leader",
            Self::CannotEditAnnouncement => "cannot_edit_announcement",
            Self::ChatOverloaded => "chat_overloaded",
            Self::ChatUnavailable => "chat_unavailable",
            Self::DailyReportNotEditable => "daily_report_not_editable",
            Self::EmailAlreadyInUse => "email_already_in_use",
            Self::FlexAlreadyExistsForDate => "flex_already_exists_for_date",
            Self::FlexMonthlyCapReached => "flex_monthly_cap_reached",
            Self::FromUserNotLeader => "from_user_not_leader",
            Self::GroupAlreadyCollaborator => "group_already_collaborator",
            Self::GroupAlreadyHasLeader => "group_already_has_leader",
            Self::GroupHasActiveProjects => "group_has_active_projects",
            Self::InsufficientLeaveBalance => "insufficient_leave_balance",
            Self::InviteAlreadyPending => "invite_already_pending",
            Self::MembershipInactive => "membership_inactive",
            Self::MessageDeleted => "message_deleted",
            Self::NoLeaveGrant => "no_leave_grant",
            Self::NotAwaitingHr => "not_awaiting_hr",
            Self::OvertimeMonthlyCapExceeded => "overtime_monthly_cap_exceeded",
            Self::ProjectNotActive => "project_not_active",
            Self::ReassignOpenRequests => "reassign_open_requests",
            Self::RecipientNotActive => "recipient_not_active",
            Self::RequestNotEditable => "request_not_editable",
            Self::ToUserInactive => "to_user_inactive",
            Self::TransferLeadershipFirst => "transfer_leadership_first",
            Self::UserAlreadyMember => "user_already_member",
        }
    }
}

impl fmt::Display for ConflictCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
