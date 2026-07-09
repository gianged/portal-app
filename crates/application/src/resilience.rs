//! Self-healing runtime building blocks: backoff, task supervision, circuit
//! breaking, health tracking, and the chat-spool drainer. Pure orchestration -
//! the concrete probes/spool live in `infrastructure` behind `domain` ports.

mod backoff;
mod circuit;
mod dispatch_queue;
mod drainer;
mod health_registry;
mod supervisor;

pub use backoff::Backoff;
pub use circuit::{CircuitBreaker, CircuitConfig};
pub use dispatch_queue::{DispatchQueue, SpooledJob};
pub use drainer::{Drainer, DrainerConfig};
pub use health_registry::HealthRegistry;
pub use supervisor::{SupervisorHandle, supervise};
