use std::{fmt::Write, sync::Arc};

use password_hash::rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use uuid::Uuid;

use domain::{
    ids::{ServiceAccountId, UserId},
    model::{ServiceAccount, ServiceAccountStatus},
    repository::ServiceAccountRepository,
};

use crate::{
    error::{Error, Result},
    permissions::{Permissions, ServiceAccountScope},
};

/// Prefix marking external API keys, so leaked strings are recognisable.
const KEY_PREFIX: &str = "pak_";
const KEY_BYTES: usize = 32;

/// A freshly created account plus the plaintext key. The key exists only in
/// this value: it is returned to the admin once and never stored.
pub struct CreatedServiceAccount {
    pub account: ServiceAccount,
    pub key: String,
}

/// Admin-gated lifecycle of external API service accounts: create (generates
/// the key, stores its hash, writes the scope tuples), list, revoke. Key
/// authentication for the ext middleware lives here too so the hashing scheme
/// has exactly one home.
pub struct ServiceAccountService {
    accounts: Arc<dyn ServiceAccountRepository>,
    perms: Arc<Permissions>,
}

impl ServiceAccountService {
    #[must_use]
    pub fn new(accounts: Arc<dyn ServiceAccountRepository>, perms: Arc<Permissions>) -> Self {
        Self { accounts, perms }
    }

    /// Creates an account with the given read scopes and returns the plaintext
    /// key exactly once.
    ///
    /// # Errors
    /// Returns `Forbidden` unless the actor is Director/HR, `Validation` for an
    /// empty name or scope list, `Conflict` on a duplicate name, or a
    /// repository / authz error.
    #[tracing::instrument(skip_all, fields(actor = ?actor))]
    pub async fn create(
        &self,
        actor: UserId,
        name: &str,
        scopes: &[ServiceAccountScope],
    ) -> Result<CreatedServiceAccount> {
        self.perms.require_admin(actor).await?;
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Validation("name must not be empty".into()));
        }
        if scopes.is_empty() {
            return Err(Error::Validation("at least one scope is required".into()));
        }

        let mut secret = [0u8; KEY_BYTES];
        OsRng.fill_bytes(&mut secret);
        let key = format!("{KEY_PREFIX}{}", hex(&secret));

        let now = OffsetDateTime::now_utc();
        let account = ServiceAccount {
            id: ServiceAccountId(Uuid::now_v7()),
            name: name.to_owned(),
            key_hash: hash_key(&key),
            status: ServiceAccountStatus::Active,
            created_by: actor,
            revoked_at: None,
            created_at: now,
            updated_at: now,
        };
        self.accounts.create(&account).await?;
        if let Err(e) = self
            .perms
            .grant_service_account_scopes(account.id, scopes)
            .await
        {
            // Compensate best-effort: a failed grant must not leave an active
            // zero-scope account behind.
            let mut orphan = account;
            if orphan.revoke(OffsetDateTime::now_utc()).is_ok()
                && let Err(save_err) = self.accounts.save(&orphan).await
            {
                tracing::warn!(error = %save_err, "orphan service account cleanup failed");
            }
            return Err(e);
        }
        Ok(CreatedServiceAccount { account, key })
    }

    /// Every account, newest first.
    ///
    /// # Errors
    /// Returns `Forbidden` unless the actor is Director/HR, or a repository error.
    #[tracing::instrument(skip_all, fields(actor = ?actor))]
    pub async fn list(&self, actor: UserId) -> Result<Vec<ServiceAccount>> {
        self.perms.require_admin(actor).await?;
        self.accounts.list().await.map_err(Into::into)
    }

    /// Revokes an account: its key stops authenticating immediately; the scope
    /// tuples are cleaned up best-effort.
    ///
    /// # Errors
    /// Returns `Forbidden` unless the actor is Director/HR, `NotFound` for an
    /// unknown id, `Transition` when already revoked, or a repository error.
    #[tracing::instrument(skip_all, fields(actor = ?actor, id = ?id))]
    pub async fn revoke(&self, actor: UserId, id: ServiceAccountId) -> Result<()> {
        self.perms.require_admin(actor).await?;
        let mut account = self
            .accounts
            .find_by_id(id)
            .await?
            .ok_or(Error::NotFound("service account"))?;
        account.revoke(OffsetDateTime::now_utc())?;
        self.accounts.save(&account).await?;
        self.perms.revoke_service_account_scopes(id).await;
        Ok(())
    }

    /// Resolves a presented `pak_*` key to its active account, or `None` when
    /// the key is malformed, unknown, or revoked.
    ///
    /// # Errors
    /// Returns a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all)]
    pub async fn authenticate(&self, presented_key: &str) -> Result<Option<ServiceAccount>> {
        if !presented_key.starts_with(KEY_PREFIX) {
            return Ok(None);
        }
        self.accounts
            .find_active_by_key_hash(&hash_key(presented_key))
            .await
            .map_err(Into::into)
    }
}

/// SHA-256 of the full presented key string. The key is 32 random bytes, so a
/// fast hash is sufficient; no salt or slow KDF needed.
fn hash_key(key: &str) -> Vec<u8> {
    Sha256::digest(key.as_bytes()).to_vec()
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        write!(out, "{b:02x}").expect("writing to a String cannot fail");
    }
    out
}
