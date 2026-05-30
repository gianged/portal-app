use async_trait::async_trait;

use crate::{error::AuthzError, ids::UserId};

/// A single `ReBAC` relationship tuple: `subject` holds `relation` on `object`.
///
/// `subject` is a fully-qualified `ReBAC` id and may be a user (`user:<id>`), a
/// userset (`group:<id>#member`), a type-bound wildcard (`user:*`), or another
/// object (`company:portal`). Formatting is the caller's responsibility — see
/// `application::permissions` — so `domain` does not accumulate `ReBAC`
/// vocabulary.
#[derive(Debug, Clone)]
pub struct RelationTuple {
    pub subject: String,
    pub relation: String,
    pub object: String,
}

impl RelationTuple {
    #[must_use]
    pub fn new(
        subject: impl Into<String>,
        relation: impl Into<String>,
        object: impl Into<String>,
    ) -> Self {
        Self {
            subject: subject.into(),
            relation: relation.into(),
            object: object.into(),
        }
    }
}

/// `OpenFGA`-style `ReBAC` port. Object, relation, and subject strings cross the
/// boundary as `&str`; typed wrappers (`Object::project(id)`, `Relation::Viewer`)
/// belong in `application` so `domain` does not accumulate `ReBAC` vocabulary.
#[async_trait]
pub trait AuthzClient: Send + Sync {
    async fn check(&self, user: UserId, relation: &str, object: &str) -> Result<bool, AuthzError>;

    /// Write a single tuple. `subject` is a fully-qualified id (`user:<id>`,
    /// `group:<id>`, `company:portal`, `user:*`, …) — not just a user.
    async fn write_tuple(
        &self,
        subject: &str,
        relation: &str,
        object: &str,
    ) -> Result<(), AuthzError>;

    async fn delete_tuple(
        &self,
        subject: &str,
        relation: &str,
        object: &str,
    ) -> Result<(), AuthzError>;

    /// Atomically apply several writes and/or deletes in one backend call, so a
    /// multi-tuple resource grant (e.g. a project's `owner_group` + `company`)
    /// cannot land half-written. An empty `writes` and empty `deletes` is a no-op.
    async fn write_tuples(
        &self,
        writes: &[RelationTuple],
        deletes: &[RelationTuple],
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
