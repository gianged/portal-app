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
#[derive(Debug, Clone)]
pub struct AddChatAttachmentCommand {
    pub filename: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}
