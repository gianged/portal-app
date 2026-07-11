use std::sync::{Arc, LazyLock};

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use domain::{
    ids::UserId,
    model::{ChannelKind, GroupRole, RequestStatus, User, UserStatus},
    ports::token_revocation::TokenRevocation,
    repository::{ChatRepository, GroupRepository, RequestRepository, UserRepository},
};
use time::OffsetDateTime;
use tokio::task;
use uuid::Uuid;

use crate::{
    commands::user::{CreateUserCommand, UpdateProfileCommand},
    error::{ConflictCode, Error, Result},
    events::{DomainEvent, EventBus},
    permissions::Permissions,
};

const OPEN_REQUEST_STATUSES: &[RequestStatus] = &[
    RequestStatus::Submitted,
    RequestStatus::Assigned,
    RequestStatus::InProgress,
    RequestStatus::Review,
];

pub struct UserService {
    users: Arc<dyn UserRepository>,
    groups: Arc<dyn GroupRepository>,
    requests: Arc<dyn RequestRepository>,
    chats: Arc<dyn ChatRepository>,
    perms: Arc<Permissions>,
    events: Arc<EventBus>,
    revocation: Arc<dyn TokenRevocation>,
}

impl UserService {
    #[must_use]
    pub fn new(
        users: Arc<dyn UserRepository>,
        groups: Arc<dyn GroupRepository>,
        requests: Arc<dyn RequestRepository>,
        chats: Arc<dyn ChatRepository>,
        perms: Arc<Permissions>,
        events: Arc<EventBus>,
        revocation: Arc<dyn TokenRevocation>,
    ) -> Self {
        Self {
            users,
            groups,
            requests,
            chats,
            perms,
            events,
            revocation,
        }
    }

    /// Ensure the user sees the company-wide general channel. No-op if the
    /// general channel hasn't been bootstrapped yet.
    async fn subscribe_to_general(&self, user_id: UserId) -> Result<()> {
        if let Some(channel) = self.chats.find_general_channel().await? {
            self.chats
                .subscribe_member(user_id, channel.id(), ChannelKind::General)
                .await?;
        }
        Ok(())
    }

    async fn unsubscribe_from_general(&self, user_id: UserId) -> Result<()> {
        if let Some(channel) = self.chats.find_general_channel().await? {
            self.chats.unsubscribe_member(user_id, channel.id()).await?;
        }
        Ok(())
    }

    /// Creates a new `Pending` user.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not HR, `Conflict` if the email is
    /// already in use, `Internal` if password hashing fails, a repository error
    /// if the datastore or authz backend is unavailable, or an event error if
    /// the event bus fails.
    #[tracing::instrument(skip_all, fields(actor = ?actor))]
    pub async fn create_user(&self, actor: UserId, cmd: CreateUserCommand) -> Result<User> {
        self.perms.require_hr(actor).await?;
        let now = OffsetDateTime::now_utc();

        if self.users.find_by_email(&cmd.email).await?.is_some() {
            return Err(Error::Conflict(ConflictCode::EmailAlreadyInUse));
        }

        let password_hash = hash_password(cmd.password).await?;
        let user = User {
            id: UserId(Uuid::now_v7()),
            email: cmd.email,
            password_hash,
            full_name: cmd.full_name,
            avatar_storage_key: None,
            phone: cmd.phone,
            timezone: cmd.timezone,
            status: UserStatus::Pending,
            system_role: cmd.system_role,
            email_notifications: true,
            first_logged_in_at: None,
            deactivated_at: None,
            created_at: now,
            updated_at: now,
        };

        self.users.save(&user).await?;
        // Mirror an org role (HR/Director) into company tuples so the model's
        // `... or director from company` viewer branches resolve.
        if let Some(role) = user.system_role {
            self.perms.grant_company_role(user.id, role).await?;
        }
        self.events
            .emit(DomainEvent::UserCreated {
                user_id: user.id,
                actor,
                at: now,
            })
            .await?;
        Ok(user)
    }

