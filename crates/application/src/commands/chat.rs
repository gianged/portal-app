use std::fmt;

use domain::ids::{ChannelId, UserId};

#[derive(Debug, Clone)]
pub struct PostMessageCommand {
    pub channel_id: ChannelId,
    pub body: String,
    pub mentions: Vec<UserId>,
    pub attachment_keys: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PostAnnouncementCommand {
    pub channel_id: ChannelId,
    pub body: String,
}

/// One chat upload (mirrors `AddAttachmentCommand` for requests).
#[derive(Clone)]
pub struct AddChatAttachmentCommand {
    pub filename: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

impl fmt::Debug for AddChatAttachmentCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AddChatAttachmentCommand")
            .field("filename", &self.filename)
            .field("content_type", &self.content_type)
            .field("bytes_len", &self.bytes.len())
            .finish()
    }
}
