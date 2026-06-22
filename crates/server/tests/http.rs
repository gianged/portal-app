//! HTTP integration tests: drive the real [`server::app::router`] over in-memory
//! fakes via `tower`'s `oneshot`, asserting status codes, the stable
//! `{code,message}` error body, auth gating, rate limiting, and CORS — with no
//! infrastructure running.

mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
    response::Response,
};
use serde::de::DeserializeOwned;
use tower::ServiceExt;
use uuid::Uuid;

use domain::ids::UserId;
use shared::dto::{
    common::{ApiError, ErrorCode},
    user::{LoginRequest, UserDto},
};

use server::{
    app::{cors_layer, router},
    middleware::rate_limit::RateLimits,
};

use common::{active_user, default_test_app, test_app};

async fn decode<T: DeserializeOwned>(response: Response) -> T {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("buffer response body");
    serde_json::from_slice(&bytes).expect("decode response body")
}

fn get(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .body(Body::empty())
        .expect("build request")
}

fn authed_get(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header(header::COOKIE, format!("portal_session={token}"))
        .body(Body::empty())
        .expect("build request")
}

#[tokio::test]
async fn healthz_returns_ok() {
    let app = default_test_app();
    let response = router(app.state).oneshot(get("/healthz")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&bytes[..], b"ok");
}

#[tokio::test]
async fn protected_route_without_cookie_is_401() {
    let app = default_test_app();
    let response = router(app.state).oneshot(get("/api/v1/me")).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body: ApiError = decode(response).await;
    assert_eq!(body.code, ErrorCode::Unauthenticated);
}

#[tokio::test]
async fn protected_route_with_garbage_cookie_is_401() {
    let app = default_test_app();
    let response = router(app.state)
        .oneshot(authed_get("/api/v1/me", "not-a-jwt"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body: ApiError = decode(response).await;
    assert_eq!(body.code, ErrorCode::Unauthenticated);
}

#[tokio::test]
async fn login_with_unknown_email_is_401_invalid_credentials() {
    let app = default_test_app();
    let payload = LoginRequest {
        email: "nobody@example.com".to_owned(),
        password: "whatever".to_owned(),
    };
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/login")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&payload).unwrap()))
        .unwrap();
    let response = router(app.state).oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body: ApiError = decode(response).await;
    assert_eq!(body.code, ErrorCode::InvalidCredentials);
}

#[tokio::test]
async fn me_with_valid_cookie_but_missing_user_is_404() {
    let app = default_test_app();
    let token = app.state.token.issue(UserId(Uuid::now_v7()), 0);
    let response = router(app.state)
        .oneshot(authed_get("/api/v1/me", &token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body: ApiError = decode(response).await;
    assert_eq!(body.code, ErrorCode::NotFound);
}

#[tokio::test]
async fn me_with_valid_cookie_and_seeded_user_is_200() {
    let app = default_test_app();
    let uid = UserId(Uuid::now_v7());
    app.users
        .users
        .lock()
        .unwrap()
        .push(active_user(uid, "me@example.com"));
    let token = app.state.token.issue(uid, 0);
    let response = router(app.state)
        .oneshot(authed_get("/api/v1/me", &token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: UserDto = decode(response).await;
    assert_eq!(body.email, "me@example.com");
}

#[tokio::test]
async fn replaying_a_token_after_logout_is_401() {
    let app = default_test_app();
    let uid = UserId(Uuid::now_v7());
    app.users
        .users
        .lock()
        .unwrap()
        .push(active_user(uid, "replay@example.com"));
    let token = app.state.token.issue(uid, 0);
    let service = router(app.state);

    // Logout with the cookie attached denylists its jti server-side.
    let logout = Request::builder()
        .method("POST")
        .uri("/api/v1/logout")
        .header(header::COOKIE, format!("portal_session={token}"))
        .body(Body::empty())
        .unwrap();
    let response = service.clone().oneshot(logout).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = service
        .oneshot(authed_get("/api/v1/me", &token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body: ApiError = decode(response).await;
    assert_eq!(body.code, ErrorCode::Unauthenticated);
}

#[tokio::test]
async fn token_minted_before_a_version_bump_is_401() {
    let app = default_test_app();
    let uid = UserId(Uuid::now_v7());
    app.users
        .users
        .lock()
        .unwrap()
        .push(active_user(uid, "bumped@example.com"));
    let token = app.state.token.issue(uid, 0);

    // A bump (deactivation, password change) outdates every version-0 token.
    app.revocation.versions.lock().unwrap().insert(uid, 1);

    let response = router(app.state)
        .oneshot(authed_get("/api/v1/me", &token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body: ApiError = decode(response).await;
    assert_eq!(body.code, ErrorCode::Unauthenticated);
}

#[tokio::test]
async fn per_user_rate_limit_returns_429() {
    // api ceiling 0: the per-user limiter trips on the first authenticated call.
    let app = test_app(RateLimits {
        auth: 1000,
        api: 0,
        chat: 1000,
    });
    let token = app.state.token.issue(UserId(Uuid::now_v7()), 0);
    let response = router(app.state)
        .oneshot(authed_get("/api/v1/me", &token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    let body: ApiError = decode(response).await;
    assert_eq!(body.code, ErrorCode::RateLimited);
}

#[tokio::test]
async fn unknown_route_is_404() {
    let app = default_test_app();
    let response = router(app.state)
        .oneshot(get("/api/v1/does-not-exist"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn cors_preflight_reflects_allowed_origin() {
    let app = default_test_app();
    let service = router(app.state).layer(cors_layer(&["http://localhost:8080".to_owned()]));
    let request = Request::builder()
        .method("OPTIONS")
        .uri("/api/v1/me")
        .header(header::ORIGIN, "http://localhost:8080")
        .header("access-control-request-method", "GET")
        .body(Body::empty())
        .unwrap();
    let response = service.oneshot(request).await.unwrap();

    assert_eq!(
        response
            .headers()
            .get("access-control-allow-origin")
            .expect("allow-origin header"),
        "http://localhost:8080",
    );
    assert_eq!(
        response
            .headers()
            .get("access-control-allow-credentials")
            .expect("allow-credentials header"),
        "true",
    );
}
