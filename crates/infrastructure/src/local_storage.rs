use std::{
    io::ErrorKind,
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use time::OffsetDateTime;
use tokio::fs;
use uuid::Uuid;

use domain::{
    error::StorageError,
    ids::UserId,
    ports::file_storage::{FileStorage, StorageObject},
};

use crate::signed_url::SignedUrl;

// RFC 3986 unreserved characters stay literal; everything else is encoded.
const PATH_SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

/// Local-filesystem implementation of [`FileStorage`]. Objects are written
/// under `root` (configured as `STORAGE_ROOT`). Keys are validated to stay
/// within the root so a crafted key like `../../etc/passwd` cannot escape it.
pub struct LocalStorage {
    root: PathBuf,
    public_base: String,
    signer: Arc<SignedUrl>,
}

impl LocalStorage {
    #[must_use]
    pub fn new(root: PathBuf, public_base: &str, signer: Arc<SignedUrl>) -> Self {
        Self {
            root,
            // Trailing slash normalised away so presign URLs join cleanly.
            public_base: public_base.trim_end_matches('/').to_string(),
            signer,
        }
    }

    /// Resolves a storage key to an absolute path under `root`, rejecting any
    /// key that would escape it (absolute paths, `..`, Windows drive prefixes).
    fn resolve(&self, key: &str) -> Result<PathBuf, StorageError> {
        let mut safe = PathBuf::new();
        for comp in Path::new(key).components() {
            match comp {
                Component::Normal(c) => safe.push(c),
                _ => return Err(StorageError::InvalidKey(key.to_owned())),
            }
        }
        if safe.as_os_str().is_empty() {
            return Err(StorageError::InvalidKey(key.to_owned()));
        }
        Ok(self.root.join(safe))
    }

    /// Inverse of `resolve`: maps an absolute path under `root` back to a
    /// forward-slash storage key.
    fn key_for(&self, path: &Path) -> Result<String, StorageError> {
        let rel = path
            .strip_prefix(&self.root)
            .map_err(|_| StorageError::Backend("path escaped storage root".into()))?;
        let key = rel
            .components()
            .filter_map(|c| match c {
                Component::Normal(s) => s.to_str(),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/");
        if key.is_empty() {
            return Err(StorageError::Backend("empty storage key".into()));
        }
        Ok(key)
    }
}

#[async_trait]
impl FileStorage for LocalStorage {
    #[tracing::instrument(skip_all)]
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
        // Write-then-rename so a crash or full disk never leaves a truncated
        // object visible at the final key.
        let tmp = path.with_extension(format!("tmp-{}", Uuid::now_v7().simple()));
        fs::write(&tmp, bytes)
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        if let Err(e) = fs::rename(&tmp, &path).await {
            let _ = fs::remove_file(&tmp).await;
            return Err(StorageError::Backend(e.to_string()));
        }
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn get(&self, key: &str) -> Result<Vec<u8>, StorageError> {
        let path = self.resolve(key)?;
        fs::read(&path).await.map_err(|e| match e.kind() {
            ErrorKind::NotFound => StorageError::NotFound,
            _ => StorageError::Backend(e.to_string()),
        })
    }

    #[tracing::instrument(skip_all)]
    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let path = self.resolve(key)?;
        match fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            // Idempotent: deleting a missing object is a no-op.
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(StorageError::Backend(e.to_string())),
        }
    }

    #[tracing::instrument(skip_all, fields(user = ?user))]
    async fn presign_get(
        &self,
        key: &str,
        ttl: Duration,
        user: UserId,
    ) -> Result<String, StorageError> {
        // Validate the key, then emit a signed URL bound to key, expiry, and viewer.
        self.resolve(key)?;
        let (exp, sig) = self
            .signer
            .sign_for(key, user, ttl, OffsetDateTime::now_utc());
        // Filenames may contain `?`, `#`, `%`, etc.; the signature covers the
        // raw key, which the route sees again after axum percent-decodes.
        let encoded = key
            .split('/')
            .map(|seg| utf8_percent_encode(seg, PATH_SEGMENT).to_string())
            .collect::<Vec<_>>()
            .join("/");
        Ok(format!(
            "{}/files/{encoded}?exp={exp}&sig={sig}",
            self.public_base
        ))
    }

    #[tracing::instrument(skip_all)]
    async fn list(&self, prefix: &str) -> Result<Vec<StorageObject>, StorageError> {
        let base = if prefix.is_empty() {
            self.root.clone()
        } else {
            self.resolve(prefix)?
        };
        let mut objects = Vec::new();
        let mut stack = vec![base];
        while let Some(dir) = stack.pop() {
            let mut entries = match fs::read_dir(&dir).await {
                Ok(entries) => entries,
                // A missing prefix directory simply yields no objects.
                Err(e) if e.kind() == ErrorKind::NotFound => continue,
                Err(e) => return Err(StorageError::Backend(e.to_string())),
            };
            while let Some(entry) = entries
                .next_entry()
                .await
                .map_err(|e| StorageError::Backend(e.to_string()))?
            {
                let meta = entry
                    .metadata()
                    .await
                    .map_err(|e| StorageError::Backend(e.to_string()))?;
                if meta.is_dir() {
                    stack.push(entry.path());
                } else if meta.is_file() {
                    let modified = meta
                        .modified()
                        .map_err(|e| StorageError::Backend(e.to_string()))?;
                    objects.push(StorageObject {
                        key: self.key_for(&entry.path())?,
                        modified_at: OffsetDateTime::from(modified),
                        size: meta.len(),
                    });
                }
            }
        }
        Ok(objects)
    }
}
