use async_trait::async_trait;

use crate::{error::AuthzError, ids::UserId};

/// `OpenFGA`-style `ReBAC` port. Object and relation strings cross the boundary
/// as `&str`; typed wrappers (`Object::project(id)`, `Relation::CanAssign`) belong
/// in `application` so `domain` does not accumulate `ReBAC` vocabulary.
#[async_trait]
pub trait AuthzClient: Send + Sync {
    async fn check(&self, user: UserId, relation: &str, object: &str) -> Result<bool, AuthzError>;

    async fn write_tuple(
        &self,
        user: UserId,
        relation: &str,
        object: &str,
    ) -> Result<(), AuthzError>;

    async fn delete_tuple(
        &self,
        user: UserId,
        relation: &str,
        object: &str,
    ) -> Result<(), AuthzError>;

    /// Returns object IDs (e.g. `"project:abc-123"`) for which `user` holds
    /// `relation`.
    async fn list_objects(
        &self,
        user: UserId,
        relation: &str,
        object_type: &str,
    ) -> Result<Vec<String>, AuthzError>;
}
