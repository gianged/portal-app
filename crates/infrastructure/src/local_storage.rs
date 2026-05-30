use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use tokio::fs;

use domain::{error::StorageError, ports::file_storage::FileStorage};

/// Local-filesystem implementation of [`FileStorage`]. Objects are written
/// under `root` (configured as `STORAGE_ROOT`). Keys are validated to stay
/// within the root so a crafted key like `../../etc/passwd` cannot escape it.
pub struct LocalStorage {
    root: PathBuf,
    public_base: String,
}

impl LocalStorage {
    #[must_use]
    pub fn new(root: PathBuf, public_base: &str) -> Self {
        Self {
            root,
            // Trailing slash normalised away so presign URLs join cleanly.
            public_base: public_base.trim_end_matches('/').to_string(),
        }
    }

    /// Resolves a storage key to an absolute path under `root`, rejecting any
    /// key that would escape it (absolute paths, `..`, Windows drive prefixes).
    fn resolve(&self, key: &str) -> Result<PathBuf, StorageError> {
        let mut safe = PathBuf::new();
        for comp in Path::new(key).components() {
            match comp {
                Component::Normal(c) => safe.push(c),
                _ => return Err(StorageError::Backend(format!("invalid storage key: {key}"))),
            }
        }
        if safe.as_os_str().is_empty() {
            return Err(StorageError::Backend(format!("empty storage key: {key}")));
        }
        Ok(self.root.join(safe))
    }
}

#[async_trait]
impl FileStorage for LocalStorage {
    async fn put(
        &self,
        key: &str,
        _content_type: &str,
        bytes: Vec<u8>,
    ) -> Result<(), StorageError> {
        // `content_type` is not persisted here; attachment MIME lives in Postgres.
        let path = self.resolve(key)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| StorageError::Backend(e.to_string()))?;
        }
        fs::write(&path, bytes)
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>, StorageError> {
        let path = self.resolve(key)?;
        fs::read(&path).await.map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => StorageError::NotFound,
            _ => StorageError::Backend(e.to_string()),
        })
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let path = self.resolve(key)?;
        match fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            // Idempotent: deleting a missing object is a no-op.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(StorageError::Backend(e.to_string())),
        }
    }

    async fn presign_get(&self, key: &str, _ttl: Duration) -> Result<String, StorageError> {
        // Local storage has no real presigning; return a direct URL routed
        // through the (future) file-serving endpoint. The key is still validated
        // so the no-escape contract matches the other methods. TODO: sign with
        // an expiry once the file route exists; `_ttl` is accepted but unenforced.
        self.resolve(key)?;
        Ok(format!("{}/files/{key}", self.public_base))
    }
}
