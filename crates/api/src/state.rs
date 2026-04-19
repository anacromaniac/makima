//! Shared application state threaded through Axum handlers.

use std::sync::Arc;

use domain::{PortfolioRepository, RefreshTokenRepository, UserRepository};

/// Application state available to all request handlers via [`axum::extract::State`].
///
/// Holds repository trait objects (ports) so the API layer never depends on
/// concrete database types. The `pool` is kept for infrastructure-level operations
/// (health checks, migration runs) that happen outside the repository abstraction.
#[derive(Clone)]
pub struct AppState {
    /// Raw connection pool — used only for `/ready` health checks and migrations.
    pub pool: sqlx::PgPool,
    /// User account storage (port).
    pub user_repo: Arc<dyn UserRepository>,
    /// Refresh token storage (port).
    pub refresh_token_repo: Arc<dyn RefreshTokenRepository>,
    /// Portfolio storage (port).
    pub portfolio_repo: Arc<dyn PortfolioRepository>,
    /// HS256 signing secret for JWT access tokens.
    pub jwt_secret: String,
}
