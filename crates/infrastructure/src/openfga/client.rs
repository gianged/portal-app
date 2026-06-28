use std::fmt;

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};

use domain::{
    error::AuthzError,
    ids::UserId,
    ports::authz_client::{AuthzClient, RelationTuple},
};

/// Configuration for the `OpenFGA` HTTP client.
#[derive(Clone)]
pub struct OpenFgaConfig {
    pub endpoint: String,
    pub store_id: String,
    pub authorization_model_id: String,
    pub bearer_token: Option<String>,
}

// Manual `Debug` to keep the bearer token out of logs and panic dumps.
impl fmt::Debug for OpenFgaConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenFgaConfig")
            .field("endpoint", &self.endpoint)
            .field("store_id", &self.store_id)
            .field("authorization_model_id", &self.authorization_model_id)
            .field(
                "bearer_token",
                &self.bearer_token.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

/// HTTP-REST adapter implementing the [`AuthzClient`] port against an `OpenFGA` server.
pub struct OpenFgaAuthzClient {
    http: Client,
    endpoint: String,
    store_id: String,
    authorization_model_id: String,
    bearer_token: Option<String>,
}

impl OpenFgaAuthzClient {
    pub fn new(cfg: OpenFgaConfig) -> Result<Self, AuthzError> {
        let http = Client::builder()
            .tls_backend_rustls()
            .build()
            .map_err(|e| AuthzError::Backend(e.to_string()))?;
        Ok(Self {
            http,
            endpoint: cfg.endpoint.trim_end_matches('/').to_string(),
            store_id: cfg.store_id,
            authorization_model_id: cfg.authorization_model_id,
            bearer_token: cfg.bearer_token,
        })
    }

    fn store_url(&self, path: &str) -> String {
        format!("{}/stores/{}/{}", self.endpoint, self.store_id, path)
    }

    fn user_key(user: UserId) -> String {
        format!("user:{}", user.0)
    }

    async fn post_json<Req, Resp>(&self, url: String, body: &Req) -> Result<Resp, AuthzError>
    where
        Req: Serialize + ?Sized,
        Resp: for<'de> Deserialize<'de>,
    {
        let mut req = self.http.post(&url).json(body);
        if let Some(token) = &self.bearer_token {
            req = req.bearer_auth(token);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| AuthzError::Backend(e.to_string()))?;
        let status = resp.status();
        if status.is_success() {
            resp.json::<Resp>()
                .await
                .map_err(|e| AuthzError::Backend(e.to_string()))
        } else {
            let detail = resp.text().await.unwrap_or_default();
            Err(AuthzError::Backend(format!(
                "openfga {url} returned {status}: {detail}"
            )))
        }
    }

    /// Single-tuple write that treats `OpenFGA`'s "already exists" 400 as success,
    /// making a re-grant idempotent. Batch `write_tuples` stays strict.
    async fn write_single(&self, req: &WriteRequest<'_>) -> Result<(), AuthzError> {
        let url = self.store_url("write");
        let mut http = self.http.post(&url).json(req);
        if let Some(token) = &self.bearer_token {
            http = http.bearer_auth(token);
        }
        let resp = http
            .send()
            .await
            .map_err(|e| AuthzError::Backend(e.to_string()))?;
        let status = resp.status();
        if status.is_success() {
            return Ok(());
        }
        let detail = resp.text().await.unwrap_or_default();
        if status == StatusCode::BAD_REQUEST && detail.contains("already exists") {
            return Ok(());
        }
        Err(AuthzError::Backend(format!(
            "openfga {url} returned {status}: {detail}"
        )))
    }
}

#[async_trait]
impl AuthzClient for OpenFgaAuthzClient {
    #[tracing::instrument(skip_all, fields(user = ?user))]
    async fn check(&self, user: UserId, relation: &str, object: &str) -> Result<bool, AuthzError> {
        let req = CheckRequest {
            tuple_key: TupleKeyDto {
                user: Self::user_key(user),
                relation: relation.to_string(),
                object: object.to_string(),
            },
            authorization_model_id: &self.authorization_model_id,
        };
        let resp: CheckResponse = self.post_json(self.store_url("check"), &req).await?;
        Ok(resp.allowed)
    }

    #[tracing::instrument(skip_all)]
    async fn write_tuple(
        &self,
        subject: &str,
        relation: &str,
        object: &str,
    ) -> Result<(), AuthzError> {
        let req = WriteRequest {
            writes: Some(TupleKeysDto {
                tuple_keys: vec![TupleKeyDto {
                    user: subject.to_string(),
                    relation: relation.to_string(),
                    object: object.to_string(),
                }],
            }),
            deletes: None,
            authorization_model_id: &self.authorization_model_id,
        };
        self.write_single(&req).await
    }

    #[tracing::instrument(skip_all)]
    async fn delete_tuple(
        &self,
        subject: &str,
        relation: &str,
        object: &str,
    ) -> Result<(), AuthzError> {
        let req = WriteRequest {
            writes: None,
            deletes: Some(TupleKeysDto {
                tuple_keys: vec![TupleKeyDto {
                    user: subject.to_string(),
                    relation: relation.to_string(),
                    object: object.to_string(),
                }],
            }),
            authorization_model_id: &self.authorization_model_id,
        };
        let _: serde_json::Value = self.post_json(self.store_url("write"), &req).await?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn write_tuples(
        &self,
        writes: &[RelationTuple],
        deletes: &[RelationTuple],
    ) -> Result<(), AuthzError> {
        if writes.is_empty() && deletes.is_empty() {
            return Ok(());
        }
        let to_keys = |tuples: &[RelationTuple]| TupleKeysDto {
            tuple_keys: tuples
                .iter()
                .map(|t| TupleKeyDto {
                    user: t.subject.clone(),
                    relation: t.relation.clone(),
                    object: t.object.clone(),
                })
                .collect(),
        };
        let req = WriteRequest {
            writes: (!writes.is_empty()).then(|| to_keys(writes)),
            deletes: (!deletes.is_empty()).then(|| to_keys(deletes)),
            authorization_model_id: &self.authorization_model_id,
        };
        let _: serde_json::Value = self.post_json(self.store_url("write"), &req).await?;
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(user = ?user))]
    async fn list_objects(
        &self,
        user: UserId,
        relation: &str,
        object_type: &str,
    ) -> Result<Vec<String>, AuthzError> {
        let req = ListObjectsRequest {
            r#type: object_type.to_string(),
            relation: relation.to_string(),
            user: Self::user_key(user),
            authorization_model_id: &self.authorization_model_id,
        };
        let resp: ListObjectsResponse =
            self.post_json(self.store_url("list-objects"), &req).await?;
        Ok(resp.objects)
    }
}

#[derive(Serialize)]
struct TupleKeyDto {
    user: String,
    relation: String,
    object: String,
}

#[derive(Serialize)]
struct TupleKeysDto {
    tuple_keys: Vec<TupleKeyDto>,
}

#[derive(Serialize)]
struct CheckRequest<'a> {
    tuple_key: TupleKeyDto,
    authorization_model_id: &'a str,
}

#[derive(Deserialize)]
struct CheckResponse {
    #[serde(default)]
    allowed: bool,
}

#[derive(Serialize)]
struct WriteRequest<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    writes: Option<TupleKeysDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    deletes: Option<TupleKeysDto>,
    authorization_model_id: &'a str,
}

#[derive(Serialize)]
struct ListObjectsRequest<'a> {
    r#type: String,
    relation: String,
    user: String,
    authorization_model_id: &'a str,
}

#[derive(Deserialize)]
struct ListObjectsResponse {
    #[serde(default)]
    objects: Vec<String>,
}
