use reqwasm::http::Request;
use serde::{Serialize, de::DeserializeOwned};
use shared::dto::common::ApiError;
use web_sys::FormData;

use crate::api::error::FrontendError;

#[must_use]
pub fn api_url(path: &str) -> String {
    format!("/api{path}")
}

pub async fn post_json<B, T>(path: &str, body: &B) -> Result<T, FrontendError>
where
    B: Serialize,
    T: DeserializeOwned,
{
    let body_str = serde_json::to_string(body)?;
    let resp = Request::post(&api_url(path))
        .header("content-type", "application/json")
        .body(body_str)
        .send()
        .await?;
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
    let body_str = serde_json::to_string(body)?;
    let resp = Request::patch(&api_url(path))
        .header("content-type", "application/json")
        .body(body_str)
        .send()
        .await?;
    handle_json(resp).await
}

/// `DELETE` (and other no-content actions): succeeds on any 2xx, ignoring the body.
pub async fn del(path: &str) -> Result<(), FrontendError> {
    let resp = Request::delete(&api_url(path)).send().await?;
    handle_empty(resp).await
}

/// `POST` with no JSON body, for lifecycle actions (`/submit`, `/approve`, …)
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
    let body_str = serde_json::to_string(body)?;
    let resp = Request::post(&api_url(path))
        .header("content-type", "application/json")
        .body(body_str)
        .send()
        .await?;
    handle_empty(resp).await
}

/// Multipart `POST` for file uploads. The caller builds the [`FormData`] from an
/// `<input type="file">`; we deliberately set no `content-type` so the browser
/// adds the multipart boundary itself.
pub async fn post_multipart<T>(path: &str, form: FormData) -> Result<T, FrontendError>
where
    T: DeserializeOwned,
{
    let resp = Request::post(&api_url(path)).body(form).send().await?;
    handle_json(resp).await
}

/// Build a `?k=v&…` query string from already-URL-safe pairs (enum tags, UUIDs,
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

async fn handle_json<T>(resp: reqwasm::http::Response) -> Result<T, FrontendError>
where
    T: DeserializeOwned,
{
    let status = resp.status();
    if !(200..300).contains(&status) {
        let body = resp.text().await.unwrap_or_default();
        // The server returns a stable `{ code, message }` body; surface its
        // message, falling back to the raw text if it isn't an `ApiError`.
        let message = serde_json::from_str::<ApiError>(&body).map_or(body, |err| err.message);
        return Err(FrontendError::Http { status, message });
    }
    let parsed = resp.json::<T>().await?;
    Ok(parsed)
}

async fn handle_empty(resp: reqwasm::http::Response) -> Result<(), FrontendError> {
    let status = resp.status();
    if !(200..300).contains(&status) {
        let body = resp.text().await.unwrap_or_default();
        let message = serde_json::from_str::<ApiError>(&body).map_or(body, |err| err.message);
        return Err(FrontendError::Http { status, message });
    }
    Ok(())
}
