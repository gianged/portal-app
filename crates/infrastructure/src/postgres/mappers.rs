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
