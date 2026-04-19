//! PostgreSQL implementation of the [`domain::ExchangeRateRepository`] port.

use crate::error::map_sqlx_error;
use chrono::{NaiveDate, Utc};
use domain::{ExchangeRate, ExchangeRateRepository, NewExchangeRate, RepositoryError};
use sqlx::PgPool;
use uuid::Uuid;

/// Internal row type mirroring the `exchange_rates` table.
#[derive(sqlx::FromRow)]
struct ExchangeRateRow {
    id: Uuid,
    from_currency: String,
    to_currency: String,
    date: NaiveDate,
    rate: rust_decimal::Decimal,
}

impl From<ExchangeRateRow> for ExchangeRate {
    fn from(row: ExchangeRateRow) -> Self {
        Self {
            id: row.id,
            from_currency: row.from_currency,
            to_currency: row.to_currency,
            date: row.date,
            rate: row.rate,
        }
    }
}

/// PostgreSQL-backed implementation of [`ExchangeRateRepository`].
pub struct PgExchangeRateRepository {
    pool: PgPool,
}

impl PgExchangeRateRepository {
    /// Create a new repository backed by the given connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl ExchangeRateRepository for PgExchangeRateRepository {
    async fn insert(
        &self,
        new_exchange_rate: &NewExchangeRate,
    ) -> Result<ExchangeRate, RepositoryError> {
        let now = Utc::now();
        let row = sqlx::query_as::<_, ExchangeRateRow>(
            "INSERT INTO exchange_rates (
                id, from_currency, to_currency, date, rate, created_at, updated_at
             )
             VALUES ($1, $2, $3, $4, $5, $6, $6)
             ON CONFLICT (from_currency, to_currency, date)
             DO UPDATE SET
                rate = EXCLUDED.rate,
                updated_at = EXCLUDED.updated_at
             RETURNING id, from_currency, to_currency, date, rate",
        )
        .bind(Uuid::now_v7())
        .bind(&new_exchange_rate.from_currency)
        .bind(&new_exchange_rate.to_currency)
        .bind(new_exchange_rate.date)
        .bind(new_exchange_rate.rate)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(row.into())
    }

    async fn find_latest(
        &self,
        from_currency: &str,
        to_currency: &str,
    ) -> Result<Option<ExchangeRate>, RepositoryError> {
        let row = sqlx::query_as::<_, ExchangeRateRow>(
            "SELECT id, from_currency, to_currency, date, rate
             FROM exchange_rates
             WHERE from_currency = $1 AND to_currency = $2
             ORDER BY date DESC, id DESC
             LIMIT 1",
        )
        .bind(from_currency)
        .bind(to_currency)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(row.map(Into::into))
    }

    async fn find_by_date(
        &self,
        from_currency: &str,
        to_currency: &str,
        date: NaiveDate,
    ) -> Result<Option<ExchangeRate>, RepositoryError> {
        let row = sqlx::query_as::<_, ExchangeRateRow>(
            "SELECT id, from_currency, to_currency, date, rate
             FROM exchange_rates
             WHERE from_currency = $1 AND to_currency = $2 AND date = $3",
        )
        .bind(from_currency)
        .bind(to_currency)
        .bind(date)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(row.map(Into::into))
    }
}
