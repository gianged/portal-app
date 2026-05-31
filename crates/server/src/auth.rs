//! Session tokens (HS256 JWT) and the cookie they ride in.
//!
//! The token carries identity only — `sub` (user id) plus `iat`/`exp`. Roles and
//! active-status are deliberately NOT encoded: `application::Permissions`
//! re-resolves them from Postgres + `OpenFGA` on every call, so a deactivated user
//! loses access immediately instead of at token expiry.

use axum_extra::extract::cookie::{Cookie, SameSite};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use domain::ids::UserId;

use crate::error::AuthError;

/// Name of the cookie carrying the session JWT.
pub const SESSION_COOKIE: &str = "portal_session";

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    /// Subject: the authenticated user's id.
    sub: Uuid,
    /// Issued-at, seconds since the Unix epoch.
    iat: i64,
    /// Expiry, seconds since the Unix epoch. Validated by `jsonwebtoken`.
    exp: i64,
}

/// Mints and verifies session tokens and builds the cookie they travel in.
/// Holds the symmetric key, so it lives behind an `Arc` in `AppState`.
pub struct TokenService {
    encoding: EncodingKey,
    decoding: DecodingKey,
    validation: Validation,
    ttl: Duration,
    secure: bool,
}

impl TokenService {
    #[must_use]
    pub fn new(secret: &str, ttl_secs: u64, secure: bool) -> Self {
        Self {
            encoding: EncodingKey::from_secret(secret.as_bytes()),
            decoding: DecodingKey::from_secret(secret.as_bytes()),
            validation: Validation::new(Algorithm::HS256),
            // `ttl_secs` is config-derived (hours * 3600); it fits i64 comfortably.
            ttl: Duration::seconds(i64::try_from(ttl_secs).unwrap_or(i64::MAX)),
            secure,
        }
    }

    /// Mints a signed token for `user_id`, valid for the configured TTL.
    #[must_use]
    pub fn issue(&self, user_id: UserId) -> String {
        let now = OffsetDateTime::now_utc();
        let claims = Claims {
            sub: user_id.0,
            iat: now.unix_timestamp(),
            exp: (now + self.ttl).unix_timestamp(),
        };
        // HS256 signing of an in-memory struct with a valid key is infallible; an
        // error here would be a configuration bug at startup, not a runtime path.
        encode(&Header::new(Algorithm::HS256), &claims, &self.encoding)
            .expect("HS256 encoding of session claims is infallible")
    }

    /// Verifies signature + expiry and returns the token's subject.
    pub fn verify(&self, token: &str) -> Result<UserId, AuthError> {
        let data = decode::<Claims>(token, &self.decoding, &self.validation)
            .map_err(|_| AuthError::Invalid)?;
        Ok(UserId(data.claims.sub))
    }

    /// `Set-Cookie` for a fresh session.
    #[must_use]
    pub fn session_cookie(&self, token: String) -> Cookie<'static> {
        self.cookie(token, self.ttl)
    }

    /// `Set-Cookie` that clears the session (logout).
    #[must_use]
    pub fn clear_cookie(&self) -> Cookie<'static> {
        self.cookie(String::new(), Duration::ZERO)
    }

    fn cookie(&self, value: String, max_age: Duration) -> Cookie<'static> {
        Cookie::build((SESSION_COOKIE, value))
            .http_only(true)
            .secure(self.secure)
            .same_site(SameSite::Lax)
            .path("/")
            .max_age(max_age)
            .build()
    }
}
