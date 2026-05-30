use domain::{
    ids::ChannelId,
    model::{Channel, GeneralChannel},
    repository::ChatRepository,
};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{error::Result, permissions::Permissions};

/// Well-known id of the single company-wide general channel. Fixed (not a fresh
/// `Uuid::now_v7`) so the find-or-create below is idempotent even if two
/// composition roots boot concurrently — both converge on the same row instead
/// of racing to create two general channels.
const GENERAL_CHANNEL_ID: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_00a1);

/// Idempotent org bootstrap, run by each composition root at startup. Wires the
/// pieces the `OpenFGA` model and the chat list assume already exist:
///
/// - the `company#member` wildcard, so every user resolves the general channel's
///   `viewer` (`member from company`);
/// - the single general channel row (find-or-create);
/// - the general channel's `company` tuple, so its viewer resolves.
///
/// Safe to call on every boot: each step is a no-op when already applied.
pub async fn seed_company(chats: &dyn ChatRepository, perms: &Permissions) -> Result<()> {
    perms.seed_company_member_wildcard().await?;

    let channel = if let Some(channel) = chats.find_general_channel().await? {
        channel
    } else {
        let channel = Channel::General(GeneralChannel {
            id: ChannelId(GENERAL_CHANNEL_ID),
            created_at: OffsetDateTime::now_utc(),
        });
        chats.save_channel(&channel).await?;
        channel
    };
    perms.grant_general_channel_company(channel.id()).await?;
    Ok(())
}
