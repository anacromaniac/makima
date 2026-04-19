//! Internal helpers for mapping infrastructure errors into domain repository errors.

use domain::RepositoryError;
use sqlx::Error as SqlxError;

/// Convert a sqlx error into the domain-level repository error type.
pub(crate) fn map_sqlx_error(error: SqlxError) -> RepositoryError {
    match error {
        SqlxError::Database(db_error) if db_error.is_unique_violation() => {
            RepositoryError::Conflict(db_error.message().to_string())
        }
        other => RepositoryError::Internal(other.to_string()),
    }
}
