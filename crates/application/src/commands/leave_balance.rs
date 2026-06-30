use domain::ids::UserId;

/// HR sets a user's entitlement for a given year. Maps to an upsert of that year's
/// grant.
#[derive(Debug, Clone)]
pub struct SetLeaveGrantCommand {
    pub user_id: UserId,
    pub grant_year: u16,
    pub days_granted: f64,
}

/// HR posts a manual balance correction (positive or negative) against the user's
/// most-recent grant.
#[derive(Debug, Clone)]
pub struct AdjustBalanceCommand {
    pub user_id: UserId,
    pub delta: f64,
    pub reason: String,
}
