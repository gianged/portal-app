use std::sync::OnceLock;

use gloo::net::http::{Request, Response};
use serde::{Serialize, de::DeserializeOwned};
use shared::dto::common::{ApiError, ErrorCode};
use web_sys::FormData;

use crate::api::error::FrontendError;

static UNAUTHORIZED_HOOK: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

/// Install the hook run on every 401 response. Set once by `App`: it clears the
/// auth state so the route guard redirects expired sessions to `/login`.
pub fn on_unauthorized(hook: impl Fn() + Send + Sync + 'static) {
    let _ = UNAUTHORIZED_HOOK.set(Box::new(hook));
}

#[must_use]
pub fn api_url(path: &str) -> String {
    format!("/api{path}")
}

pub async fn post_json<B, T>(path: &str, body: &B) -> Result<T, FrontendError>
where
    B: Serialize,
    T: DeserializeOwned,
{
    let resp = Request::post(&api_url(path)).json(body)?.send().await?;
    handle_json(resp).await
}

pub async fn get_json<T>(path: &str) -> Result<T, FrontendError>
where
    T: DeserializeOwned,
{
    let resp = Request::get(&api_url(path)).send().await?;
    handle_json(resp).await
}

pub async fn patch_json<B, T>(path: &str, body: &B) -> Result<T, FrontendError>
where
    B: Serialize,
    T: DeserializeOwned,
{
    let resp = Request::patch(&api_url(path)).json(body)?.send().await?;
    handle_json(resp).await
}

/// `PUT` a JSON body for an idempotent upsert (policy, holiday, leave grant).
pub async fn put_json<B, T>(path: &str, body: &B) -> Result<T, FrontendError>
where
    B: Serialize,
    T: DeserializeOwned,
{
    let resp = Request::put(&api_url(path)).json(body)?.send().await?;
    handle_json(resp).await
}

/// `DELETE` (and other no-content actions): succeeds on any 2xx, ignoring the body.
pub async fn del(path: &str) -> Result<(), FrontendError> {
    let resp = Request::delete(&api_url(path)).send().await?;
    handle_empty(resp).await
}

/// `POST` with no JSON body, for lifecycle actions (`/submit`, `/approve`, ...)
/// that take their input from the path and return the updated resource.
pub async fn post_empty<T>(path: &str) -> Result<T, FrontendError>
where
    T: DeserializeOwned,
{
    let resp = Request::post(&api_url(path)).send().await?;
    handle_json(resp).await
}

/// `POST` with no body and no response body (204 actions like `/deactivate`).
pub async fn post_no_content(path: &str) -> Result<(), FrontendError> {
    let resp = Request::post(&api_url(path)).send().await?;
    handle_empty(resp).await
}

/// `POST` a JSON body for an action that returns 204 (e.g. transfer leadership).
pub async fn post_json_no_content<B>(path: &str, body: &B) -> Result<(), FrontendError>
where
    B: Serialize,
{
    let resp = Request::post(&api_url(path)).json(body)?.send().await?;
    handle_empty(resp).await
}

/// Multipart `POST` for file uploads. The caller builds the [`FormData`] from an
/// `<input type="file">`; we deliberately set no `content-type` so the browser
/// adds the multipart boundary itself.
pub async fn post_multipart<T>(path: &str, form: FormData) -> Result<T, FrontendError>
where
    T: DeserializeOwned,
{
    let resp = Request::post(&api_url(path)).body(form)?.send().await?;
    handle_json(resp).await
}

/// Build a `?k=v&...` query string from already-URL-safe pairs (enum tags, UUIDs,
/// ints, bools). Returns an empty string when there are no pairs.
#[must_use]
pub fn query(pairs: &[(&str, &str)]) -> String {
    if pairs.is_empty() {
        return String::new();
    }
    let joined = pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    format!("?{joined}")
}

async fn handle_json<T>(resp: Response) -> Result<T, FrontendError>
where
    T: DeserializeOwned,
{
    let status = resp.status();
    if !(200..300).contains(&status) {
        return Err(http_error(resp).await);
    }
    let parsed = resp.json::<T>().await?;
    Ok(parsed)
}

async fn handle_empty(resp: Response) -> Result<(), FrontendError> {
    let status = resp.status();
    if !(200..300).contains(&status) {
        return Err(http_error(resp).await);
    }
    Ok(())
}

/// Build a structured [`FrontendError::Http`] from a non-2xx response; a non-JSON
/// body falls back to `Unknown` with the raw text as the message. A 401 also
/// fires the [`on_unauthorized`] hook, clearing the session and redirecting to
/// `/login`.
async fn http_error(resp: Response) -> FrontendError {
    let status = resp.status();
    if status == 401
        && let Some(hook) = UNAUTHORIZED_HOOK.get()
    {
        hook();
    }
    let request_id = resp.headers().get("x-request-id");
    let body = resp.text().await.unwrap_or_default();
    let (code, message) = match serde_json::from_str::<ApiError>(&body) {
        Ok(err) => (err.code, err.message),
        Err(_) => (ErrorCode::Unknown, body),
    };
    FrontendError::Http {
        status,
        code,
        message,
        request_id,
    }
}
