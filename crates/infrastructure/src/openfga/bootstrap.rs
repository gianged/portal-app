use std::time::Duration;

use percent_encoding::NON_ALPHANUMERIC;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};

use domain::error::AuthzError;

use super::client::OpenFgaConfig;

/// Resolves an [`OpenFgaConfig`] at startup: finds or creates the named store,
/// then resolves the latest authorization-model id, uploading `model_json` if absent.
pub async fn resolve_config(
    endpoint: &str,
    store_name: &str,
    model_json: &str,
    bearer_token: Option<String>,
) -> Result<OpenFgaConfig, AuthzError> {
    let endpoint = endpoint.trim_end_matches('/').to_string();
    // Bounded like every other backend client so a stalled OpenFGA cannot hang boot.
    let http = Client::builder()
        .tls_backend_rustls()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| AuthzError::Backend(e.to_string()))?;

    let store_id = ensure_store(&http, &endpoint, store_name, bearer_token.as_deref()).await?;
    let authorization_model_id = ensure_model(
        &http,
        &endpoint,
        &store_id,
        model_json,
        bearer_token.as_deref(),
    )
    .await?;

    Ok(OpenFgaConfig {
        endpoint,
        store_id,
        authorization_model_id,
        bearer_token,
    })
}

/// Returns the id of the store named `name`, creating it if absent. Walks every
/// page: missing the store on a later page would create a duplicate and point
/// the app at an empty one.
async fn ensure_store(
    http: &Client,
    endpoint: &str,
    name: &str,
    bearer: Option<&str>,
) -> Result<String, AuthzError> {
    let mut url = format!("{endpoint}/stores?page_size=100");
    loop {
        let list: StoresResponse = get_json(http, url, bearer).await?;
        if let Some(store) = list.stores.into_iter().find(|s| s.name == name) {
            return Ok(store.id);
        }
        match list.continuation_token.filter(|t| !t.is_empty()) {
            Some(token) => {
                let token = percent_encoding::utf8_percent_encode(&token, NON_ALPHANUMERIC);
                url = format!("{endpoint}/stores?page_size=100&continuation_token={token}");
            }
            None => break,
        }
    }
    let created: StoreResponse = post_json(
        http,
        format!("{endpoint}/stores"),
        &json!({ "name": name }),
        bearer,
    )
    .await?;
    Ok(created.id)
}

/// Returns the latest authorization-model id (models come newest-first), uploading `model_json` when the store has none.
async fn ensure_model(
    http: &Client,
    endpoint: &str,
    store_id: &str,
    model_json: &str,
    bearer: Option<&str>,
) -> Result<String, AuthzError> {
    let url = format!("{endpoint}/stores/{store_id}/authorization-models?page_size=1");
    let existing: ModelsResponse = get_json(http, url, bearer).await?;
    if let Some(model) = existing.authorization_models.into_iter().next() {
        return Ok(model.id);
    }
    let body: Value = serde_json::from_str(model_json)
        .map_err(|e| AuthzError::Backend(format!("invalid openfga model json: {e}")))?;
    let created: WriteModelResponse = post_json(
        http,
        format!("{endpoint}/stores/{store_id}/authorization-models"),
        &body,
        bearer,
    )
    .await?;
    Ok(created.authorization_model_id)
}

async fn get_json<T>(http: &Client, url: String, bearer: Option<&str>) -> Result<T, AuthzError>
where
    T: for<'de> Deserialize<'de>,
{
    let mut req = http.get(&url);
    if let Some(token) = bearer {
        req = req.bearer_auth(token);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| AuthzError::Backend(e.to_string()))?;
    parse(resp, &url).await
}

async fn post_json<B, T>(
    http: &Client,
    url: String,
    body: &B,
    bearer: Option<&str>,
) -> Result<T, AuthzError>
where
    B: serde::Serialize + ?Sized,
    T: for<'de> Deserialize<'de>,
{
    let mut req = http.post(&url).json(body);
    if let Some(token) = bearer {
        req = req.bearer_auth(token);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| AuthzError::Backend(e.to_string()))?;
    parse(resp, &url).await
}

async fn parse<T>(resp: reqwest::Response, url: &str) -> Result<T, AuthzError>
where
    T: for<'de> Deserialize<'de>,
{
    let status = resp.status();
    if status.is_success() {
        resp.json::<T>()
            .await
            .map_err(|e| AuthzError::Backend(e.to_string()))
    } else {
        let detail = resp.text().await.unwrap_or_default();
        Err(AuthzError::Backend(format!(
            "openfga {url} returned {status}: {detail}"
        )))
    }
}

#[derive(Deserialize)]
struct StoresResponse {
    #[serde(default)]
    stores: Vec<StoreResponse>,
    #[serde(default)]
    continuation_token: Option<String>,
}

#[derive(Deserialize)]
struct StoreResponse {
    id: String,
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
struct ModelsResponse {
    #[serde(default)]
    authorization_models: Vec<ModelEntry>,
}

#[derive(Deserialize)]
struct ModelEntry {
    id: String,
}

#[derive(Deserialize)]
struct WriteModelResponse {
    authorization_model_id: String,
}
