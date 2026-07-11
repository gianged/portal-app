use crate::errors::SharedError;

// App-level field bounds; Postgres columns are uncapped `TEXT`, so the frontend uses these for `maxlength`.
pub const NAME_MIN: usize = 1;
pub const NAME_MAX: usize = 120;
pub const TITLE_MAX: usize = 200;
pub const DESCRIPTION_MAX: usize = 5_000;
pub const MESSAGE_BODY_MAX: usize = 4_000;
pub const PHONE_MAX: usize = 32;
pub const TIMEZONE_MAX: usize = 64;
pub const PASSWORD_MIN: usize = 8;
pub const PASSWORD_MAX: usize = 128;
// Caps on client-supplied lists so a JSON body cannot carry tens of thousands of entries.
pub const MENTIONS_MAX: usize = 20;
// Mirrored by `ChatService::MAX_MESSAGE_ATTACHMENTS` (application cannot depend on this crate).
pub const ATTACHMENT_KEYS_MAX: usize = 10;
pub const STORAGE_KEY_MAX: usize = 512;
pub const NOTIFICATION_BATCH_MAX: usize = 100;
pub const COMMENT_BODY_MAX: usize = 4_000;

/// Leave amounts are tracked in half-day units. Mirrors `domain`'s `LEAVE_UNIT`
/// (the two crates cannot share a const).
pub const LEAVE_UNIT: f64 = 0.5;

/// Rejects a value outside the inclusive `[min, max]` range.
///
/// # Errors
///
/// Returns [`SharedError::Validation`] when `value` is below `min` or above `max`.
pub fn in_range(field: &str, value: f64, min: f64, max: f64) -> Result<(), SharedError> {
    if value < min || value > max {
        return Err(SharedError::Validation(format!(
            "{field} must be between {min} and {max}"
        )));
    }
    Ok(())
}

/// Rejects a value that is not a whole multiple of [`LEAVE_UNIT`].
///
/// # Errors
///
/// Returns [`SharedError::Validation`] when `value` is not a multiple of 0.5.
pub fn half_step(field: &str, value: f64) -> Result<(), SharedError> {
    let scaled = value * 2.0;
    if (scaled - scaled.round()).abs() > 1e-6 {
        return Err(SharedError::Validation(format!(
            "{field} must be in steps of {LEAVE_UNIT}"
        )));
    }
    Ok(())
}

/// Rejects a value that is empty after trimming.
///
/// # Errors
///
/// Returns [`SharedError::Validation`] when `value` is empty or whitespace-only.
pub fn non_empty(field: &str, value: &str) -> Result<(), SharedError> {
    if value.trim().is_empty() {
        return Err(SharedError::Validation(format!("{field} is required")));
    }
    Ok(())
}

/// Rejects a value longer than `max` characters (counts `char`s, not bytes).
///
/// # Errors
///
/// Returns [`SharedError::Validation`] when `value` exceeds `max` characters.
pub fn max_len(field: &str, value: &str, max: usize) -> Result<(), SharedError> {
    if value.chars().count() > max {
        return Err(SharedError::Validation(format!(
            "{field} must be at most {max} characters"
        )));
    }
    Ok(())
}

/// Rejects a collection holding more than `max` items.
///
/// # Errors
///
/// Returns [`SharedError::Validation`] when `len` exceeds `max`.
pub fn max_items(field: &str, len: usize, max: usize) -> Result<(), SharedError> {
    if len > max {
        return Err(SharedError::Validation(format!(
            "{field} must have at most {max} items"
        )));
    }
    Ok(())
}

/// Rejects empty-after-trim, then enforces `min..=max` characters on the
/// trimmed value.
///
/// # Errors
///
/// Returns [`SharedError::Validation`] when `value` is empty, shorter than
/// `min`, or longer than `max` characters.
pub fn len_range(field: &str, value: &str, min: usize, max: usize) -> Result<(), SharedError> {
    non_empty(field, value)?;
    let len = value.trim().chars().count();
    if len < min {
        return Err(SharedError::Validation(format!(
            "{field} must be at least {min} characters"
        )));
    }
    if len > max {
        return Err(SharedError::Validation(format!(
            "{field} must be at most {max} characters"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn non_empty_rejects_blank() {
        assert!(super::non_empty("X", "   ").is_err());
        assert!(super::non_empty("X", "a").is_ok());
    }

    #[test]
    fn half_step_tolerates_both_sides_of_a_step() {
        assert!(super::half_step("X", 1.5).is_ok());
        assert!(super::half_step("X", 1.499_999_999_999_999_8).is_ok());
        assert!(super::half_step("X", 1.500_000_000_000_000_2).is_ok());
        assert!(super::half_step("X", 1.23).is_err());
    }

    #[test]
    fn max_len_boundary() {
        assert!(super::max_len("X", "abc", 3).is_ok());
        assert!(super::max_len("X", "abcd", 3).is_err());
    }

    #[test]
    fn len_range_boundaries() {
        assert!(super::len_range("X", "", 1, 3).is_err());
        assert!(super::len_range("X", "a", 1, 3).is_ok());
        assert!(super::len_range("X", "abc", 1, 3).is_ok());
        assert!(super::len_range("X", "abcd", 1, 3).is_err());
    }
}
