use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Error, Serialize, Deserialize)]
pub enum SharedError {
    #[error("validation: {0}")]
    Validation(String),
}
