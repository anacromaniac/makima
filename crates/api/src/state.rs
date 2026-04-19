//! Shared application state threaded through Axum handlers.

use std::sync::Arc;

use application::{
    assets::AssetService, auth::AuthService, portfolios::PortfolioService,
    positions::PositionService, transactions::TransactionService, users::UserService,
};

/// Application state available to all request handlers via [`axum::extract::State`].
///
/// Holds application services so the API layer stays focused on transport
/// concerns. The `pool` is kept for infrastructure-level operations (health
/// checks, migration runs) that happen outside the application service layer.
#[derive(Clone)]
pub struct AppState {
    /// Raw connection pool — used only for `/ready` health checks and migrations.
    pub pool: sqlx::PgPool,
    /// Authentication workflows.
    pub auth_service: Arc<AuthService>,
    /// Shared asset workflows.
    pub asset_service: Arc<AssetService>,
    /// Portfolio workflows.
    pub portfolio_service: Arc<PortfolioService>,
    /// Derived position workflows.
    pub position_service: Arc<PositionService>,
    /// Transaction workflows.
    pub transaction_service: Arc<TransactionService>,
    /// User profile workflows.
    pub user_service: Arc<UserService>,
    /// HS256 signing secret for JWT access tokens.
    pub jwt_secret: String,
}
