//! Domain <-> wire projections for service accounts.

use domain::model;

use application::{permissions::ServiceAccountScope, service::CreatedServiceAccount};
use shared::dto::service_account::{
    CreatedServiceAccountDto, ServiceAccountDto, ServiceAccountScope as WireServiceAccountScope,
    ServiceAccountStatus as WireServiceAccountStatus,
};

#[must_use]
pub fn service_account_status_dto(status: model::ServiceAccountStatus) -> WireServiceAccountStatus {
    match status {
        model::ServiceAccountStatus::Active => WireServiceAccountStatus::Active,
        model::ServiceAccountStatus::Revoked => WireServiceAccountStatus::Revoked,
    }
}

#[must_use]
pub fn service_account_scope_domain(scope: WireServiceAccountScope) -> ServiceAccountScope {
    match scope {
        WireServiceAccountScope::Projects => ServiceAccountScope::Projects,
        WireServiceAccountScope::Requests => ServiceAccountScope::Requests,
        WireServiceAccountScope::Reports => ServiceAccountScope::Reports,
    }
}

#[must_use]
pub fn service_account_dto(account: &model::ServiceAccount) -> ServiceAccountDto {
    ServiceAccountDto {
        id: super::service_account_id(account.id),
        name: account.name.clone(),
        status: service_account_status_dto(account.status),
        created_by: super::user_id(account.created_by),
        revoked_at: account.revoked_at,
        created_at: account.created_at,
    }
}

#[must_use]
pub fn created_service_account_dto(created: &CreatedServiceAccount) -> CreatedServiceAccountDto {
    CreatedServiceAccountDto {
        account: service_account_dto(&created.account),
        key: created.key.clone(),
    }
}
