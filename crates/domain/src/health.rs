use serde::{Deserialize, Serialize};

/// Liveness of one external backend, as reported by readiness checks. `Degraded`
/// is the in-between state a recovering backend sits in while its circuit breaker
/// probes (half-open) before being trusted as fully `Up`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Up,
    Degraded,
    Down,
}

/// Identifies an external dependency. Keys every circuit breaker and health
/// entry, so the prober, registry, and readiness endpoint all agree on names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendId {
    Postgres,
    Scylla,
    Redis,
    OpenFga,
}

impl BackendId {
    /// Every backend, in a stable order for iterating the registry.
    pub const ALL: [BackendId; 4] = [
        BackendId::Postgres,
        BackendId::Scylla,
        BackendId::Redis,
        BackendId::OpenFga,
    ];

    /// Stable wire token, matching the serde rename.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Postgres => "postgres",
            Self::Scylla => "scylla",
            Self::Redis => "redis",
            Self::OpenFga => "open_fga",
        }
    }
}
