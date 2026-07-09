use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::dto::ids::{ServiceAccountId, UserId};

/// Read scope grantable to a service account on the external API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceAccountScope {
    Projects,
    Requests,
    Reports,
}

/// Mirrors `domain::model::ServiceAccountStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceAccountStatus {
    Active,
    Revoked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceAccountDto {
    pub id: ServiceAccountId,
    pub name: String,
    pub status: ServiceAccountStatus,
    pub created_by: UserId,
    #[serde(with = "time::serde::rfc3339::option")]
    pub revoked_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateServiceAccountRequest {
    pub name: String,
    pub scopes: Vec<ServiceAccountScope>,
}

/// Returned once from create; the key is never retrievable again.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatedServiceAccountDto {
    pub account: ServiceAccountDto,
    pub key: String,
}
