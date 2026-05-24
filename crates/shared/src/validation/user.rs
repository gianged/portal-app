use crate::errors::SharedError;

/// Lightweight client-side email format check.
///
/// # Errors
///
/// Returns [`SharedError::Validation`] when `email` is empty, missing `@`,
/// or has an empty local-part / domain-part, or whose domain contains no `.`.
pub fn validate_email(email: &str) -> Result<(), SharedError> {
    let trimmed = email.trim();
    if trimmed.is_empty() {
        return Err(SharedError::Validation("Email is required".into()));
    }
    let Some((local, domain)) = trimmed.split_once('@') else {
        return Err(SharedError::Validation("Email must contain @".into()));
    };
    if local.is_empty() || domain.is_empty() || !domain.contains('.') {
        return Err(SharedError::Validation("Email is not valid".into()));
    }
    Ok(())
}

/// Lightweight client-side password length check.
///
/// # Errors
///
/// Returns [`SharedError::Validation`] when `password` is empty or shorter than 8 characters.
pub fn validate_password(password: &str) -> Result<(), SharedError> {
    if password.is_empty() {
        return Err(SharedError::Validation("Password is required".into()));
    }
    if password.len() < 8 {
        return Err(SharedError::Validation(
            "Password must be at least 8 characters".into(),
        ));
    }
    Ok(())
}
