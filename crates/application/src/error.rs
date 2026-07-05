use domain::error::{
    AuthzError, EventError, JobError, RenderError, RepositoryError, StorageError, TransitionError,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("entity not found: {0}")]
    NotFound(&'static str),

    #[error("validation failed: {0}")]
    Validation(String),

    #[error("forbidden")]
    Forbidden,

    #[error("conflict: {0}")]
    Conflict(String),

    #[error(transparent)]
    Transition(#[from] TransitionError),

    #[error(transparent)]
    Repository(#[from] RepositoryError),

    #[error(transparent)]
    Storage(#[from] StorageError),

    #[error(transparent)]
    Event(#[from] EventError),
    #[error(transparent)]
    Job(#[from] JobError),
    #[error(transparent)]
    Render(#[from] RenderError),

    /// Authz backend fault, kept distinct from datastore faults for triage.
    #[error("authz backend error: {0}")]
    Authz(String),
}

impl From<AuthzError> for Error {
    fn from(err: AuthzError) -> Self {
        match err {
            AuthzError::Denied => Self::Forbidden,
            AuthzError::Backend(msg) => Self::Authz(msg),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
