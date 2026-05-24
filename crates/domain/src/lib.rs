// Reason: every fallible domain method returns `TransitionError::Invalid` and only
// that — the failure mode is uniform and documented on the enum itself; per-method
// `# Errors` sections would restate the same sentence ~50 times without adding info.
#![allow(clippy::missing_errors_doc)]

pub mod announcement;
pub mod audit;
pub mod chat;
pub mod error;
pub mod group;
pub mod ids;
pub mod notification;
pub mod ports;
pub mod project;
pub mod request;
pub mod ticket;
pub mod user;
