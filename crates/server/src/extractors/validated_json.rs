//! `ValidatedJson`: a `Json` body extractor that also runs the DTO's
//! `shared::validation::Validate` impl, so handlers cannot skip validation.
//! Malformed JSON rejects with the same `{ code, message }` body as a
//! validation failure instead of axum's plain-text default.

use axum::{
    Json,
    extract::{FromRequest, Request},
};
use serde::de::DeserializeOwned;

use shared::validation::Validate;

use crate::error::AppError;

/// A JSON body that deserialized AND passed its `Validate` impl.
#[derive(Debug)]
pub struct ValidatedJson<T>(pub T);

impl<S, T> FromRequest<S> for ValidatedJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Validate,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(body) = Json::<T>::from_request(req, state)
            .await
            .map_err(|e| AppError::Validation(e.body_text()))?;
        body.validate()
            .map_err(|e| AppError::Validation(e.to_string()))?;
        Ok(Self(body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use axum::{
        body::Body,
        http::{Request as HttpRequest, StatusCode, header},
        response::IntoResponse,
    };
    use serde::Deserialize;

    use shared::errors::SharedError;

    #[derive(Debug, Deserialize)]
    struct Probe {
        name: String,
    }

    impl Validate for Probe {
        fn validate(&self) -> Result<(), SharedError> {
            if self.name.is_empty() {
                return Err(SharedError::Validation("Name is required".into()));
            }
            Ok(())
        }
    }

    fn json_request(body: &str) -> Request {
        HttpRequest::builder()
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_owned()))
            .expect("build request")
    }

    #[tokio::test]
    async fn passes_a_valid_body_through() {
        let extracted = ValidatedJson::<Probe>::from_request(json_request(r#"{"name":"a"}"#), &())
            .await
            .expect("valid body extracts");
        assert_eq!(extracted.0.name, "a");
    }

    #[tokio::test]
    async fn invalid_field_rejects_as_400() {
        let rejection = ValidatedJson::<Probe>::from_request(json_request(r#"{"name":""}"#), &())
            .await
            .expect_err("failing Validate must reject");
        assert_eq!(rejection.into_response().status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn malformed_json_rejects_as_400() {
        let rejection = ValidatedJson::<Probe>::from_request(json_request("{nope"), &())
            .await
            .expect_err("malformed JSON must reject");
        assert_eq!(rejection.into_response().status(), StatusCode::BAD_REQUEST);
    }
}