    /// Promotes a `Pending` user to `Active` on their first successful login.
    /// Caller authentication is enforced upstream (in the login route); this
    /// service trusts the `user_id` it receives.
    ///
    /// # Errors
    /// Returns `NotFound` if the user does not exist, `Transition` if the user is
    /// not in a state that can be activated, a repository error if the datastore
    /// is unavailable, or an event error if the event bus fails.
    #[tracing::instrument(skip_all, fields(user_id = ?user_id))]
    pub async fn complete_first_login(&self, user_id: UserId) -> Result<User> {
        let now = OffsetDateTime::now_utc();
        let mut user = self
            .users
            .find_by_id(user_id)
            .await?
            .ok_or(Error::NotFound("user"))?;
        user.activate(now, now)?;
        self.users.save(&user).await?;
        self.subscribe_to_general(user.id).await?;
        self.events
            .emit(DomainEvent::UserActivated {
                user_id: user.id,
                at: now,
            })
            .await?;
        Ok(user)
    }

    /// Authenticates a login attempt and returns the resolved active user.
    ///
    /// Returns `Ok(None)` for every failure mode (unknown email, wrong password, or
    /// a deactivated account) so callers cannot enumerate accounts. A `Pending` user
    /// who supplies the right password is promoted to `Active` on this first login
    /// (via [`Self::complete_first_login`]) and returned activated.
    ///
    /// # Errors
    /// Returns a repository error if the datastore is unavailable, `Internal` if
    /// the stored password hash cannot be parsed or verified, and propagates the
    /// errors of first-login activation when a `Pending` user is promoted.
    /// Unknown email, wrong password, and deactivated accounts yield `Ok(None)`,
    /// not an error.
    #[tracing::instrument(skip_all)]
    pub async fn login(&self, email: &str, password: &str) -> Result<Option<User>> {
        let Some(user) = self.users.find_by_email(email).await? else {
            decoy_verify(password).await;
            return Ok(None);
        };
        // Deactivated accounts cannot authenticate, even with a valid password.
        if user.status == UserStatus::Deactivated {
            decoy_verify(password).await;
            return Ok(None);
        }
        if !verify_password(user.password_hash.clone(), password).await? {
            return Ok(None);
        }
        if user.status == UserStatus::Pending {
            return self.complete_first_login(user.id).await.map(Some);
        }
        Ok(Some(user))
    }

    /// Deactivates a user, dropping their memberships and org-role tuples.
    /// Retry-safe: an already deactivated target re-runs the remaining cleanup.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not HR, `NotFound` if the target does
    /// not exist, `Conflict` if the target still leads a group or has open
    /// requests assigned, `Transition` if the target is still `Pending`, a
    /// repository error if the datastore or authz backend is unavailable, or an
    /// event error if the event bus fails.
    #[tracing::instrument(skip_all, fields(actor = ?actor, target = ?target))]
    pub async fn deactivate_user(&self, actor: UserId, target: UserId) -> Result<()> {
        self.perms.require_hr(actor).await?;
        let now = OffsetDateTime::now_utc();

        let mut user = self
            .users
            .find_by_id(target)
            .await?
            .ok_or(Error::NotFound("user"))?;

        let memberships = self.groups.list_active_memberships_for_user(target).await?;
        if memberships.iter().any(|m| m.role == GroupRole::Leader) {
            return Err(Error::Conflict(ConflictCode::TransferLeadershipFirst));
        }
        for status in OPEN_REQUEST_STATUSES {
            let open = self
                .requests
                .list_for_assignee(target, Some(*status), None)
                .await?;
            if !open.is_empty() {
                return Err(Error::Conflict(ConflictCode::ReassignOpenRequests));
            }
        }

        // An already-deactivated target skips the transition so a partially
        // failed run can resume; every cleanup step below is idempotent.
        if user.status != UserStatus::Deactivated {
            user.deactivate(now)?;
            self.users.save(&user).await?;
        }
        // Invalidate every session token the user still holds; status checks
        // alone leave read endpoints open until token expiry.
        self.revocation.bump_version(target).await?;
        for mut membership in memberships {
            membership.deactivate(now);
            self.groups.save_membership(&membership).await?;
            self.perms.revoke_group_membership(&membership).await?;
        }
        // Drop org-role tuples too; a deactivated HR/Director loses access until
        // reactivated.
        if let Some(role) = user.system_role {
            self.perms.revoke_company_role(target, role).await?;
        }
        self.unsubscribe_from_general(target).await?;
        self.events
            .emit(DomainEvent::UserDeactivated {
                user_id: user.id,
                actor,
                at: now,
            })
            .await?;
        Ok(())
    }

