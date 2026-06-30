use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::{Date, OffsetDateTime};

use crate::ids::{DayOffId, LeaveGrantId, LeaveTransactionId, UserId};

/// Leave is tracked in half-day units. `application` and `shared` mirror this in
/// their own constant since neither can depend on the other's crate.
pub const LEAVE_UNIT: f64 = 0.5;

/// Raised when a balance operation cannot be satisfied.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LeaveError {
    #[error("insufficient leave balance")]
    Insufficient,
}

/// An HR-granted yearly leave entitlement. Unused days carry up to
/// `policy.balance_carry_years` years (FIFO oldest-first), then expire on
/// `expires_on`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaveGrant {
    pub id: LeaveGrantId,
    pub user_id: UserId,
    pub grant_year: u16,
    pub days_granted: f64,
    pub days_remaining: f64,
    pub expires_on: Date,
    pub created_by: Option<UserId>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl LeaveGrant {
    /// Whether the grant has lapsed: `asof` is past its `expires_on` day.
    #[must_use]
    pub fn is_expired(&self, asof: Date) -> bool {
        asof > self.expires_on
    }
}

/// The kind of balance movement recorded in the immutable ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaveTxnKind {
    Grant,
    Consume,
    Refund,
    Adjust,
    Expire,
}

impl LeaveTxnKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Grant => "grant",
            Self::Consume => "consume",
            Self::Refund => "refund",
            Self::Adjust => "adjust",
            Self::Expire => "expire",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "grant" => Some(Self::Grant),
            "consume" => Some(Self::Consume),
            "refund" => Some(Self::Refund),
            "adjust" => Some(Self::Adjust),
            "expire" => Some(Self::Expire),
            _ => None,
        }
    }
}

/// One immutable entry in the balance ledger. `dayoff_id` links a consume / refund
/// to the leave request that drove it; `work_pct` is recorded on expiry when the
/// policy says so.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaveTransaction {
    pub id: LeaveTransactionId,
    pub user_id: UserId,
    pub grant_id: LeaveGrantId,
    pub kind: LeaveTxnKind,
    pub delta: f64,
    pub dayoff_id: Option<DayOffId>,
    pub work_pct: Option<f64>,
    pub reason: String,
    pub created_by: Option<UserId>,
    pub created_at: OffsetDateTime,
}

/// Draws `days` across non-expired grants oldest-first (by `expires_on`, then
/// `grant_year`) in [`LEAVE_UNIT`] steps. Returns the per-grant negative deltas to
/// apply, or [`LeaveError::Insufficient`] when the grants can't cover `days`.
///
/// Works in whole half-days internally so the half-step arithmetic is exact.
///
/// # Errors
/// Returns [`LeaveError::Insufficient`] when the available balance is below `days`.
pub fn allocate_fifo(
    grants: &[LeaveGrant],
    days: f64,
    asof: Date,
) -> Result<Vec<(LeaveGrantId, f64)>, LeaveError> {
    let step = 1.0 / LEAVE_UNIT;
    let mut need = (days * step).round() as i64;
    if need <= 0 {
        return Ok(Vec::new());
    }
    let mut open: Vec<&LeaveGrant> = grants
        .iter()
        .filter(|g| !g.is_expired(asof) && g.days_remaining > 0.0)
        .collect();
    open.sort_by(|a, b| {
        a.expires_on
            .cmp(&b.expires_on)
            .then(a.grant_year.cmp(&b.grant_year))
    });

    let mut deltas = Vec::new();
    for g in open {
        if need == 0 {
            break;
        }
        let avail = (g.days_remaining * step).round() as i64;
        let take = avail.min(need);
        if take > 0 {
            deltas.push((g.id, -(take as f64) * LEAVE_UNIT));
            need -= take;
        }
    }
    if need > 0 {
        return Err(LeaveError::Insufficient);
    }
    Ok(deltas)
}

#[cfg(test)]
mod tests {
    use time::Month;
    use uuid::Uuid;

    use super::*;

    fn grant(year: u16, remaining: f64, expires: Date) -> LeaveGrant {
        LeaveGrant {
            id: LeaveGrantId(Uuid::now_v7()),
            user_id: UserId(Uuid::now_v7()),
            grant_year: year,
            days_granted: remaining,
            days_remaining: remaining,
            expires_on: expires,
            created_by: None,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    fn dec31(year: i32) -> Date {
        Date::from_calendar_date(year, Month::December, 31).unwrap()
    }

    #[test]
    fn fifo_draws_oldest_first() {
        let asof = Date::from_calendar_date(2026, Month::January, 1).unwrap();
        let grants = vec![grant(2025, 2.0, dec31(2027)), grant(2024, 1.5, dec31(2026))];
        let deltas = allocate_fifo(&grants, 2.5, asof).unwrap();
        // Oldest (2026 expiry) drained first for 1.5, then 1.0 from the next.
        assert_eq!(deltas[0].1, -1.5);
        assert_eq!(deltas[1].1, -1.0);
    }

    #[test]
    fn fifo_errors_when_short() {
        let asof = Date::from_calendar_date(2026, Month::January, 1).unwrap();
        let grants = vec![grant(2025, 1.0, dec31(2027))];
        assert_eq!(
            allocate_fifo(&grants, 2.0, asof),
            Err(LeaveError::Insufficient)
        );
    }

    #[test]
    fn fifo_skips_expired() {
        let asof = Date::from_calendar_date(2026, Month::June, 1).unwrap();
        let grants = vec![grant(2024, 5.0, dec31(2025)), grant(2025, 1.0, dec31(2027))];
        let deltas = allocate_fifo(&grants, 1.0, asof).unwrap();
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].1, -1.0);
    }
}
