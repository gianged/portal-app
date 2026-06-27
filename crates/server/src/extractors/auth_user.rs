//! The `AuthUser` extractor: the only way a handler obtains the caller's id.
//!
//! `middleware::auth::require_auth` verifies the session cookie and inserts an
//! `AuthUser` into the request extensions; this extractor reads it back out.
//! Roles are NOT carried here - services resolve them through `Permissions`.

use axum::{extract::FromRequestParts, http::request::Parts};

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

#[cfg(test)]
mod tests {
    use super::*;

    use axum::{http::{Request, StatusCode}, response::IntoResponse};

    fn parts_with(user: Option<AuthUser>) -> Parts {
        let mut request = Request::builder().body(()).expect("build request");
        if let Some(user) = user {
            request.extensions_mut().insert(user);
        }
        request.into_parts().0
    }

    #[tokio::test]
    async fn reads_auth_user_from_extensions() {
        let user = AuthUser {
            user_id: UserId(uuid::Uuid::nil()),
        };
        let mut parts = parts_with(Some(user));
        let extracted = AuthUser::from_request_parts(&mut parts, &())
            .await
            .expect("extract AuthUser");
        assert_eq!(extracted.user_id, user.user_id);
    }

    #[tokio::test]
    async fn missing_extension_rejects_as_401() {
        let mut parts = parts_with(None);
        let rejection = AuthUser::from_request_parts(&mut parts, &())
            .await
            .expect_err("missing AuthUser must reject");
        assert_eq!(rejection.into_response().status(), StatusCode::UNAUTHORIZED);
    }
}
