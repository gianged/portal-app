use domain::error::RepositoryError;
use sqlx::error::ErrorKind;
use time::{Date, Month};

pub(crate) fn map_pg_error(e: sqlx::Error) -> RepositoryError {
    match e {
        sqlx::Error::RowNotFound => RepositoryError::NotFound,
        sqlx::Error::Database(db_err) => match db_err.kind() {
            ErrorKind::UniqueViolation
            | ErrorKind::ForeignKeyViolation
            | ErrorKind::CheckViolation => RepositoryError::Conflict(db_err.to_string()),
            _ => RepositoryError::Backend(db_err.to_string()),
        },
        other => RepositoryError::Backend(other.to_string()),
    }
}

/// Inclusive first/last day of a calendar month.
pub(crate) fn month_bounds(year: i32, month: u32) -> Result<(Date, Date), RepositoryError> {
    let month_u8 = u8::try_from(month).unwrap_or(1);
    let m =
        Month::try_from(month_u8).map_err(|_| RepositoryError::Backend("invalid month".into()))?;
    let first = Date::from_calendar_date(year, m, 1)
        .map_err(|e| RepositoryError::Backend(e.to_string()))?;
    let last = Date::from_calendar_date(year, m, m.length(year))
        .map_err(|e| RepositoryError::Backend(e.to_string()))?;
    Ok((first, last))
}

/// Escapes LIKE metacharacters (backslash first, it's the escape char itself) and wraps in `%...%` for literal ILIKE matching.
pub(crate) fn like_pattern(q: &str) -> String {
    let escaped = q
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

#[cfg(test)]
mod tests {
    use super::like_pattern;

    #[test]
    fn metacharacters_are_escaped() {
        assert_eq!(like_pattern("abc"), "%abc%");
        assert_eq!(like_pattern("50%"), "%50\\%%");
        assert_eq!(like_pattern("a_b"), "%a\\_b%");
        assert_eq!(like_pattern("a\\b"), "%a\\\\b%");
    }
}
