//! PostgreSQL implementation of the [`domain::AssetRepository`] port.

use async_trait::async_trait;
use chrono::Utc;
use domain::{
    Asset, AssetClass, AssetFilters, AssetRepository, NewAsset, PaginatedResult, PaginationMeta,
    PaginationParams, RepositoryError, UpdateAsset,
};
use sqlx::{Error as SqlxError, PgPool};
use uuid::Uuid;

/// Internal row type mirroring the `assets` table.
#[derive(sqlx::FromRow)]
struct AssetRow {
    id: Uuid,
    isin: String,
    yahoo_ticker: Option<String>,
    name: String,
    asset_class: String,
    currency: String,
    exchange: Option<String>,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
}

impl TryFrom<AssetRow> for Asset {
    type Error = RepositoryError;

    fn try_from(row: AssetRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            isin: row.isin,
            yahoo_ticker: row.yahoo_ticker,
            name: row.name,
            asset_class: row.asset_class.parse().map_err(RepositoryError::Internal)?,
            currency: row.currency,
            exchange: row.exchange,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

/// PostgreSQL-backed implementation of [`AssetRepository`].
pub struct PgAssetRepository {
    pool: PgPool,
}

impl PgAssetRepository {
    /// Create a new repository backed by the given connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn map_sqlx_error(error: SqlxError) -> RepositoryError {
    match error {
        SqlxError::Database(db_error) if db_error.is_unique_violation() => {
            RepositoryError::Conflict(db_error.message().to_string())
        }
        other => RepositoryError::Internal(other.to_string()),
    }
}

#[async_trait]
impl AssetRepository for PgAssetRepository {
    async fn create(&self, new_asset: &NewAsset) -> Result<Asset, RepositoryError> {
        let id = Uuid::now_v7();
        let now = Utc::now();

        let row = sqlx::query_as::<_, AssetRow>(
            "INSERT INTO assets (
                id, isin, yahoo_ticker, name, asset_class, currency, exchange, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5::asset_class, $6, $7, $8, $8)
            RETURNING id, isin, yahoo_ticker, name, asset_class::text AS asset_class, currency, exchange, created_at, updated_at",
        )
        .bind(id)
        .bind(&new_asset.isin)
        .bind(&new_asset.yahoo_ticker)
        .bind(&new_asset.name)
        .bind(new_asset.asset_class.as_str())
        .bind(&new_asset.currency)
        .bind(&new_asset.exchange)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        row.try_into()
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Asset>, RepositoryError> {
        let row = sqlx::query_as::<_, AssetRow>(
            "SELECT id, isin, yahoo_ticker, name, asset_class::text AS asset_class, currency, exchange, created_at, updated_at
             FROM assets
             WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        row.map(TryInto::try_into).transpose()
    }

    async fn find_by_isin(&self, isin: &str) -> Result<Option<Asset>, RepositoryError> {
        let row = sqlx::query_as::<_, AssetRow>(
            "SELECT id, isin, yahoo_ticker, name, asset_class::text AS asset_class, currency, exchange, created_at, updated_at
             FROM assets
             WHERE isin = $1",
        )
        .bind(isin)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        row.map(TryInto::try_into).transpose()
    }

    async fn list(
        &self,
        pagination: &PaginationParams,
        filters: &AssetFilters,
    ) -> Result<PaginatedResult<Asset>, RepositoryError> {
        let limit = pagination.limit.min(100) as i64;
        let offset = ((pagination.page.saturating_sub(1)) as i64) * limit;
        let asset_class = filters.asset_class.map(AssetClass::as_str);
        let name_search = filters
            .name_search
            .as_ref()
            .map(|value| format!("%{value}%"));

        let total_items: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM assets
             WHERE ($1::asset_class IS NULL OR asset_class = $1::asset_class)
               AND ($2::VARCHAR IS NULL OR name ILIKE $2)",
        )
        .bind(asset_class)
        .bind(&name_search)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let rows = sqlx::query_as::<_, AssetRow>(
            "SELECT id, isin, yahoo_ticker, name, asset_class::text AS asset_class, currency, exchange, created_at, updated_at
             FROM assets
             WHERE ($1::asset_class IS NULL OR asset_class = $1::asset_class)
               AND ($2::VARCHAR IS NULL OR name ILIKE $2)
             ORDER BY name ASC, created_at ASC
             LIMIT $3 OFFSET $4",
        )
        .bind(asset_class)
        .bind(&name_search)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let total_items = total_items as u64;
        let total_pages = ((total_items as f64) / (pagination.limit as f64)).ceil() as u32;
        let total_pages = total_pages.max(1);

        Ok(PaginatedResult {
            data: rows
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()?,
            pagination: PaginationMeta {
                page: pagination.page,
                limit: pagination.limit,
                total_items,
                total_pages,
            },
        })
    }

    async fn list_in_use(&self) -> Result<Vec<Asset>, RepositoryError> {
        let rows = sqlx::query_as::<_, AssetRow>(
            "SELECT DISTINCT
                a.id,
                a.isin,
                a.yahoo_ticker,
                a.name,
                a.asset_class::text AS asset_class,
                a.currency,
                a.exchange,
                a.created_at,
                a.updated_at
             FROM assets a
             INNER JOIN transactions t ON t.asset_id = a.id
             ORDER BY a.name ASC, a.created_at ASC, a.id ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        rows.into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn update(&self, id: Uuid, update: &UpdateAsset) -> Result<Asset, RepositoryError> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, AssetRow>(
            "UPDATE assets
             SET yahoo_ticker = $1,
                 name = $2,
                 asset_class = $3::asset_class,
                 currency = $4,
                 exchange = $5,
                 updated_at = $6
             WHERE id = $7
             RETURNING id, isin, yahoo_ticker, name, asset_class::text AS asset_class, currency, exchange, created_at, updated_at",
        )
        .bind(&update.yahoo_ticker)
        .bind(&update.name)
        .bind(update.asset_class.as_str())
        .bind(&update.currency)
        .bind(&update.exchange)
        .bind(now)
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        row.try_into()
    }

    async fn update_yahoo_ticker(
        &self,
        id: Uuid,
        yahoo_ticker: Option<&str>,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            "UPDATE assets
             SET yahoo_ticker = $1, updated_at = $2
             WHERE id = $3",
        )
        .bind(yahoo_ticker)
        .bind(Utc::now())
        .bind(id)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(map_sqlx_error)
    }
}
