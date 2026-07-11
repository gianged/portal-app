//! Domain <-> wire projection for the audit log.

use domain::model;
use shared::dto::{
    audit::{AuditAction as WireAuditAction, AuditLogDto},
    common::UserSummaryDto,
};

#[must_use]
pub fn audit_action_dto(action: model::AuditAction) -> WireAuditAction {
    match action {
        model::AuditAction::Create => WireAuditAction::Create,
        model::AuditAction::Update => WireAuditAction::Update,
        model::AuditAction::Delete => WireAuditAction::Delete,
        model::AuditAction::StatusChange => WireAuditAction::StatusChange,
        model::AuditAction::Assign => WireAuditAction::Assign,
        model::AuditAction::Transfer => WireAuditAction::Transfer,
    }
}

/// Projects an audit row plus its already-resolved actor summary. A `None` actor
/// is a system action (or an actor that has since been hard-deleted).
#[must_use]
pub fn audit_log_dto(log: &model::AuditLog, actor: Option<UserSummaryDto>) -> AuditLogDto {
    AuditLogDto {
        id: super::audit_log_id(log.id),
        actor,
        action: audit_action_dto(log.action),
        entity_schema: log.entity_schema.clone(),
        entity_table: log.entity_table.clone(),
        entity_id: log.entity_id,
        occurred_at: log.occurred_at,
    }
}
