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
    /// The workers' internal gRPC ingest plane, probed by the server.
    WorkersGrpc,
}

impl BackendId {
    /// Every backend, in a stable order for iterating the registry.
    pub const ALL: [BackendId; 5] = [
        BackendId::Postgres,
        BackendId::Scylla,
        BackendId::Redis,
        BackendId::OpenFga,
        BackendId::WorkersGrpc,
    ];

    /// Stable wire token, matching the serde rename.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Postgres => "postgres",
            Self::Scylla => "scylla",
            Self::Redis => "redis",
            Self::OpenFga => "open_fga",
            Self::WorkersGrpc => "workers_grpc",
        }
    }

    /// Whether this backend being down should flip readiness. The workers gRPC
    /// plane never gates: job dispatch falls back to the direct apalis path.
    #[must_use]
    pub const fn gates_readiness(self) -> bool {
        !matches!(self, Self::WorkersGrpc)
    }
}
