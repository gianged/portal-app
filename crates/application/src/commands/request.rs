use std::fmt;

use domain::{ids::ProjectId, request::RequestPriority};
use time::OffsetDateTime;

#[derive(Debug, Clone)]
pub struct CreateRequestCommand {
    pub project_id: ProjectId,
    pub title: String,
    pub description: String,
    pub priority: RequestPriority,
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
