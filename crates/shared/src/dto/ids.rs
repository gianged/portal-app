use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Wire-side ID newtypes mirroring `domain::ids`, re-declared here because
/// `shared` compiles to wasm and cannot depend on `domain`. They serialize as a
/// bare UUID string (`#[serde(transparent)]`).
macro_rules! id_newtype {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);
    };
}

id_newtype!(UserId);
id_newtype!(GroupId);
id_newtype!(MembershipId);
id_newtype!(ProjectId);
id_newtype!(ProjectCollaboratorId);
id_newtype!(ProjectInviteId);
id_newtype!(RequestId);
id_newtype!(RequestAttachmentId);
id_newtype!(TicketId);
id_newtype!(ChannelId);
id_newtype!(MessageId);
id_newtype!(ChatAttachmentId);
id_newtype!(CommentId);
id_newtype!(NotificationId);
id_newtype!(AuditLogId);
id_newtype!(ReportId);
