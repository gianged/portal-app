//! Boundary mapping between `domain` types and the wire DTOs in `shared`.
//!
//! `shared` cannot depend on `domain` (it targets wasm), so the orphan rule
//! rules out `impl From<domain::X> for shared::Y`. These free functions are the
//! one place that projection lives; handlers map here and never leak domain
//! types over HTTP. Denormalized DTOs that embed `UserSummaryDto` take the
//! resolved summary as an argument — see `crate::resolve`.

use application::commands::{
    ticket::RaiseTicketCommand,
    user::{CreateUserCommand, UpdateProfileCommand},
};
use domain::{ids, model};
use shared::dto::{
    common::UserSummaryDto,
    ids as wire,
    notification::{NotificationDto, NotificationPayloadDto},
    request::RequestStatus as WireRequestStatus,
    ticket::{
        RaiseTicketRequest, TicketCategory as WireTicketCategory, TicketDto,
        TicketPriority as WireTicketPriority, TicketStatus as WireTicketStatus,
    },
    user::{
        CreateUserRequest, SystemRole as WireSystemRole, UpdateProfileRequest, UserDto,
        UserProfileDto, UserRole, UserStatus as WireUserStatus,
    },
};

use application::commands::chat::{PostAnnouncementCommand, PostMessageCommand};
use application::commands::group::{
    AddMembershipCommand, CreateGroupCommand, UpdateGroupMetadataCommand,
};
use application::commands::project::{CreateProjectCommand, UpdateProjectMetadataCommand};
use application::commands::request::{CreateRequestCommand, UpdateRequestCommand};
use shared::dto::announcement::{AnnouncementDto, PostAnnouncementRequest};
use shared::dto::chat::{
    ChannelDto, ChannelKind as WireChannelKind, ChannelSummaryDto, MessageDto, SendMessageRequest,
};
use shared::dto::common::GroupSummaryDto;
use shared::dto::group::{
    AddMemberRequest, CreateGroupRequest, GroupDto, GroupKind as WireGroupKind,
    GroupRole as WireGroupRole, MembershipDto, UpdateGroupRequest,
};
use shared::dto::project::{
    CreateProjectRequest, ProjectCollaboratorDto, ProjectDto, ProjectInviteDto,
    ProjectInviteStatus as WireProjectInviteStatus, ProjectStatus as WireProjectStatus,
    UpdateProjectMetadataRequest,
};
use shared::dto::request::{
    CreateRequestRequest, RequestAttachmentDto, RequestDto, RequestPriority as WireRequestPriority,
    UpdateRequestRequest,
};
use time::OffsetDateTime;

// --- id projections ---
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
id_map!(notification_id, NotificationId);
id_map!(audit_log_id, AuditLogId);

// --- user enums ---

#[must_use]
pub fn system_role_dto(role: model::SystemRole) -> WireSystemRole {
    match role {
        model::SystemRole::Director => WireSystemRole::Director,
        model::SystemRole::Hr => WireSystemRole::Hr,
    }
}

#[must_use]
pub fn system_role_domain(role: WireSystemRole) -> model::SystemRole {
    match role {
        WireSystemRole::Director => model::SystemRole::Director,
        WireSystemRole::Hr => model::SystemRole::Hr,
    }
}

#[must_use]
pub fn user_status_dto(status: model::UserStatus) -> WireUserStatus {
    match status {
        model::UserStatus::Pending => WireUserStatus::Pending,
        model::UserStatus::Active => WireUserStatus::Active,
        model::UserStatus::Deactivated => WireUserStatus::Deactivated,
    }
}

/// Flattens the domain's split identity (`SystemRole` + per-group `GroupRole` +
/// `GroupKind::It`) into the single synthetic display role the UI shows.
/// Precedence: Director > HR > IT > Group Leader > Group Sub-leader > Member.
#[must_use]
pub fn resolve_user_role(
    system_role: Option<model::SystemRole>,
    group_roles: &[model::GroupRole],
    is_it: bool,
) -> UserRole {
    match system_role {
        Some(model::SystemRole::Director) => return UserRole::Director,
        Some(model::SystemRole::Hr) => return UserRole::Hr,
        None => {}
    }
    if is_it {
        UserRole::It
    } else if group_roles.contains(&model::GroupRole::Leader) {
        UserRole::GroupLeader
    } else if group_roles.contains(&model::GroupRole::SubLeader) {
        UserRole::GroupSubLeader
    } else {
        UserRole::Member
    }
}

