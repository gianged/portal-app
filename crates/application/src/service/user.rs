use std::sync::Arc;

use argon2::{
    Argon2, PasswordHasher,
    password_hash::{SaltString, rand_core::OsRng},
};
use domain::{
    error::RepositoryError,
    ids::UserId,
    model::{GroupRole, RequestStatus, User, UserStatus},
    repository::{GroupRepository, RequestRepository, UserRepository},
};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    commands::user::{CreateUserCommand, UpdateProfileCommand},
    error::{Error, Result},
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
    perms: Arc<Permissions>,
    events: Arc<EventBus>,
}

impl UserService {
    #[must_use]
    pub fn new(
        users: Arc<dyn UserRepository>,
        groups: Arc<dyn GroupRepository>,
        requests: Arc<dyn RequestRepository>,
        perms: Arc<Permissions>,
        events: Arc<EventBus>,
    ) -> Self {
        Self {
            users,
            groups,
            requests,
            perms,
            events,
        }
    }

    pub async fn create_user(&self, actor: UserId, cmd: CreateUserCommand) -> Result<User> {
        self.perms.require_hr(actor).await?;
        let now = OffsetDateTime::now_utc();

        if self.users.find_by_email(&cmd.email).await?.is_some() {
            return Err(Error::Conflict("email_already_in_use".into()));
        }

        let password_hash = hash_password(&cmd.password)?;
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
            first_logged_in_at: None,
            deactivated_at: None,
            created_at: now,
            updated_at: now,
        };

        self.users.save(&user).await?;
        self.events
            .emit(DomainEvent::UserCreated {
                user_id: user.id,
                actor,
                at: now,
                after: user.clone(),
            })
            .await?;
        Ok(user)
    }

    /// Promotes a `Pending` user to `Active` on their first successful login.
    /// Caller authentication is enforced upstream (in the login route) — this
    /// service trusts the `user_id` it receives.
    pub async fn complete_first_login(&self, user_id: UserId) -> Result<User> {
        let now = OffsetDateTime::now_utc();
        let mut user = self
            .users
            .find_by_id(user_id)
            .await?
            .ok_or(Error::NotFound("user"))?;
        user.activate(now, now)?;
        self.users.save(&user).await?;
        self.events
            .emit(DomainEvent::UserActivated {
                user_id: user.id,
                at: now,
                after: user.clone(),
            })
            .await?;
        Ok(user)
    }

    pub async fn deactivate_user(&self, actor: UserId, target: UserId) -> Result<()> {
        self.perms.require_hr(actor).await?;
        let now = OffsetDateTime::now_utc();

        let mut user = self
            .users
            .find_by_id(target)
            .await?
            .ok_or(Error::NotFound("user"))?;
        let before = user.clone();

        let memberships = self.groups.list_active_memberships_for_user(target).await?;
        if memberships.iter().any(|m| m.role == GroupRole::Leader) {
            return Err(Error::Conflict("transfer_leadership_first".into()));
        }
        for status in OPEN_REQUEST_STATUSES {
            let open = self
                .requests
                .list_for_assignee(target, Some(*status))
                .await?;
            if !open.is_empty() {
                return Err(Error::Conflict("reassign_open_requests".into()));
            }
        }

        user.deactivate(now)?;
        self.users.save(&user).await?;
        for mut membership in memberships {
            membership.deactivate(now);
            self.groups.save_membership(&membership).await?;
            self.perms.revoke_group_membership(&membership).await?;
        }
        self.events
            .emit(DomainEvent::UserDeactivated {
                user_id: user.id,
                actor,
                at: now,
                before,
                after: user,
            })
            .await?;
        Ok(())
    }

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
        self.events
            .emit(DomainEvent::UserReactivated {
                user_id: user.id,
                actor,
                at: now,
                after: user.clone(),
            })
            .await?;
        Ok(user)
    }

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
        let before = user.clone();

        if let Some(full_name) = cmd.full_name {
            user.full_name = full_name;
        }
        if cmd.phone.is_some() {
            user.phone = cmd.phone;
        }
        if let Some(timezone) = cmd.timezone {
            user.timezone = timezone;
        }
        if cmd.avatar_storage_key.is_some() {
            user.avatar_storage_key = cmd.avatar_storage_key;
        }
        user.updated_at = now;

        self.users.save(&user).await?;
        self.events
            .emit(DomainEvent::UserProfileUpdated {
                user_id: user.id,
                actor,
                at: now,
                before,
                after: user.clone(),
            })
            .await?;
        Ok(user)
    }

    pub async fn find(&self, id: UserId) -> Result<Option<User>> {
        Ok(self.users.find_by_id(id).await?)
    }

    pub async fn find_by_email(&self, email: &str) -> Result<Option<User>> {
        Ok(self.users.find_by_email(email).await?)
    }

    pub async fn list_active(&self, limit: u32, offset: u32) -> Result<Vec<User>> {
        Ok(self.users.list_active(limit, offset).await?)
    }
}

fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| {
            Error::Repository(RepositoryError::Backend(format!(
                "password hash failed: {e}"
            )))
        })?
        .to_string();
    Ok(hash)
}
