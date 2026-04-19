//! Request and response DTOs for portfolio endpoints.

use chrono::{DateTime, Utc};
use domain::models::portfolio::Portfolio;
use garde::Validate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Request body for `POST /api/v1/portfolios`.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
pub struct CreatePortfolioRequest {
    /// Human-readable portfolio name (required, non-empty).
    #[garde(length(min = 1))]
    pub name: String,
    /// Optional longer description.
    #[garde(skip)]
    pub description: Option<String>,
}

/// Request body for `PUT /api/v1/portfolios/{id}`.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
pub struct UpdatePortfolioRequest {
    /// New portfolio name (required, non-empty).
    #[garde(length(min = 1))]
    pub name: String,
    /// New description; `null` clears an existing description.
    #[garde(skip)]
    pub description: Option<String>,
}

/// Portfolio returned by the API.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct PortfolioResponse {
    /// Unique portfolio identifier.
    pub id: Uuid,
    /// Owning user.
    pub user_id: Uuid,
    /// Human-readable portfolio name.
    pub name: String,
    /// Optional longer description.
    pub description: Option<String>,
    /// Base currency (EUR for the MVP).
    pub base_currency: String,
    /// When the portfolio was created.
    pub created_at: DateTime<Utc>,
    /// When the portfolio was last updated.
    pub updated_at: DateTime<Utc>,
}

impl From<Portfolio> for PortfolioResponse {
    fn from(p: Portfolio) -> Self {
        Self {
            id: p.id,
            user_id: p.user_id,
            name: p.name,
            description: p.description,
            base_currency: p.base_currency,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}
