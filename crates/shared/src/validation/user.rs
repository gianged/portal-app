use crate::{
    dto::user::{
        ChangePasswordRequest, CreateUserRequest, ResetPasswordRequest, UpdateProfileRequest,
    },
    errors::SharedError,
    validation::common::{
        NAME_MAX, NAME_MIN, PASSWORD_MAX, PASSWORD_MIN, PHONE_MAX, TIMEZONE_MAX, len_range,
        max_len, non_empty,
    },
};

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
/// Returns [`SharedError::Validation`] when `password` is empty, shorter than
/// [`PASSWORD_MIN`], or longer than [`PASSWORD_MAX`].
pub fn validate_password(password: &str) -> Result<(), SharedError> {
    if password.is_empty() {
        return Err(SharedError::Validation("Password is required".into()));
    }
    if password.len() < PASSWORD_MIN {
        return Err(SharedError::Validation(format!(
            "Password must be at least {PASSWORD_MIN} characters"
        )));
    }
    if password.len() > PASSWORD_MAX {
        return Err(SharedError::Validation(format!(
            "Password must be at most {PASSWORD_MAX} characters"
        )));
    }
    Ok(())
}

/// # Errors
///
/// Returns [`SharedError::Validation`] when `full_name` is empty or longer than
/// [`NAME_MAX`].
pub fn validate_full_name(full_name: &str) -> Result<(), SharedError> {
    len_range("Full name", full_name, NAME_MIN, NAME_MAX)
}

/// Phone is optional; an empty string is accepted. When present, only digits,
/// spaces, and `+ - ( )` are allowed.
///
/// # Errors
///
/// Returns [`SharedError::Validation`] when a non-empty `phone` exceeds
/// [`PHONE_MAX`] or contains disallowed characters.
pub fn validate_phone(phone: &str) -> Result<(), SharedError> {
    let trimmed = phone.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    max_len("Phone", trimmed, PHONE_MAX)?;
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | ' ' | '(' | ')'))
    {
        return Err(SharedError::Validation(
            "Phone may contain only digits, spaces, and + - ( )".into(),
        ));
    }
    Ok(())
}

/// Format/length check only; no timezone database lookup (that crate is not
/// wasm-safe and the server re-validates against a real tz set).
///
/// # Errors
///
/// Returns [`SharedError::Validation`] when `timezone` is empty or longer than
/// [`TIMEZONE_MAX`].
pub fn validate_timezone(timezone: &str) -> Result<(), SharedError> {
    non_empty("Timezone", timezone)?;
    max_len("Timezone", timezone, TIMEZONE_MAX)
}

/// Composite check for the create-user form; returns the first field failure.
///
/// # Errors
///
/// Returns the first [`SharedError::Validation`] among email, password, name,
/// phone, and timezone checks.
pub fn validate_create_user(req: &CreateUserRequest) -> Result<(), SharedError> {
    validate_email(&req.email)?;
    validate_password(&req.password)?;
    validate_full_name(&req.full_name)?;
    if let Some(phone) = &req.phone {
        validate_phone(phone)?;
    }
    validate_timezone(&req.timezone)?;
    Ok(())
}

/// Composite check for the change-password form.
///
/// # Errors
///
/// Returns [`SharedError::Validation`] when the current password is empty or
/// the new password fails [`validate_password`].
pub fn validate_change_password(req: &ChangePasswordRequest) -> Result<(), SharedError> {
    non_empty("Current password", &req.current_password)?;
    validate_password(&req.new_password)
}

/// Check for the HR reset-password form.
///
/// # Errors
///
/// Returns [`SharedError::Validation`] when the new password fails
/// [`validate_password`].
pub fn validate_reset_password(req: &ResetPasswordRequest) -> Result<(), SharedError> {
    validate_password(&req.new_password)
}

/// Composite check for the edit-profile form; validates only present fields.
///
/// # Errors
///
/// Returns the first [`SharedError::Validation`] among the fields that are set.
pub fn validate_update_profile(req: &UpdateProfileRequest) -> Result<(), SharedError> {
    if let Some(full_name) = &req.full_name {
        validate_full_name(full_name)?;
    }
    if let Some(phone) = &req.phone {
        validate_phone(phone)?;
    }
    if let Some(timezone) = &req.timezone {
        validate_timezone(timezone)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_checks() {
        assert!(validate_email("a@b.com").is_ok());
        assert!(validate_email("").is_err());
        assert!(validate_email("nope").is_err());
        assert!(validate_email("a@b").is_err());
    }

    #[test]
    fn password_length() {
        assert!(validate_password("short").is_err());
        assert!(validate_password("longenough").is_ok());
    }

    #[test]
    fn create_user_composite() {
        let req = CreateUserRequest {
            email: "a@b.com".to_owned(),
            password: "longenough".to_owned(),
            full_name: "Ada Lovelace".to_owned(),
            phone: None,
            timezone: "UTC".to_owned(),
            system_role: None,
        };
        assert!(validate_create_user(&req).is_ok());
    }
}