// --- user views ---

#[must_use]
pub fn user_dto(user: &model::User, role: UserRole) -> UserDto {
    UserDto {
        id: user_id(user.id),
        name: user.full_name.clone(),
        email: user.email.clone(),
        role,
        group_name: None,
    }
}

#[must_use]
pub fn user_profile_dto(user: &model::User) -> UserProfileDto {
    UserProfileDto {
        id: user_id(user.id),
        email: user.email.clone(),
        full_name: user.full_name.clone(),
        avatar_storage_key: user.avatar_storage_key.clone(),
        phone: user.phone.clone(),
        timezone: user.timezone.clone(),
        status: user_status_dto(user.status),
        system_role: user.system_role.map(system_role_dto),
        created_at: user.created_at,
    }
}

#[must_use]
pub fn user_summary_dto(user: &model::User, role: UserRole) -> UserSummaryDto {
    UserSummaryDto {
        id: user_id(user.id),
        full_name: user.full_name.clone(),
        avatar_storage_key: user.avatar_storage_key.clone(),
        role,
    }
}

/// Fallback for a dangling user reference (should not occur for FK-backed ids);
/// keeps a denormalized response renderable instead of failing the whole call.
#[must_use]
pub fn unknown_user_summary(id: ids::UserId) -> UserSummaryDto {
    UserSummaryDto {
        id: user_id(id),
        full_name: "Unknown user".to_owned(),
        avatar_storage_key: None,
        role: UserRole::Member,
    }
}

// --- user commands ---

#[must_use]
pub fn create_user_command(req: CreateUserRequest) -> CreateUserCommand {
    CreateUserCommand {
        email: req.email,
        password: req.password,
        full_name: req.full_name,
        phone: req.phone,
        timezone: req.timezone,
        system_role: req.system_role.map(system_role_domain),
    }
}

#[must_use]
pub fn update_profile_command(req: UpdateProfileRequest) -> UpdateProfileCommand {
    UpdateProfileCommand {
        full_name: req.full_name,
        phone: req.phone,
        timezone: req.timezone,
        avatar_storage_key: req.avatar_storage_key,
    }
}

// --- request status (shared by requests + notification payloads) ---

#[must_use]
pub fn request_status_dto(status: model::RequestStatus) -> WireRequestStatus {
    match status {
        model::RequestStatus::Draft => WireRequestStatus::Draft,
        model::RequestStatus::Submitted => WireRequestStatus::Submitted,
        model::RequestStatus::Assigned => WireRequestStatus::Assigned,
        model::RequestStatus::InProgress => WireRequestStatus::InProgress,
        model::RequestStatus::Review => WireRequestStatus::Review,
        model::RequestStatus::Completed => WireRequestStatus::Completed,
        model::RequestStatus::Cancelled => WireRequestStatus::Cancelled,
    }
}

#[must_use]
pub fn request_status_domain(status: WireRequestStatus) -> model::RequestStatus {
    match status {
        WireRequestStatus::Draft => model::RequestStatus::Draft,
        WireRequestStatus::Submitted => model::RequestStatus::Submitted,
        WireRequestStatus::Assigned => model::RequestStatus::Assigned,
        WireRequestStatus::InProgress => model::RequestStatus::InProgress,
        WireRequestStatus::Review => model::RequestStatus::Review,
        WireRequestStatus::Completed => model::RequestStatus::Completed,
        WireRequestStatus::Cancelled => model::RequestStatus::Cancelled,
    }
}

// --- tickets ---

#[must_use]
pub fn ticket_status_dto(status: model::TicketStatus) -> WireTicketStatus {
    match status {
        model::TicketStatus::Open => WireTicketStatus::Open,
        model::TicketStatus::Triaged => WireTicketStatus::Triaged,
        model::TicketStatus::Assigned => WireTicketStatus::Assigned,
        model::TicketStatus::InProgress => WireTicketStatus::InProgress,
        model::TicketStatus::Resolved => WireTicketStatus::Resolved,
        model::TicketStatus::Closed => WireTicketStatus::Closed,
        model::TicketStatus::Reopened => WireTicketStatus::Reopened,
    }
}

