//! Boundary mapping between `domain` types and the wire DTOs in `shared`.
//!
//! `shared` cannot depend on `domain` (it targets wasm), so the orphan rule rules
//! out `impl From`. These free functions are the one place projection lives;
//! handlers map here and never leak domain types over HTTP. Denormalized DTOs that
//! embed `UserSummaryDto` take the resolved summary as an argument; see `crate::resolve`.
//!
//! Split by entity into submodules and re-exported here so call sites stay
//! `dto::<fn>`; id projections and the `id_map!` macro live in this parent.

mod audit;
mod chat;
mod comment;
mod daily_report;
mod day_off;
mod ext;
mod flex_hours;
mod group;
mod holiday;
mod leave_balance;
mod notification;
mod overtime;
mod policy;
mod project;
mod report;
mod request;
mod service_account;
mod ticket;
mod user;

use time::Time;

use domain::ids;
use shared::{dto::ids as wire, errors::SharedError};

pub use self::{
    audit::{audit_action_dto, audit_log_dto},
    chat::{
        announcement_dto, channel_dto, channel_kind_dto, channel_summary_dto, chat_attachment_dto,
        message_created_at, message_dto, post_announcement_command, post_message_command,
    },
    comment::comment_dto,
    daily_report::{daily_report_dto, review_daily_report_command, upsert_daily_report_command},
    day_off::{
        create_day_off_command, day_off_dto, day_off_kind_domain, day_off_kind_dto,
        day_off_status_dto, decide_day_off_command,
    },
    ext::{ext_project_dto, ext_request_dto},
    flex_hours::{decide_flex_command, flex_hours_dto, flex_status_dto, request_flex_command},
    group::{
        add_membership_command, create_group_command, group_dto, group_kind_domain, group_kind_dto,
        group_role_domain, group_role_dto, group_summary_dto, membership_dto,
        unknown_group_summary, update_group_metadata_command,
    },
    holiday::holiday_dto,
    leave_balance::{
        adjust_balance_command, leave_balance_dto, leave_grant_dto, leave_statement_dto,
        leave_transaction_dto, leave_txn_kind_dto, set_leave_grant_command,
    },
    notification::{notification_dto, notification_payload_dto},
    overtime::{
        create_overtime_command, decide_overtime_command, overtime_dto, overtime_status_dto,
    },
    policy::{policy_dto, update_policy_command},
    project::{
        create_project_command, project_collaborator_dto, project_dto, project_invite_dto,
        project_invite_status_dto, project_status_dto, update_project_metadata_command,
    },
    report::{monthly_report_dto, report_summary_dto, staff_monthly_report_dto, yearly_report_dto},
    request::{
        create_request_command, request_attachment_dto, request_dto, request_priority_domain,
        request_priority_dto, request_status_domain, request_status_dto, update_request_command,
    },
    service_account::{
        created_service_account_dto, service_account_dto, service_account_scope_domain,
        service_account_status_dto,
    },
    ticket::{
        raise_ticket_command, ticket_category_domain, ticket_category_dto, ticket_dto,
        ticket_priority_domain, ticket_priority_dto, ticket_status_dto,
    },
    user::{
        create_user_command, resolve_user_role, system_role_domain, system_role_dto,
        unknown_user_summary, update_profile_command, user_dto, user_membership_dto,
        user_profile_dto, user_status_dto, user_summary_dto,
    },
};

//
// Domain and wire id newtypes mirror each other one-for-one; both wrap `Uuid`.
// Not every id is projected by a current route, so the maps carry `allow(dead_code)`.

macro_rules! id_map {
    ($fn:ident, $ty:ident) => {
        #[must_use]
        #[allow(dead_code)]
        pub fn $fn(id: ids::$ty) -> wire::$ty {
            wire::$ty(id.0)
        }
    };
}

id_map!(user_id, UserId);
id_map!(group_id, GroupId);
id_map!(membership_id, MembershipId);
id_map!(project_id, ProjectId);
id_map!(project_collaborator_id, ProjectCollaboratorId);
id_map!(project_invite_id, ProjectInviteId);
id_map!(request_id, RequestId);
id_map!(request_attachment_id, RequestAttachmentId);
id_map!(ticket_id, TicketId);
id_map!(channel_id, ChannelId);
id_map!(message_id, MessageId);
id_map!(chat_attachment_id, ChatAttachmentId);
id_map!(comment_id, CommentId);
id_map!(notification_id, NotificationId);
id_map!(audit_log_id, AuditLogId);
id_map!(report_id, ReportId);
id_map!(daily_report_id, DailyReportId);
id_map!(daily_report_entry_id, DailyReportEntryId);
id_map!(leave_grant_id, LeaveGrantId);
id_map!(leave_transaction_id, LeaveTransactionId);
id_map!(day_off_id, DayOffId);
id_map!(overtime_id, OvertimeId);
id_map!(flex_hours_id, FlexHoursId);
id_map!(flex_segment_id, FlexSegmentId);
id_map!(service_account_id, ServiceAccountId);

/// `HH:MM` wire form of a time-of-day, shared by the policy and flex projections.
fn fmt_time(t: Time) -> String {
    format!("{:02}:{:02}", t.hour(), t.minute())
}

/// Parses a wire `HH:MM` field, naming `field` in the validation error.
/// `policy` here is the sibling dto module, so the validation path stays full.
fn to_time(s: &str, field: &str) -> Result<Time, SharedError> {
    let (h, m) = shared::validation::policy::parse_hhmm(s)
        .ok_or_else(|| SharedError::Validation(format!("{field} must be a valid HH:MM time")))?;
    Time::from_hms(h, m, 0)
        .map_err(|_| SharedError::Validation(format!("{field} is not a valid time")))
}
