pub mod bootstrap;
pub mod client;

pub use bootstrap::resolve_config;
pub use client::{OpenFgaAuthzClient, OpenFgaConfig};
