//! Announcement HTTP wrappers; announcements are per-channel, so listing requires a channel id.

use shared::dto::announcement::{
    AnnouncementDto, EditAnnouncementRequest, PostAnnouncementRequest,
};
use shared::dto::ids::{ChannelId, MessageId};

use crate::api::client;
use crate::api::error::FrontendError;

/// Announcements posted to a channel (`GET /announcements?channel=…`).
pub async fn list(channel: ChannelId) -> Result<Vec<AnnouncementDto>, FrontendError> {
    let cid = channel.0.to_string();
    let q = client::query(&[("channel", &cid)]);
    client::get_json(&format!("/announcements{q}")).await
}

pub async fn post(req: &PostAnnouncementRequest) -> Result<AnnouncementDto, FrontendError> {
    client::post_json("/announcements", req).await
}

pub async fn edit(
    channel: ChannelId,
    announcement: MessageId,
    req: &EditAnnouncementRequest,
) -> Result<AnnouncementDto, FrontendError> {
    client::patch_json(
        &format!("/announcements/{}/{}", channel.0, announcement.0),
        req,
    )
    .await
}

pub async fn delete(channel: ChannelId, announcement: MessageId) -> Result<(), FrontendError> {
    client::del(&format!("/announcements/{}/{}", channel.0, announcement.0)).await
}