#[must_use]
pub fn ticket_priority_dto(priority: model::TicketPriority) -> WireTicketPriority {
    match priority {
        model::TicketPriority::Low => WireTicketPriority::Low,
        model::TicketPriority::Normal => WireTicketPriority::Normal,
        model::TicketPriority::High => WireTicketPriority::High,
        model::TicketPriority::Urgent => WireTicketPriority::Urgent,
    }
}

#[must_use]
pub fn ticket_priority_domain(priority: WireTicketPriority) -> model::TicketPriority {
    match priority {
        WireTicketPriority::Low => model::TicketPriority::Low,
        WireTicketPriority::Normal => model::TicketPriority::Normal,
        WireTicketPriority::High => model::TicketPriority::High,
        WireTicketPriority::Urgent => model::TicketPriority::Urgent,
    }
}

#[must_use]
pub fn ticket_category_dto(category: model::TicketCategory) -> WireTicketCategory {
    match category {
        model::TicketCategory::Hardware => WireTicketCategory::Hardware,
        model::TicketCategory::Software => WireTicketCategory::Software,
        model::TicketCategory::Access => WireTicketCategory::Access,
        model::TicketCategory::Other => WireTicketCategory::Other,
    }
}

#[must_use]
pub fn ticket_category_domain(category: WireTicketCategory) -> model::TicketCategory {
    match category {
        WireTicketCategory::Hardware => model::TicketCategory::Hardware,
        WireTicketCategory::Software => model::TicketCategory::Software,
        WireTicketCategory::Access => model::TicketCategory::Access,
        WireTicketCategory::Other => model::TicketCategory::Other,
    }
}

#[must_use]
pub fn raise_ticket_command(req: RaiseTicketRequest) -> RaiseTicketCommand {
    RaiseTicketCommand {
        title: req.title,
        description: req.description,
        category: ticket_category_domain(req.category),
    }
}

/// Builds a `TicketDto` from a ticket plus its already-resolved user summaries.
#[must_use]
pub fn ticket_dto(
    ticket: &model::Ticket,
    requester: UserSummaryDto,
    assignee: Option<UserSummaryDto>,
) -> TicketDto {
    TicketDto {
        id: ticket_id(ticket.id),
        requester,
        assignee,
        title: ticket.title.clone(),
        description: ticket.description.clone(),
        status: ticket_status_dto(ticket.status),
        priority: ticket.priority.map(ticket_priority_dto),
        category: ticket_category_dto(ticket.category),
        triaged_at: ticket.triaged_at,
        resolved_at: ticket.resolved_at,
        closed_at: ticket.closed_at,
        created_at: ticket.created_at,
        updated_at: ticket.updated_at,
    }
}

// --- notifications ---

#[must_use]
pub fn notification_payload_dto(payload: &model::NotificationPayload) -> NotificationPayloadDto {
    match payload {
        model::NotificationPayload::Announcement {
            announcement_id,
            channel_id,
        } => NotificationPayloadDto::Announcement {
            announcement_id: message_id(*announcement_id),
            channel_id: channel_id_wire(*channel_id),
        },
        model::NotificationPayload::Mention {
            message_id: msg,
            channel_id,
            mentioned_by,
        } => NotificationPayloadDto::Mention {
            message_id: message_id(*msg),
            channel_id: channel_id_wire(*channel_id),
            mentioned_by: user_id(*mentioned_by),
        },
        model::NotificationPayload::TicketUrgent { ticket_id: tid } => {
            NotificationPayloadDto::TicketUrgent {
                ticket_id: ticket_id(*tid),
            }
        }
        model::NotificationPayload::RequestAssigned { request_id: rid } => {
            NotificationPayloadDto::RequestAssigned {
                request_id: request_id(*rid),
            }
        }
        model::NotificationPayload::RequestStatusChange {
            request_id: rid,
            from,
            to,
        } => NotificationPayloadDto::RequestStatusChange {
            request_id: request_id(*rid),
            from: request_status_dto(*from),
            to: request_status_dto(*to),
        },
        model::NotificationPayload::ProjectInvite {
            invite_id,
            project_id: pid,
        } => NotificationPayloadDto::ProjectInvite {
            invite_id: project_invite_id(*invite_id),
            project_id: project_id(*pid),
        },
        model::NotificationPayload::System { message } => NotificationPayloadDto::System {
            message: message.clone(),
        },
    }
}

