//! Portfolio domain model.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A named collection of investment positions owned by a user.
///
/// Each user can have multiple portfolios (e.g. "Fineco Account",
/// "Long-term Bonds"). Deleting a portfolio cascades to its transactions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Portfolio {
    /// Unique identifier (UUID v7).
    pub id: Uuid,
    /// Owning user.
    pub user_id: Uuid,
    /// Human-readable portfolio name.
    pub name: String,
    /// Optional longer description.
    pub description: Option<String>,
    /// Base currency for the portfolio. Fixed to EUR for the MVP.
    pub base_currency: String,
    /// When the portfolio was created.
    pub created_at: DateTime<Utc>,
    /// When the portfolio was last updated.
    pub updated_at: DateTime<Utc>,
}

/// Data needed to create a new portfolio.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewPortfolio {
    /// Owning user.
    pub user_id: Uuid,
    /// Human-readable portfolio name.
    pub name: String,
    /// Optional longer description.
    pub description: Option<String>,
    /// Base currency (defaults to EUR).
    pub base_currency: String,
}
