//! `AppJson`: a `Json` body extractor whose rejections keep axum's status
//! (400/413/415/422) but carry the API's `{ code, message }` error body. The
//! deserialize-only twin of `ValidatedJson`, for request DTOs without a
//! `Validate` impl.

use axum::{
    Json,
    extract::{FromRequest, Request},
};
use serde::de::DeserializeOwned;

use crate::error::AppError;

/// A JSON body that deserialized; no validation beyond serde's.
#[derive(Debug)]
pub struct AppJson<T>(pub T);

impl<S, T> FromRequest<S> for AppJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(body) = Json::<T>::from_request(req, state)
            .await
            .map_err(|e| AppError::JsonRejection(e.status(), e.body_text()))?;
        Ok(Self(body))
    }
}