#[must_use]
pub fn notification_dto(notification: &model::Notification) -> NotificationDto {
    NotificationDto {
        id: notification_id(notification.id),
        payload: notification_payload_dto(&notification.payload),
        read: notification.read_at.is_some(),
        created_at: notification.created_at,
    }
}

// `channel_id` collides with a local binding name in the match above; alias it.
#[must_use]
#[allow(dead_code)]
fn channel_id_wire(id: ids::ChannelId) -> wire::ChannelId {
    wire::ChannelId(id.0)
}

// --- groups ---

#[must_use]
pub fn group_kind_dto(kind: model::GroupKind) -> WireGroupKind {
    match kind {
        model::GroupKind::Standard => WireGroupKind::Standard,
        model::GroupKind::It => WireGroupKind::It,
    }
}

#[must_use]
pub fn group_kind_domain(kind: WireGroupKind) -> model::GroupKind {
    match kind {
        WireGroupKind::Standard => model::GroupKind::Standard,
        WireGroupKind::It => model::GroupKind::It,
    }
}

#[must_use]
pub fn group_role_dto(role: model::GroupRole) -> WireGroupRole {
    match role {
        model::GroupRole::Leader => WireGroupRole::Leader,
        model::GroupRole::SubLeader => WireGroupRole::SubLeader,
        model::GroupRole::Member => WireGroupRole::Member,
    }
}

#[must_use]
pub fn group_role_domain(role: WireGroupRole) -> model::GroupRole {
    match role {
        WireGroupRole::Leader => model::GroupRole::Leader,
        WireGroupRole::SubLeader => model::GroupRole::SubLeader,
        WireGroupRole::Member => model::GroupRole::Member,
    }
}

#[must_use]
pub fn group_dto(group: &model::Group, member_count: u32) -> GroupDto {
    GroupDto {
        id: group_id(group.id),
        name: group.name.clone(),
        description: group.description.clone(),
        kind: group_kind_dto(group.kind),
        member_count,
        created_at: group.created_at,
    }
}

#[must_use]
pub fn membership_dto(membership: &model::Membership, user: UserSummaryDto) -> MembershipDto {
    MembershipDto {
        id: membership_id(membership.id),
        user,
        role: group_role_dto(membership.role),
        joined_at: membership.joined_at,
        active: membership.deactivated_at.is_none(),
    }
}

#[must_use]
pub fn create_group_command(req: CreateGroupRequest) -> CreateGroupCommand {
    CreateGroupCommand {
        name: req.name,
        description: req.description,
        kind: group_kind_domain(req.kind),
    }
}

#[must_use]
pub fn update_group_metadata_command(req: UpdateGroupRequest) -> UpdateGroupMetadataCommand {
    UpdateGroupMetadataCommand {
        name: req.name,
        description: req.description,
    }
}

#[must_use]
pub fn add_membership_command(group: ids::GroupId, req: &AddMemberRequest) -> AddMembershipCommand {
    AddMembershipCommand {
        group_id: group,
        user_id: ids::UserId(req.user_id.0),
        role: group_role_domain(req.role),
    }
}

// --- groups: summaries ---

#[must_use]
pub fn group_summary_dto(group: &model::Group) -> GroupSummaryDto {
    GroupSummaryDto {
        id: group_id(group.id),
        name: group.name.clone(),
        kind: group_kind_dto(group.kind),
    }
}

#[must_use]
pub fn unknown_group_summary(id: ids::GroupId) -> GroupSummaryDto {
    GroupSummaryDto {
        id: group_id(id),
        name: "Unknown group".to_owned(),
        kind: WireGroupKind::Standard,
    }
}

// --- projects ---

#[must_use]
pub fn project_status_dto(status: model::ProjectStatus) -> WireProjectStatus {
    match status {
        model::ProjectStatus::Planning => WireProjectStatus::Planning,
        model::ProjectStatus::Active => WireProjectStatus::Active,
        model::ProjectStatus::OnHold => WireProjectStatus::OnHold,
        model::ProjectStatus::Completed => WireProjectStatus::Completed,
        model::ProjectStatus::Cancelled => WireProjectStatus::Cancelled,
    }
}

