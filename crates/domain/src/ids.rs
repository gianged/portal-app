use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! id_newtype {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
id_newtype!(DailyReportId);
id_newtype!(DailyReportEntryId);
id_newtype!(LeaveGrantId);
id_newtype!(LeaveTransactionId);
id_newtype!(DayOffId);
id_newtype!(OvertimeId);
id_newtype!(FlexHoursId);
id_newtype!(FlexSegmentId);
id_newtype!(ServiceAccountId);
