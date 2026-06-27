use serde::{Deserialize, Serialize};

/// Liveness of one backend on the wire. Mirrors `domain::health::HealthStatus`;
/// `Unknown` lets an older frontend decode a status a newer backend adds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendStatus {
    Up,
    Degraded,
    Down,
    #[serde(other)]
    Unknown,
}

/// One backend's readiness line. `backend` is the stable `snake_case` id
/// (`postgres`, `scylla`, `redis`, `open_fga`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendHealth {
    pub backend: String,
    pub status: BackendStatus,
}

/// Aggregate readiness: `status` is the worst of `backends`. The server returns
/// it from `/readyz` (200 unless any backend is `Down`), and the frontend can
/// render a status panel from the same type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessResponse {
    pub status: BackendStatus,
    pub backends: Vec<BackendHealth>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_serializes_to_snake_case() {
        let json = serde_json::to_string(&BackendStatus::Degraded).expect("serialize");
        assert_eq!(json, r#""degraded""#);
    }

    #[test]
    fn unknown_status_decodes() {
        let parsed: BackendStatus = serde_json::from_str(r#""teapot""#).expect("deserialize");
        assert_eq!(parsed, BackendStatus::Unknown);
    }

    #[test]
    fn readiness_round_trips() {
        let original = ReadinessResponse {
            status: BackendStatus::Down,
            backends: vec![BackendHealth {
                backend: "postgres".to_owned(),
                status: BackendStatus::Down,
            }],
        };
        let json = serde_json::to_string(&original).expect("serialize");
        let back: ReadinessResponse = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.status, original.status);
        assert_eq!(back.backends.len(), 1);
    }
}