#[must_use]
pub fn project_invite_status_dto(status: model::ProjectInviteStatus) -> WireProjectInviteStatus {
    match status {
        model::ProjectInviteStatus::Pending => WireProjectInviteStatus::Pending,
        model::ProjectInviteStatus::Accepted => WireProjectInviteStatus::Accepted,
        model::ProjectInviteStatus::Declined => WireProjectInviteStatus::Declined,
        model::ProjectInviteStatus::Revoked => WireProjectInviteStatus::Revoked,
    }
}

#[must_use]
pub fn project_dto(
    project: &model::Project,
    owner_group: GroupSummaryDto,
    created_by: UserSummaryDto,
) -> ProjectDto {
    ProjectDto {
        id: project_id(project.id),
        owner_group,
        created_by,
        name: project.name.clone(),
        description: project.description.clone(),
        status: project_status_dto(project.status),
        created_at: project.created_at,
        updated_at: project.updated_at,
    }
}

#[must_use]
pub fn project_collaborator_dto(
    collaborator: &model::ProjectCollaborator,
    group: GroupSummaryDto,
) -> ProjectCollaboratorDto {
    ProjectCollaboratorDto {
        id: project_collaborator_id(collaborator.id),
        group,
        created_at: collaborator.created_at,
    }
}

#[must_use]
pub fn project_invite_dto(
    invite: &model::ProjectInvite,
    invited_by: UserSummaryDto,
    invited_group: GroupSummaryDto,
    responded_by: Option<UserSummaryDto>,
) -> ProjectInviteDto {
    ProjectInviteDto {
        id: project_invite_id(invite.id),
        project_id: project_id(invite.project_id),
        invited_by,
        invited_group,
        responded_by,
        status: project_invite_status_dto(invite.status),
        responded_at: invite.responded_at,
        created_at: invite.created_at,
    }
}

#[must_use]
pub fn create_project_command(req: CreateProjectRequest) -> CreateProjectCommand {
    CreateProjectCommand {
        owner_group_id: ids::GroupId(req.owner_group_id.0),
        name: req.name,
        description: req.description,
    }
}

#[must_use]
pub fn update_project_metadata_command(
    req: UpdateProjectMetadataRequest,
) -> UpdateProjectMetadataCommand {
    UpdateProjectMetadataCommand {
        name: req.name,
        description: req.description,
    }
}

// --- requests ---

#[must_use]
pub fn request_priority_dto(priority: model::RequestPriority) -> WireRequestPriority {
    match priority {
        model::RequestPriority::Low => WireRequestPriority::Low,
        model::RequestPriority::Normal => WireRequestPriority::Normal,
        model::RequestPriority::High => WireRequestPriority::High,
        model::RequestPriority::Urgent => WireRequestPriority::Urgent,
    }
}

#[must_use]
pub fn request_priority_domain(priority: WireRequestPriority) -> model::RequestPriority {
    match priority {
        WireRequestPriority::Low => model::RequestPriority::Low,
        WireRequestPriority::Normal => model::RequestPriority::Normal,
        WireRequestPriority::High => model::RequestPriority::High,
        WireRequestPriority::Urgent => model::RequestPriority::Urgent,
    }
}

#[must_use]
pub fn request_dto(
    request: &model::Request,
    creator: UserSummaryDto,
    assignee: Option<UserSummaryDto>,
) -> RequestDto {
    RequestDto {
        id: request_id(request.id),
        project_id: project_id(request.project_id),
        creator,
        assignee,
        title: request.title.clone(),
        description: request.description.clone(),
        status: request_status_dto(request.status),
        priority: request_priority_dto(request.priority),
        due_at: request.due_at,
        created_at: request.created_at,
        updated_at: request.updated_at,
    }
}

#[must_use]
pub fn request_attachment_dto(
    attachment: &model::RequestAttachment,
    uploaded_by: UserSummaryDto,
) -> RequestAttachmentDto {
    RequestAttachmentDto {
        id: request_attachment_id(attachment.id),
        filename: attachment.filename.clone(),
        content_type: attachment.content_type.clone(),
        size_bytes: attachment.size_bytes,
        uploaded_by,
        created_at: attachment.created_at,
    }
}

