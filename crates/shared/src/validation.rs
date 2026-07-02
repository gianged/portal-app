pub mod announcement;
pub mod chat;
pub mod comment;
pub mod common;
pub mod daily_report;
pub mod day_off;
pub mod file;
pub mod flex_hours;
pub mod group;
pub mod holiday;
pub mod leave_balance;
pub mod notification;
pub mod overtime;
pub mod policy;
pub mod project;
pub mod request;
pub mod ticket;
pub mod user;

use crate::errors::SharedError;

/// A request body checkable as a whole; the server validates during extraction.
pub trait Validate {
    /// Checks every field, returning the first failure.
    ///
    /// # Errors
    ///
    /// Returns [`SharedError::Validation`] when any field is invalid.
    fn validate(&self) -> Result<(), SharedError>;
}
