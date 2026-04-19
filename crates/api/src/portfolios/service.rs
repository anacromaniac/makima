//! Portfolio service — business logic for portfolio CRUD operations.
//!
//! This module has no axum or sqlx dependencies. It works exclusively through
//! the [`domain::PortfolioRepository`] port.

use domain::{
    NewPortfolio, PaginatedResult, PaginationParams, Portfolio, PortfolioRepository,
    RepositoryError,
};
use uuid::Uuid;

/// Errors that can occur during portfolio operations.
#[derive(Debug, thiserror::Error)]
pub enum PortfolioError {
    /// Portfolio not found, or it belongs to a different user.
    ///
    /// Returns 404 in both cases to avoid leaking the existence of other users'
    /// portfolios.
    #[error("portfolio not found")]
    NotFound,
    /// Underlying repository error (storage failure).
    #[error("repository error: {0}")]
    Repository(#[from] RepositoryError),
}

/// Create a new portfolio owned by `user_id`.
pub async fn create(
    repo: &dyn PortfolioRepository,
    user_id: Uuid,
    name: String,
    description: Option<String>,
) -> Result<Portfolio, PortfolioError> {
    let portfolio = repo
        .create(&NewPortfolio {
            user_id,
            name,
            description,
            base_currency: "EUR".to_string(),
        })
        .await?;
    Ok(portfolio)
}

/// Return all portfolios owned by `user_id`, paginated.
pub async fn list(
    repo: &dyn PortfolioRepository,
    user_id: Uuid,
    pagination: &PaginationParams,
) -> Result<PaginatedResult<Portfolio>, PortfolioError> {
    let result = repo.find_by_user_id(user_id, pagination).await?;
    Ok(result)
}

/// Return a single portfolio, verifying it belongs to `user_id`.
///
/// Returns [`PortfolioError::NotFound`] if the portfolio does not exist **or**
/// belongs to a different user.
pub async fn get(
    repo: &dyn PortfolioRepository,
    user_id: Uuid,
    portfolio_id: Uuid,
) -> Result<Portfolio, PortfolioError> {
    let portfolio = repo
        .find_by_id(portfolio_id)
        .await?
        .filter(|p| p.user_id == user_id)
        .ok_or(PortfolioError::NotFound)?;
    Ok(portfolio)
}

/// Update a portfolio's name and description, verifying ownership first.
pub async fn update(
    repo: &dyn PortfolioRepository,
    user_id: Uuid,
    portfolio_id: Uuid,
    name: String,
    description: Option<String>,
) -> Result<Portfolio, PortfolioError> {
    // Ownership check — returns 404 if not found or not owned.
    repo.find_by_id(portfolio_id)
        .await?
        .filter(|p| p.user_id == user_id)
        .ok_or(PortfolioError::NotFound)?;

    let portfolio = repo
        .update(portfolio_id, &name, description.as_deref())
        .await?;
    Ok(portfolio)
}

/// Delete a portfolio, verifying ownership first.
///
/// Cascade deletes all transactions belonging to the portfolio via the FK
/// constraint defined in the migration.
pub async fn delete(
    repo: &dyn PortfolioRepository,
    user_id: Uuid,
    portfolio_id: Uuid,
) -> Result<(), PortfolioError> {
    // Ownership check — returns 404 if not found or not owned.
    repo.find_by_id(portfolio_id)
        .await?
        .filter(|p| p.user_id == user_id)
        .ok_or(PortfolioError::NotFound)?;

    repo.delete(portfolio_id).await?;
    Ok(())
}
