//! Admin management of external API service accounts (Director/HR only,
//! gated inside `ServiceAccountService`). The create response carries the
//! plaintext key exactly once.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing,
};
use uuid::Uuid;

use domain::ids::ServiceAccountId;
use shared::dto::service_account::{
    CreateServiceAccountRequest, CreatedServiceAccountDto, ServiceAccountDto,
};

use crate::{
    app::AppState, dto, error::AppError, extractors::auth_user::AuthUser,
    extractors::validated_json::ValidatedJson,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/service-accounts", routing::post(create).get(list))
        .route("/service-accounts/{id}", routing::delete(revoke))
}

async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedJson(body): ValidatedJson<CreateServiceAccountRequest>,
) -> Result<Json<CreatedServiceAccountDto>, AppError> {
    let scopes: Vec<_> = body
        .scopes
        .iter()
        .copied()
        .map(dto::service_account_scope_domain)
        .collect();
    let created = state
        .service_accounts
        .create(auth.user_id, &body.name, &scopes)
        .await?;
    Ok(Json(dto::created_service_account_dto(&created)))
}

async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<ServiceAccountDto>>, AppError> {
    let accounts = state.service_accounts.list(auth.user_id).await?;
    Ok(Json(
        accounts.iter().map(dto::service_account_dto).collect(),
    ))
}

async fn revoke(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    state
        .service_accounts
        .revoke(auth.user_id, ServiceAccountId(id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
