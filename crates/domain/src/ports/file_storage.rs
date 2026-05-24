use std::time::Duration;

use async_trait::async_trait;

use crate::error::StorageError;

/// Local-filesystem (or MinIO-shaped) blob store. Keys are caller-chosen
/// (the schema's `storage_key` columns).
#[async_trait]
pub trait FileStorage: Send + Sync {
    async fn put(
        &self,
        key: &str,
        content_type: &str,
        bytes: Vec<u8>,
    ) -> Result<(), StorageError>;

    async fn get(&self, key: &str) -> Result<Vec<u8>, StorageError>;

    async fn delete(&self, key: &str) -> Result<(), StorageError>;

    async fn presign_get(&self, key: &str, ttl: Duration) -> Result<String, StorageError>;
}
