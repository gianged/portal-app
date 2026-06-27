//! Self-healing runtime building blocks: backoff, task supervision, circuit
//! breaking, health tracking, and the chat-spool drainer. Pure orchestration -
//! the concrete probes/spool live in `infrastructure` behind `domain` ports.

pub mod backoff;
pub mod circuit;
pub mod drainer;
pub mod health_registry;
pub mod supervisor;

pub use backoff::Backoff;
pub use circuit::{CircuitBreaker, CircuitConfig, guarded};
pub use drainer::{Drainer, DrainerConfig};
pub use health_registry::HealthRegistry;
pub use supervisor::{SupervisorHandle, supervise};
