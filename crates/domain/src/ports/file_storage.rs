use std::time::Duration;

use async_trait::async_trait;
use time::OffsetDateTime;

use crate::error::StorageError;

/// Metadata for one stored object, returned by [`FileStorage::list`].
#[derive(Debug, Clone)]
pub struct StorageObject {
    pub key: String,
    pub modified_at: OffsetDateTime,
    pub size: u64,
}

/// Local-filesystem (or MinIO-shaped) blob store. Keys are caller-chosen
/// (the schema's `storage_key` columns).
#[async_trait]
pub trait FileStorage: Send + Sync {
    async fn put(&self, key: &str, content_type: &str, bytes: Vec<u8>) -> Result<(), StorageError>;

    async fn get(&self, key: &str) -> Result<Vec<u8>, StorageError>;

    async fn delete(&self, key: &str) -> Result<(), StorageError>;

    async fn presign_get(&self, key: &str, ttl: Duration) -> Result<String, StorageError>;

    /// Lists stored objects under the `prefix` (empty = the whole store) with
    /// their last-modified time and size. Used by the upload orphan-sweep job.
    async fn list(&self, prefix: &str) -> Result<Vec<StorageObject>, StorageError>;
}
