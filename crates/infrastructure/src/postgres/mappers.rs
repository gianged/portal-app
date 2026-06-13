use domain::error::RepositoryError;
use sqlx::error::ErrorKind;

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

/// Escape LIKE metacharacters (backslash first — it's the escape char itself) and
/// wrap in `%…%` for literal ILIKE matching.
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
