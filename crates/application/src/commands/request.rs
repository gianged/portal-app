use std::fmt;

use domain::{ids::ProjectId, model::RequestPriority};
use time::OffsetDateTime;

#[derive(Debug, Clone)]
pub struct CreateRequestCommand {
    pub project_id: ProjectId,
    pub title: String,
    pub description: String,
    pub priority: RequestPriority,
    pub due_at: Option<OffsetDateTime>,
}

/// `None` leaves the field unchanged. `due_at` clearing is not expressible here;
/// a present `Some(_)` overwrites, absent leaves untouched.
#[derive(Debug, Clone, Default)]
pub struct UpdateRequestCommand {
    pub title: Option<String>,
    pub description: Option<String>,
    pub priority: Option<RequestPriority>,
    pub due_at: Option<OffsetDateTime>,
}

pub struct AddAttachmentCommand {
    pub filename: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

impl fmt::Debug for AddAttachmentCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AddAttachmentCommand")
            .field("filename", &self.filename)
            .field("content_type", &self.content_type)
            .field("bytes_len", &self.bytes.len())
            .finish()
    }
}
