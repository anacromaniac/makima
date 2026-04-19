//! PostgreSQL implementation of the [`domain::PriceRepository`] port.

use chrono::{NaiveDate, Utc};
use domain::{
    NewPriceRecord, PaginatedResult, PaginationMeta, PaginationParams, PriceRecord,
    PriceRepository, PriceSource, RepositoryError,
};
use sqlx::{Error as SqlxError, PgPool};
use uuid::Uuid;

/// Internal row type mirroring the `price_history` table.
#[derive(sqlx::FromRow)]
struct PriceRow {
    id: Uuid,
    asset_id: Uuid,
    date: NaiveDate,
    close_price: rust_decimal::Decimal,
    currency: String,
    source: String,
}

impl TryFrom<PriceRow> for PriceRecord {
    type Error = RepositoryError;

    fn try_from(row: PriceRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            asset_id: row.asset_id,
            date: row.date,
            close_price: row.close_price,
            currency: row.currency,
            source: parse_price_source(&row.source)?,
        })
    }
}

/// PostgreSQL-backed implementation of [`PriceRepository`].
pub struct PgPriceRepository {
    pool: PgPool,
}

impl PgPriceRepository {
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

fn parse_price_source(value: &str) -> Result<PriceSource, RepositoryError> {
    match value {
        "yahoo" => Ok(PriceSource::Yahoo),
        "manual" => Ok(PriceSource::Manual),
        other => Err(RepositoryError::Internal(format!(
            "invalid price source stored in database: {other}"
        ))),
    }
}

fn price_source_value(source: PriceSource) -> &'static str {
    match source {
        PriceSource::Yahoo => "yahoo",
        PriceSource::Manual => "manual",
    }
}

#[async_trait::async_trait]
impl PriceRepository for PgPriceRepository {
    async fn insert(
        &self,
        new_price_record: &NewPriceRecord,
    ) -> Result<PriceRecord, RepositoryError> {
        let now = Utc::now();
        let row = sqlx::query_as::<_, PriceRow>(
            "INSERT INTO price_history (
                id, asset_id, date, close_price, currency, source, created_at, updated_at
             )
             VALUES ($1, $2, $3, $4, $5, $6, $7, $7)
             ON CONFLICT (asset_id, date)
             DO UPDATE SET
                close_price = EXCLUDED.close_price,
                currency = EXCLUDED.currency,
                source = EXCLUDED.source,
                updated_at = EXCLUDED.updated_at
             RETURNING id, asset_id, date, close_price, currency, source",
        )
        .bind(Uuid::now_v7())
        .bind(new_price_record.asset_id)
        .bind(new_price_record.date)
        .bind(new_price_record.close_price)
        .bind(&new_price_record.currency)
        .bind(price_source_value(new_price_record.source))
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        row.try_into()
    }

    async fn insert_batch(&self, records: &[NewPriceRecord]) -> Result<u64, RepositoryError> {
        let mut inserted = 0_u64;
        for record in records {
            self.insert(record).await?;
            inserted += 1;
        }
        Ok(inserted)
    }

    async fn find_latest(&self, asset_id: Uuid) -> Result<Option<PriceRecord>, RepositoryError> {
        let row = sqlx::query_as::<_, PriceRow>(
            "SELECT id, asset_id, date, close_price, currency, source
             FROM price_history
             WHERE asset_id = $1
             ORDER BY date DESC, id DESC
             LIMIT 1",
        )
        .bind(asset_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        row.map(TryInto::try_into).transpose()
    }

    async fn find_by_range(
        &self,
        asset_id: Uuid,
        from_date: Option<NaiveDate>,
        to_date: Option<NaiveDate>,
        pagination: &PaginationParams,
    ) -> Result<PaginatedResult<PriceRecord>, RepositoryError> {
        let limit = pagination.limit.min(100) as i64;
        let offset = (pagination.page.saturating_sub(1) as i64) * limit;

        let total_items: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM price_history
             WHERE asset_id = $1
               AND ($2::DATE IS NULL OR date >= $2)
               AND ($3::DATE IS NULL OR date <= $3)",
        )
        .bind(asset_id)
        .bind(from_date)
        .bind(to_date)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let rows = sqlx::query_as::<_, PriceRow>(
            "SELECT id, asset_id, date, close_price, currency, source
             FROM price_history
             WHERE asset_id = $1
               AND ($2::DATE IS NULL OR date >= $2)
               AND ($3::DATE IS NULL OR date <= $3)
             ORDER BY date DESC, id DESC
             LIMIT $4 OFFSET $5",
        )
        .bind(asset_id)
        .bind(from_date)
        .bind(to_date)
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
}