    /// Reactivates a deactivated user, restoring org-role tuples.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not HR, `NotFound` if the target does
    /// not exist, `Transition` if the user cannot be reactivated from its current
    /// state, a repository error if the datastore or authz backend is
    /// unavailable, or an event error if the event bus fails.
    #[tracing::instrument(skip_all, fields(actor = ?actor, target = ?target))]
    pub async fn reactivate_user(&self, actor: UserId, target: UserId) -> Result<User> {
        self.perms.require_hr(actor).await?;
        let now = OffsetDateTime::now_utc();

        let mut user = self
            .users
            .find_by_id(target)
            .await?
            .ok_or(Error::NotFound("user"))?;
        user.reactivate(now)?;
        self.users.save(&user).await?;
        if let Some(role) = user.system_role {
            self.perms.grant_company_role(target, role).await?;
        }
        self.subscribe_to_general(target).await?;
        self.events
            .emit(DomainEvent::UserReactivated {
                user_id: user.id,
                actor,
                at: now,
            })
            .await?;
        Ok(user)
    }

    /// Updates a user's profile. A user may update their own profile; otherwise
    /// the actor must be HR.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active (when editing their own
    /// profile) or not HR (when editing another user), `NotFound` if the target
    /// does not exist, a repository error if the datastore is unavailable, or an
    /// event error if the event bus fails.
    #[tracing::instrument(skip_all, fields(actor = ?actor, target = ?target))]
    pub async fn update_profile(
        &self,
        actor: UserId,
        target: UserId,
        cmd: UpdateProfileCommand,
    ) -> Result<User> {
        if actor == target {
            self.perms.require_active(actor).await?;
        } else {
            self.perms.require_hr(actor).await?;
        }
        let now = OffsetDateTime::now_utc();

        let mut user = self
            .users
            .find_by_id(target)
            .await?
            .ok_or(Error::NotFound("user"))?;

        if let Some(full_name) = cmd.full_name {
            user.full_name = full_name;
        }
        if cmd.phone.is_some() {
            user.phone = cmd.phone;
        }
        if let Some(timezone) = cmd.timezone {
            user.timezone = timezone;
        }
        if let Some(key) = cmd.avatar_storage_key {
            // A profile key is client-supplied; a prefix pin keeps it from
            // pointing at another user's uploads or arbitrary stored objects.
            if !key.starts_with(&format!("avatars/{}/", target.0)) {
                return Err(Error::Validation("invalid avatar storage key".into()));
            }
            user.avatar_storage_key = Some(key);
        }
        if let Some(email_notifications) = cmd.email_notifications {
            user.email_notifications = email_notifications;
        }
        user.updated_at = now;

        self.users.save(&user).await?;
        self.events
            .emit(DomainEvent::UserProfileUpdated {
                user_id: user.id,
                actor,
                at: now,
            })
            .await?;
        Ok(user)
    }

    /// Changes the actor's own password after re-verifying the current one,
    /// then revokes every session token they hold.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active, `Validation` if the
    /// current password is wrong, `NotFound` if the actor row is missing,
    /// `Internal` if password hashing or verification fails, or a
    /// repository/event error from the backends.
    #[tracing::instrument(skip_all, fields(actor = ?actor))]
    pub async fn change_password(
        &self,
        actor: UserId,
        current_password: &str,
        new_password: String,
    ) -> Result<()> {
        self.perms.require_active(actor).await?;
        let now = OffsetDateTime::now_utc();

        let mut user = self
            .users
            .find_by_id(actor)
            .await?
            .ok_or(Error::NotFound("user"))?;
        if !verify_password(user.password_hash.clone(), current_password).await? {
            return Err(Error::Validation("current password is incorrect".into()));
        }

        user.password_hash = hash_password(new_password).await?;
        user.updated_at = now;
        self.users.save(&user).await?;
        self.revocation.bump_version(actor).await?;
        self.events
            .emit(DomainEvent::UserPasswordChanged {
                user_id: actor,
                at: now,
            })
            .await?;
        Ok(())
    }

