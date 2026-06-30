//! Domain <-> wire projections for leave balances.

use application::commands::leave_balance::{AdjustBalanceCommand, SetLeaveGrantCommand};
use domain::{ids::UserId, model};
use shared::dto::leave_balance::{
    AdjustBalanceRequest, LeaveBalanceDto, LeaveGrantDto, LeaveStatementDto, LeaveTransactionDto,
    LeaveTxnKind as WireKind, SetLeaveGrantRequest,
};

use super::{daily_report::fmt_date, day_off_id, leave_grant_id, leave_transaction_id};

// --- enums ---

#[must_use]
pub fn leave_txn_kind_dto(kind: model::LeaveTxnKind) -> WireKind {
    match kind {
        model::LeaveTxnKind::Grant => WireKind::Grant,
        model::LeaveTxnKind::Consume => WireKind::Consume,
        model::LeaveTxnKind::Refund => WireKind::Refund,
        model::LeaveTxnKind::Adjust => WireKind::Adjust,
        model::LeaveTxnKind::Expire => WireKind::Expire,
    }
}

// --- views ---

#[must_use]
pub fn leave_grant_dto(grant: &model::LeaveGrant) -> LeaveGrantDto {
    LeaveGrantDto {
        id: leave_grant_id(grant.id),
        grant_year: grant.grant_year,
        days_granted: grant.days_granted,
        days_remaining: grant.days_remaining,
        expires_on: fmt_date(grant.expires_on),
    }
}

#[must_use]
pub fn leave_balance_dto(available: f64, grants: &[model::LeaveGrant]) -> LeaveBalanceDto {
    LeaveBalanceDto {
        available,
        grants: grants.iter().map(leave_grant_dto).collect(),
    }
}

#[must_use]
pub fn leave_transaction_dto(txn: &model::LeaveTransaction) -> LeaveTransactionDto {
    LeaveTransactionDto {
        id: leave_transaction_id(txn.id),
        kind: leave_txn_kind_dto(txn.kind),
        delta: txn.delta,
        dayoff_id: txn.dayoff_id.map(day_off_id),
        work_pct: txn.work_pct,
        reason: txn.reason.clone(),
        created_at: txn.created_at,
    }
}

#[must_use]
pub fn leave_statement_dto(
    grants: &[model::LeaveGrant],
    txns: &[model::LeaveTransaction],
) -> LeaveStatementDto {
    LeaveStatementDto {
        grants: grants.iter().map(leave_grant_dto).collect(),
        transactions: txns.iter().map(leave_transaction_dto).collect(),
    }
}

// --- commands ---

#[must_use]
pub fn set_leave_grant_command(user_id: UserId, req: SetLeaveGrantRequest) -> SetLeaveGrantCommand {
    SetLeaveGrantCommand {
        user_id,
        grant_year: req.grant_year,
        days_granted: req.days_granted,
    }
}

#[must_use]
pub fn adjust_balance_command(user_id: UserId, req: AdjustBalanceRequest) -> AdjustBalanceCommand {
    AdjustBalanceCommand {
        user_id,
        delta: req.delta,
        reason: req.reason,
    }
}
