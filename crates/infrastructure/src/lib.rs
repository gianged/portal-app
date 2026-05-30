// The wrapped backend error type is declared in every public signature; a separate `# Errors`
// rustdoc block would only restate it. Keep the conversational doc style used elsewhere.
#![allow(clippy::missing_errors_doc)]

pub mod jobs;
pub mod local_storage;
pub mod openfga;
pub mod postgres;
pub mod redis;
pub mod scylla;