    /// HR sets a temporary password for another user (e.g. a forgotten
    /// password), revoking all of the target's sessions.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not HR, `NotFound` if the target
    /// does not exist, `Internal` if password hashing fails, or a
    /// repository/event error from the backends.
    #[tracing::instrument(skip_all, fields(actor = ?actor, target = ?target))]
    pub async fn admin_reset_password(
        &self,
        actor: UserId,
        target: UserId,
        new_password: String,
    ) -> Result<()> {
        self.perms.require_hr(actor).await?;
        let now = OffsetDateTime::now_utc();

        let mut user = self
            .users
            .find_by_id(target)
            .await?
            .ok_or(Error::NotFound("user"))?;
        user.password_hash = hash_password(new_password).await?;
        user.updated_at = now;
        self.users.save(&user).await?;
        self.revocation.bump_version(target).await?;
        self.events
            .emit(DomainEvent::UserPasswordReset {
                user_id: target,
                actor,
                at: now,
            })
            .await?;
        Ok(())
    }

    /// Looks up a user by id, returning `None` if it does not exist.
    ///
    /// # Errors
    /// Returns a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(id = ?id))]
    pub async fn find(&self, id: UserId) -> Result<Option<User>> {
        Ok(self.users.find_by_id(id).await?)
    }

    /// Users for a batch of ids, any status; missing ids are simply absent.
    ///
    /// # Errors
    /// Returns a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(count = ids.len()))]
    pub async fn find_by_ids(&self, ids: &[UserId]) -> Result<Vec<User>> {
        Ok(self.users.find_by_ids(ids).await?)
    }

    /// Lists active users with pagination; `q` filters by name/email substring.
    ///
    /// # Errors
    /// Returns a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(limit = ?limit, offset = ?offset))]
    pub async fn list_active(&self, limit: u32, offset: u32, q: Option<&str>) -> Result<Vec<User>> {
        Ok(self.users.list_active(limit, offset, q).await?)
    }
}

async fn hash_password(password: impl Into<String>) -> Result<String> {
    // Argon2 is CPU-heavy (tens of ms); run it on the blocking pool so it never
    // stalls a tokio worker thread.
    let password = password.into();
    task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| Error::Internal(format!("password hash failed: {e}")))?
            .to_string();
        Ok(hash)
    })
    .await
    .map_err(|e| Error::Internal(format!("password hash task failed: {e}")))?
}

/// Burns an argon2 verification against a fixed decoy hash so the unknown-email
/// and deactivated-account paths cost the same as a real check (no timing oracle
/// for account enumeration). The outcome is discarded by design.
async fn decoy_verify(password: impl Into<String>) {
    static DECOY_HASH: LazyLock<String> = LazyLock::new(|| {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(b"decoy password", &salt)
            .expect("hashing a fixed decoy password never fails")
            .to_string()
    });
    let password = password.into();
    let _ = task::spawn_blocking(move || {
        let Ok(parsed) = PasswordHash::new(&DECOY_HASH) else {
            return;
        };
        let _ = Argon2::default().verify_password(password.as_bytes(), &parsed);
    })
    .await;
}

/// Verifies a candidate password against a stored PHC hash, off the async
/// runtime (argon2 verification costs the same as hashing).
async fn verify_password(hash: impl Into<String>, password: impl Into<String>) -> Result<bool> {
    let hash = hash.into();
    let password = password.into();
    task::spawn_blocking(move || {
        let parsed = PasswordHash::new(&hash)
            .map_err(|e| Error::Internal(format!("invalid stored password hash: {e}")))?;
        match Argon2::default().verify_password(password.as_bytes(), &parsed) {
            Ok(()) => Ok(true),
            // A mismatch is the expected "wrong password" path, not an error.
            Err(argon2::password_hash::Error::Password) => Ok(false),
            Err(e) => Err(Error::Internal(format!("password verify failed: {e}"))),
        }
    })
    .await
    .map_err(|e| Error::Internal(format!("password verify task failed: {e}")))?
}
