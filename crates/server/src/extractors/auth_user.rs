//! The `AuthUser` extractor: the only way a handler obtains the caller's id.
//!
//! `middleware::auth::require_auth` verifies the session cookie and inserts an
//! `AuthUser` into the request extensions; this extractor reads it back out.
//! Roles are NOT carried here — services resolve them through `Permissions`.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;

use domain::ids::UserId;

use crate::error::{AppError, AuthError};

#[derive(Debug, Clone, Copy)]
pub struct AuthUser {
    pub user_id: UserId,
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthUser>()
            .copied()
            .ok_or(AppError::Auth(AuthError::Missing))
    }
}
