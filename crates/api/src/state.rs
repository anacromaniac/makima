//! Shared application state threaded through the Axum router.

/// Shared state available to all handlers.
///
/// Cloned cheaply per-request — `PgPool` and `String` are both arc-backed.
#[derive(Clone)]
pub struct AppState {
    /// PostgreSQL connection pool.
    pub pool: sqlx::PgPool,
    /// HS256 signing secret for JWT access tokens.
    pub jwt_secret: String,
}
