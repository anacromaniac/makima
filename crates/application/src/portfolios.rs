//! Portfolio use cases.

use std::sync::Arc;

use domain::{
    NewPortfolio, PaginatedResult, PaginationParams, Portfolio, PortfolioRepository,
    RepositoryError,
};
use uuid::Uuid;

/// Errors that can occur during portfolio operations.
#[derive(Debug, thiserror::Error)]
pub enum PortfolioError {
    /// Portfolio not found, or it belongs to a different user.
    #[error("portfolio not found")]
    NotFound,
    /// Underlying repository error (storage failure).
    #[error("repository error: {0}")]
    Repository(#[from] RepositoryError),
}

/// Application service for portfolio CRUD workflows.
#[derive(Clone)]
pub struct PortfolioService {
    repo: Arc<dyn PortfolioRepository>,
}

impl PortfolioService {
    /// Create a new portfolio service.
    pub fn new(repo: Arc<dyn PortfolioRepository>) -> Self {
        Self { repo }
    }

    /// Create a new portfolio owned by `user_id`.
    pub async fn create(
        &self,
        user_id: Uuid,
        name: String,
        description: Option<String>,
    ) -> Result<Portfolio, PortfolioError> {
        self.repo
            .create(&NewPortfolio {
                user_id,
                name,
                description,
                base_currency: "EUR".to_string(),
            })
            .await
            .map_err(Into::into)
    }

    /// Return all portfolios owned by `user_id`, paginated.
    pub async fn list(
        &self,
        user_id: Uuid,
        pagination: &PaginationParams,
    ) -> Result<PaginatedResult<Portfolio>, PortfolioError> {
        self.repo
            .find_by_user_id(user_id, pagination)
            .await
            .map_err(Into::into)
    }

    /// Return a single portfolio, verifying it belongs to `user_id`.
    pub async fn get(
        &self,
        user_id: Uuid,
        portfolio_id: Uuid,
    ) -> Result<Portfolio, PortfolioError> {
        self.repo
            .find_by_id(portfolio_id)
            .await?
            .filter(|portfolio| portfolio.user_id == user_id)
            .ok_or(PortfolioError::NotFound)
    }

    /// Update a portfolio after verifying ownership.
    pub async fn update(
        &self,
        user_id: Uuid,
        portfolio_id: Uuid,
        name: String,
        description: Option<String>,
    ) -> Result<Portfolio, PortfolioError> {
        self.repo
            .find_by_id(portfolio_id)
            .await?
            .filter(|portfolio| portfolio.user_id == user_id)
            .ok_or(PortfolioError::NotFound)?;

        self.repo
            .update(portfolio_id, &name, description.as_deref())
            .await
            .map_err(Into::into)
    }

    /// Delete a portfolio after verifying ownership.
    pub async fn delete(&self, user_id: Uuid, portfolio_id: Uuid) -> Result<(), PortfolioError> {
        self.repo
            .find_by_id(portfolio_id)
            .await?
            .filter(|portfolio| portfolio.user_id == user_id)
            .ok_or(PortfolioError::NotFound)?;

        self.repo.delete(portfolio_id).await.map_err(Into::into)
    }
}
