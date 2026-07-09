//! Generated gRPC bindings for the internal plane, plus the shared token
//! interceptors. Native-only: `frontend` and `shared` must never depend on it.

pub mod auth;

/// `portal.internal.v1`: `Jobs` (server -> workers enqueue) and `Query`
/// (internal read plane on the server).
pub mod internal {
    pub mod v1 {
        #![allow(clippy::pedantic)]
        tonic::include_proto!("portal.internal.v1");
    }
}

// Re-exported so every binary serves/probes the same gRPC and health versions.
pub use tonic;
pub use tonic_health;
