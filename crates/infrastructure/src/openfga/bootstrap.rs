use reqwest::Client;
use serde::Deserialize;
use serde_json::json;

use domain::error::AuthzError;

use super::client::OpenFgaConfig;

/// Resolves an [`OpenFgaConfig`] at startup: finds or creates the named store,
/// then resolves the latest authorization-model id (uploading `model_json` when
/// the store has none yet). This replaces the one-shot `openfga-init.sh`
/// bootstrap so the server is self-initialising.
///
/// `model_json` is the authorization model in `OpenFGA` request-body shape
/// (`schema_version` + `type_definitions`), i.e. the contents of
/// `infra/openfga/authorization-model.json`.
pub async fn resolve_config(
    endpoint: &str,
    store_name: &str,
    model_json: &str,
    bearer_token: Option<String>,
) -> Result<OpenFgaConfig, AuthzError> {
    let endpoint = endpoint.trim_end_matches('/').to_string();
    let http = Client::builder()
        .use_rustls_tls()
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

/// Returns the id of the store named `name`, creating it if absent. A
/// single-tenant portal has a handful of stores at most, so the first listing
/// page is sufficient.
async fn ensure_store(
    http: &Client,
    endpoint: &str,
    name: &str,
    bearer: Option<&str>,
) -> Result<String, AuthzError> {
    let list: StoresResponse = get_json(http, format!("{endpoint}/stores"), bearer).await?;
    if let Some(store) = list.stores.into_iter().find(|s| s.name == name) {
        return Ok(store.id);
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

/// Returns the latest authorization-model id for the store, uploading
/// `model_json` first when the store has no model yet. `OpenFGA` returns models
/// newest-first, so the first entry is the latest.
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
    let body: serde_json::Value = serde_json::from_str(model_json)
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
