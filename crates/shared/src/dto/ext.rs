//! Wire types for the external read-only API (`/api/ext/v1`). Flat records
//! with raw ids instead of denormalized summaries, so scripts parse
//! self-contained rows without follow-up fetches.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::dto::{
    ids::{GroupId, ProjectId, RequestId, UserId},
    project::ProjectStatus,
    request::{RequestPriority, RequestStatus},
};

/// One keyset page. `next_cursor` is present only when the page came back
/// full; pass it back as `after` to fetch the next page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageDto<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtProjectDto {
    pub id: ProjectId,
    pub owner_group_id: GroupId,
    pub created_by_user_id: UserId,
    pub name: String,
    pub description: String,
    pub status: ProjectStatus,
    pub progress: u8,
    #[serde(with = "time::serde::rfc3339::option")]
    pub completed_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtRequestDto {
    pub id: RequestId,
    pub project_id: ProjectId,
    pub creator_user_id: UserId,
    pub assignee_user_id: Option<UserId>,
    pub title: String,
    pub description: String,
    pub status: RequestStatus,
    pub priority: RequestPriority,
    pub progress: u8,
    #[serde(with = "time::serde::rfc3339::option")]
    pub due_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub completed_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}
