//! PostgreSQL implementation of the [`domain::PortfolioRepository`] port.

use async_trait::async_trait;
use chrono::Utc;
use domain::{
    NewPortfolio, PaginatedResult, PaginationMeta, PaginationParams, Portfolio,
    PortfolioRepository, RepositoryError,
};
use sqlx::PgPool;
use uuid::Uuid;

/// Internal row type mirroring the `portfolios` table. Implements [`sqlx::FromRow`]
/// without introducing sqlx as a dependency of the domain crate.
#[derive(sqlx::FromRow)]
struct PortfolioRow {
    id: Uuid,
    user_id: Uuid,
    name: String,
    description: Option<String>,
    base_currency: String,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
}

impl From<PortfolioRow> for Portfolio {
    fn from(row: PortfolioRow) -> Self {
        Portfolio {
            id: row.id,
            user_id: row.user_id,
            name: row.name,
            description: row.description,
            base_currency: row.base_currency,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

/// PostgreSQL-backed implementation of [`PortfolioRepository`].
pub struct PgPortfolioRepository {
    pool: PgPool,
}

impl PgPortfolioRepository {
    /// Create a new repository backed by the given connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PortfolioRepository for PgPortfolioRepository {
    async fn create(&self, new_portfolio: &NewPortfolio) -> Result<Portfolio, RepositoryError> {
        let id = Uuid::now_v7();
        let now = Utc::now();
        sqlx::query_as::<_, PortfolioRow>(
            "INSERT INTO portfolios (id, user_id, name, description, base_currency, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $6)
             RETURNING id, user_id, name, description, base_currency, created_at, updated_at",
        )
        .bind(id)
        .bind(new_portfolio.user_id)
        .bind(&new_portfolio.name)
        .bind(&new_portfolio.description)
        .bind(&new_portfolio.base_currency)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map(Into::into)
        .map_err(|e| RepositoryError::Internal(e.to_string()))
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Portfolio>, RepositoryError> {
        sqlx::query_as::<_, PortfolioRow>(
            "SELECT id, user_id, name, description, base_currency, created_at, updated_at
             FROM portfolios WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map(|opt| opt.map(Into::into))
        .map_err(|e| RepositoryError::Internal(e.to_string()))
    }

    async fn find_by_user_id(
        &self,
        user_id: Uuid,
        pagination: &PaginationParams,
    ) -> Result<PaginatedResult<Portfolio>, RepositoryError> {
        let limit = pagination.limit.min(100) as i64;
        let offset = ((pagination.page.saturating_sub(1)) as i64) * limit;

        let total_items: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM portfolios WHERE user_id = $1")
                .bind(user_id)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| RepositoryError::Internal(e.to_string()))?;

        let rows = sqlx::query_as::<_, PortfolioRow>(
            "SELECT id, user_id, name, description, base_currency, created_at, updated_at
             FROM portfolios WHERE user_id = $1
             ORDER BY created_at ASC
             LIMIT $2 OFFSET $3",
        )
        .bind(user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Internal(e.to_string()))?;

        let total_items = total_items as u64;
        let total_pages = ((total_items as f64) / (pagination.limit as f64)).ceil() as u32;
        let total_pages = total_pages.max(1);

        Ok(PaginatedResult {
            data: rows.into_iter().map(Into::into).collect(),
            pagination: PaginationMeta {
                page: pagination.page,
                limit: pagination.limit,
                total_items,
                total_pages,
            },
        })
    }

    async fn update(
        &self,
        id: Uuid,
        name: &str,
        description: Option<&str>,
    ) -> Result<Portfolio, RepositoryError> {
        let now = Utc::now();
        sqlx::query_as::<_, PortfolioRow>(
            "UPDATE portfolios
             SET name = $1, description = $2, updated_at = $3
             WHERE id = $4
             RETURNING id, user_id, name, description, base_currency, created_at, updated_at",
        )
        .bind(name)
        .bind(description)
        .bind(now)
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map(Into::into)
        .map_err(|e| RepositoryError::Internal(e.to_string()))
    }

    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError> {
        sqlx::query("DELETE FROM portfolios WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(|e| RepositoryError::Internal(e.to_string()))
    }
}
