//! Session tokens (HS256 JWT) and the cookie they ride in.
//!
//! The token carries identity only (`sub` + `iat`/`exp`); roles and active-status
//! are re-resolved per call from Postgres + `OpenFGA`, so deactivation is immediate.

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
    /// Token id, so logout can denylist this one token server-side.
    jti: Uuid,
    /// User's token version at mint time; a version bump (deactivation,
    /// password change) invalidates every token carrying an older value.
    ver: u64,
}

/// Decoded, signature-checked token contents handed to the auth middleware,
/// which still has to consult [`TokenRevocation`] before trusting them.
///
/// [`TokenRevocation`]: domain::ports::token_revocation::TokenRevocation
#[derive(Debug, Clone, Copy)]
pub struct VerifiedToken {
    pub user_id: UserId,
    pub jti: Uuid,
    pub version: u64,
    /// Expiry, seconds since the Unix epoch; lets logout size the denylist
    /// entry to the token's remaining lifetime.
    pub exp: i64,
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
        let mut validation = Validation::new(Algorithm::HS256);
        // Strict expiry; jsonwebtoken otherwise grants 60s of clock-skew leeway.
        validation.leeway = 0;
        Self {
            encoding: EncodingKey::from_secret(secret.as_bytes()),
            decoding: DecodingKey::from_secret(secret.as_bytes()),
            validation,
            // `ttl_secs` is config-derived (hours * 3600); it fits i64 comfortably.
            ttl: Duration::seconds(i64::try_from(ttl_secs).unwrap_or(i64::MAX)),
            secure,
        }
    }

    /// Mints a signed token for `user_id` at token-version `ver`, valid for
    /// the configured TTL.
    #[must_use]
    pub fn issue(&self, user_id: UserId, ver: u64) -> String {
        self.issue_at(user_id, ver, OffsetDateTime::now_utc())
    }

    /// [`issue`](Self::issue) with the clock injected, so token expiry is
    /// exercisable in tests.
    fn issue_at(&self, user_id: UserId, ver: u64, now: OffsetDateTime) -> String {
        let claims = Claims {
            sub: user_id.0,
            iat: now.unix_timestamp(),
            exp: (now + self.ttl).unix_timestamp(),
            jti: Uuid::now_v7(),
            ver,
        };
        // HS256 signing with a valid key is infallible; an error here is a startup config bug.
        encode(&Header::new(Algorithm::HS256), &claims, &self.encoding)
            .expect("HS256 encoding of session claims is infallible")
    }

    /// Verifies signature + expiry and returns the decoded contents. Tokens
    /// minted before the jti/ver claims existed fail decode and read as invalid.
    pub fn verify(&self, token: &str) -> Result<VerifiedToken, AuthError> {
        let data = decode::<Claims>(token, &self.decoding, &self.validation)
            .map_err(|_| AuthError::Invalid)?;
        Ok(VerifiedToken {
            user_id: UserId(data.claims.sub),
            jti: data.claims.jti,
            version: data.claims.ver,
            exp: data.claims.exp,
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    const TTL_SECS: u64 = 3600;

    fn service(secret: &str) -> TokenService {
        TokenService::new(secret, TTL_SECS, false)
    }

    fn user() -> UserId {
        UserId(Uuid::from_u128(0x1234_5678_9abc_def0_1234_5678_9abc_def0))
    }

    #[test]
    fn issue_then_verify_round_trips_subject_and_version() {
        let svc = service("session-secret");
        let token = svc.issue(user(), 7);
        let verified = svc.verify(&token).expect("valid token");
        assert_eq!(verified.user_id, user());
        assert_eq!(verified.version, 7);
    }

    #[test]
    fn each_issued_token_gets_a_distinct_jti() {
        let svc = service("session-secret");
        let a = svc.verify(&svc.issue(user(), 0)).expect("valid token");
        let b = svc.verify(&svc.issue(user(), 0)).expect("valid token");
        assert_ne!(a.jti, b.jti);
    }

    #[test]
    fn verify_rejects_a_non_jwt() {
        assert!(matches!(
            service("session-secret").verify("definitely-not-a-jwt"),
            Err(AuthError::Invalid)
        ));
    }

    #[test]
    fn verify_rejects_a_token_signed_with_another_secret() {
        let token = service("secret-a").issue(user(), 0);
        assert!(matches!(
            service("secret-b").verify(&token),
            Err(AuthError::Invalid)
        ));
    }

    #[test]
    fn verify_rejects_an_expired_token() {
        let svc = service("session-secret");
        // Minted two hours ago with a one-hour TTL and zero leeway: past expiry.
        let issued = OffsetDateTime::now_utc() - Duration::hours(2);
        let token = svc.issue_at(user(), 0, issued);
        assert!(matches!(svc.verify(&token), Err(AuthError::Invalid)));
    }

    #[test]
    fn session_cookie_carries_the_security_attributes() {
        let cookie = TokenService::new("k", TTL_SECS, true).session_cookie("tok".to_owned());
        assert_eq!(cookie.name(), SESSION_COOKIE);
        assert_eq!(cookie.value(), "tok");
        assert_eq!(cookie.http_only(), Some(true));
        assert_eq!(cookie.secure(), Some(true));
        assert_eq!(cookie.same_site(), Some(SameSite::Lax));
        assert_eq!(cookie.path(), Some("/"));
        assert_eq!(cookie.max_age().map(Duration::whole_seconds), Some(3600));
    }

    #[test]
    fn cookie_secure_flag_follows_config() {
        let cookie = TokenService::new("k", TTL_SECS, false).session_cookie("t".to_owned());
        assert_eq!(cookie.secure(), Some(false));
    }

    #[test]
    fn clear_cookie_is_empty_and_expires_immediately() {
        let cookie = service("session-secret").clear_cookie();
        assert_eq!(cookie.value(), "");
        assert_eq!(cookie.max_age().map(Duration::whole_seconds), Some(0));
    }
}
