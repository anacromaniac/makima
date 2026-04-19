//! Position listing use cases.

use std::sync::Arc;

use domain::{PortfolioRepository, Position, PositionRepository, RepositoryError};
use uuid::Uuid;

/// Errors that can occur during position listing.
#[derive(Debug, thiserror::Error)]
pub enum PositionError {
    /// Portfolio not found, or it belongs to a different user.
    #[error("portfolio not found")]
    NotFound,
    /// Underlying repository error.
    #[error("repository error: {0}")]
    Repository(#[from] RepositoryError),
}

/// Application service for derived portfolio positions.
#[derive(Clone)]
pub struct PositionService {
    portfolio_repo: Arc<dyn PortfolioRepository>,
    position_repo: Arc<dyn PositionRepository>,
}

impl PositionService {
    /// Create a new position service.
    pub fn new(
        portfolio_repo: Arc<dyn PortfolioRepository>,
        position_repo: Arc<dyn PositionRepository>,
    ) -> Self {
        Self {
            portfolio_repo,
            position_repo,
        }
    }

    /// List positions for a portfolio after verifying ownership.
    pub async fn list(
        &self,
        user_id: Uuid,
        portfolio_id: Uuid,
        show_closed: bool,
    ) -> Result<Vec<Position>, PositionError> {
        self.portfolio_repo
            .find_by_id(portfolio_id)
            .await?
            .filter(|portfolio| portfolio.user_id == user_id)
            .ok_or(PositionError::NotFound)?;

        let positions = self.position_repo.list_by_portfolio(portfolio_id).await?;

        Ok(if show_closed {
            positions
        } else {
            positions
                .into_iter()
                .filter(|position| !position.closed)
                .collect()
        })
    }
}
