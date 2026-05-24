use reqwasm::http::Request;
use serde::{Serialize, de::DeserializeOwned};

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

async fn handle_json<T>(resp: reqwasm::http::Response) -> Result<T, FrontendError>
where
    T: DeserializeOwned,
{
    let status = resp.status();
    if !(200..300).contains(&status) {
        let message = resp.text().await.unwrap_or_default();
        return Err(FrontendError::Http { status, message });
    }
    let parsed = resp.json::<T>().await?;
    Ok(parsed)
}
