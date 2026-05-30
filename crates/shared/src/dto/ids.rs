use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Wire-side ID newtypes. These mirror `domain::ids` one-for-one but live here
/// because `shared` cannot depend on `domain` (it compiles to wasm). They
/// serialize as a bare UUID string (`#[serde(transparent)]`), matching the
/// default newtype encoding the server emits when projecting from domain types.
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
id_newtype!(NotificationId);
id_newtype!(AuditLogId);