#[must_use]
pub fn create_request_command(req: CreateRequestRequest) -> CreateRequestCommand {
    CreateRequestCommand {
        project_id: ids::ProjectId(req.project_id.0),
        title: req.title,
        description: req.description,
        priority: request_priority_domain(req.priority),
        due_at: req.due_at,
    }
}

#[must_use]
pub fn update_request_command(req: UpdateRequestRequest) -> UpdateRequestCommand {
    UpdateRequestCommand {
        title: req.title,
        description: req.description,
        priority: req.priority.map(request_priority_domain),
        due_at: req.due_at,
    }
}

// --- chat + announcements ---

#[must_use]
pub fn channel_kind_dto(kind: model::ChannelKind) -> WireChannelKind {
    match kind {
        model::ChannelKind::Group => WireChannelKind::Group,
        model::ChannelKind::General => WireChannelKind::General,
        model::ChannelKind::Direct => WireChannelKind::Direct,
    }
}

/// `other_user` is required for a direct channel (the participant who is not the
/// viewer, resolved by the caller); ignored for group/general channels.
#[must_use]
pub fn channel_dto(channel: &model::Channel, other_user: Option<UserSummaryDto>) -> ChannelDto {
    match channel {
        model::Channel::Group(c) => ChannelDto::Group {
            id: channel_id(c.id),
            group_id: group_id(c.group_id),
            name: c.name.clone(),
        },
        model::Channel::General(c) => ChannelDto::General {
            id: channel_id(c.id),
        },
        model::Channel::Direct(c) => ChannelDto::Direct {
            id: channel_id(c.id),
            other_user: other_user.unwrap_or_else(|| unknown_user_summary(c.user_high_id)),
        },
    }
}

#[must_use]
pub fn channel_summary_dto(
    membership: &model::ChannelMembership,
    title: String,
    unread: bool,
    last_message_at: Option<OffsetDateTime>,
) -> ChannelSummaryDto {
    ChannelSummaryDto {
        id: channel_id(membership.channel_id),
        kind: channel_kind_dto(membership.kind),
        title,
        unread,
        last_message_at,
    }
}

/// Recovers a message's creation time from its time-ordered (v7) id, mirroring
/// how the application layer derives it (the `Message` row stores no timestamp).
#[must_use]
pub fn message_created_at(id: ids::MessageId) -> OffsetDateTime {
    let ts =
        id.0.get_timestamp()
            .expect("message ids are UUIDv7 and always carry a timestamp");
    let (secs, nanos) = ts.to_unix();
    let total = i128::from(secs) * 1_000_000_000 + i128::from(nanos);
    OffsetDateTime::from_unix_timestamp_nanos(total)
        .expect("a UUIDv7 timestamp is within OffsetDateTime's range")
}

#[must_use]
pub fn message_dto(
    message: &model::Message,
    sender: UserSummaryDto,
    mentions: Vec<UserSummaryDto>,
) -> MessageDto {
    MessageDto {
        id: message_id(message.id),
        channel_id: channel_id(message.channel_id),
        sender,
        body: message.body.clone(),
        mentions,
        attachment_keys: message.attachment_keys.clone(),
        is_announcement: message.is_announcement,
        edited_at: message.edited_at,
        deleted_at: message.deleted_at,
        created_at: message_created_at(message.id),
    }
}

#[must_use]
pub fn announcement_dto(
    announcement: &model::Announcement,
    sender: UserSummaryDto,
    now: OffsetDateTime,
) -> AnnouncementDto {
    AnnouncementDto {
        id: message_id(announcement.id),
        channel_id: channel_id(announcement.channel_id),
        sender,
        body: announcement.body.clone(),
        edited_at: announcement.edited_at,
        created_at: announcement.created_at,
        editable: announcement.within_edit_grace(now),
    }
}

#[must_use]
pub fn post_message_command(
    channel: ids::ChannelId,
    req: SendMessageRequest,
) -> PostMessageCommand {
    PostMessageCommand {
        channel_id: channel,
        body: req.body,
        mentions: req.mentions.into_iter().map(|u| ids::UserId(u.0)).collect(),
        attachment_keys: req.attachment_keys,
    }
}

#[must_use]
pub fn post_announcement_command(req: PostAnnouncementRequest) -> PostAnnouncementCommand {
    PostAnnouncementCommand {
        channel_id: ids::ChannelId(req.channel_id.0),
        body: req.body,
    }
}
