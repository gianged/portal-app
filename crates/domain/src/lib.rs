// Reason: every fallible domain method returns `TransitionError::Invalid` and only
// that — the failure mode is uniform and documented on the enum itself; per-method
// `# Errors` sections would restate the same sentence ~50 times without adding info.
#![allow(clippy::missing_errors_doc)]

pub mod error;
pub mod ids;
pub mod model;
pub mod ports;
pub mod repository;
