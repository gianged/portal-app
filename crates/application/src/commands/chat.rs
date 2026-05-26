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
