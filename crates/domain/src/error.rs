use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TransitionError {
    #[error("cannot transition from {current} to {target}")]
    Invalid {
        current: &'static str,
        target: &'static str,
    },
}

impl TransitionError {
    #[must_use]
    pub const fn invalid(current: &'static str, target: &'static str) -> Self {
        Self::Invalid { current, target }
    }
}

#[derive(Debug, Error)]
pub enum RepositoryError {
    #[error("entity not found")]
    NotFound,
    #[error("conflicting state: {0}")]
    Conflict(String),
    #[error("backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Error)]
pub enum AuthzError {
    #[error("access denied")]
    Denied,
    #[error("backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("object not found")]
    NotFound,
    #[error("invalid storage key: {0}")]
    InvalidKey(String),
    #[error("backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Error)]
pub enum EventError {
    #[error("backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Error)]
pub enum JobError {
    #[error("backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Error)]
pub enum MailError {
    #[error("invalid message: {0}")]
    Invalid(String),
    #[error("backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("layout error: {0}")]
    Layout(String),
    #[error("backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Error)]
pub enum HealthError {
    #[error("backend timed out")]
    Timeout,
    #[error("backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Error)]
pub enum SpoolError {
    #[error("backend error: {0}")]
    Backend(String),
}
