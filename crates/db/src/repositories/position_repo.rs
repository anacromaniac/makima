//! PostgreSQL implementation of the [`domain::PositionRepository`] port.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::Utc;
use domain::{Asset, Position, PositionRepository, RepositoryError, calculate_gain_loss};
use rust_decimal::Decimal;
use sqlx::{Error as SqlxError, PgPool};
use uuid::Uuid;

/// Internal row type containing aggregated position state and asset metadata.
#[derive(sqlx::FromRow)]
struct PositionRow {
    asset_id: Uuid,
    isin: String,
    yahoo_ticker: Option<String>,
    name: String,
    asset_class: String,
    currency: String,
    exchange: Option<String>,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
    quantity: Decimal,
    average_cost: Decimal,
}

/// Internal row type for the latest available close price per asset.
#[derive(sqlx::FromRow)]
struct LatestPriceRow {
    asset_id: Uuid,
    close_price: Decimal,
}

/// PostgreSQL-backed implementation of [`PositionRepository`].
pub struct PgPositionRepository {
    pool: PgPool,
}

impl PgPositionRepository {
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

impl TryFrom<PositionRow> for Position {
    type Error = RepositoryError;

    fn try_from(row: PositionRow) -> Result<Self, Self::Error> {
        Ok(Self {
            asset: Asset {
                id: row.asset_id,
                isin: row.isin,
                yahoo_ticker: row.yahoo_ticker,
                name: row.name,
                asset_class: row.asset_class.parse().map_err(RepositoryError::Internal)?,
                currency: row.currency,
                exchange: row.exchange,
                created_at: row.created_at,
                updated_at: row.updated_at,
            },
            quantity: row.quantity,
            average_cost: row.average_cost,
            current_price: None,
            current_value: None,
            gain_loss_absolute: None,
            gain_loss_percentage: None,
            closed: row.quantity == Decimal::ZERO,
        })
    }
}

#[async_trait]
impl PositionRepository for PgPositionRepository {
    async fn list_by_portfolio(
        &self,
        portfolio_id: Uuid,
    ) -> Result<Vec<Position>, RepositoryError> {
        let rows = sqlx::query_as::<_, PositionRow>(
            "WITH RECURSIVE ordered_transactions AS (
                SELECT
                    t.asset_id,
                    t.transaction_type::text AS transaction_type,
                    COALESCE(t.quantity, 0) AS quantity,
                    COALESCE(t.unit_price, 0) AS unit_price,
                    ROW_NUMBER() OVER (
                        PARTITION BY t.asset_id
                        ORDER BY t.date ASC, t.created_at ASC, t.id ASC
                    ) AS rn
                FROM transactions t
                WHERE t.portfolio_id = $1
                  AND t.transaction_type IN ('Buy', 'Sell')
            ),
            position_steps AS (
                SELECT
                    ot.asset_id,
                    ot.rn,
                    CASE
                        WHEN ot.transaction_type = 'Buy' THEN ot.quantity
                        ELSE -ot.quantity
                    END AS quantity,
                    CASE
                        WHEN ot.transaction_type = 'Buy' THEN ot.quantity * ot.unit_price
                        ELSE 0::NUMERIC
                    END AS total_cost
                FROM ordered_transactions ot
                WHERE ot.rn = 1

                UNION ALL

                SELECT
                    ot.asset_id,
                    ot.rn,
                    CASE
                        WHEN ot.transaction_type = 'Buy' THEN ps.quantity + ot.quantity
                        ELSE ps.quantity - ot.quantity
                    END AS quantity,
                    CASE
                        WHEN ot.transaction_type = 'Buy' THEN ps.total_cost + (ot.quantity * ot.unit_price)
                        WHEN ps.quantity = 0 THEN 0::NUMERIC
                        ELSE ps.total_cost - (ot.quantity * (ps.total_cost / ps.quantity))
                    END AS total_cost
                FROM ordered_transactions ot
                INNER JOIN position_steps ps
                    ON ps.asset_id = ot.asset_id
                   AND ps.rn + 1 = ot.rn
            ),
            final_positions AS (
                SELECT DISTINCT ON (ps.asset_id)
                    ps.asset_id,
                    ps.quantity,
                    CASE
                        WHEN ps.quantity > 0 THEN ps.total_cost / ps.quantity
                        ELSE 0::NUMERIC
                    END AS average_cost
                FROM position_steps ps
                ORDER BY ps.asset_id, ps.rn DESC
            )
            SELECT
                a.id AS asset_id,
                a.isin,
                a.yahoo_ticker,
                a.name,
                a.asset_class::text AS asset_class,
                a.currency,
                a.exchange,
                a.created_at,
                a.updated_at,
                fp.quantity,
                fp.average_cost
            FROM final_positions fp
            INNER JOIN assets a ON a.id = fp.asset_id
            ORDER BY a.name ASC, a.created_at ASC, a.id ASC",
        )
        .bind(portfolio_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let mut positions = rows
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<Position>, _>>()?;

        if positions.is_empty() {
            return Ok(positions);
        }

        let latest_prices = fetch_latest_prices(&self.pool, &positions).await?;
        for position in &mut positions {
            if let Some(current_price) = latest_prices.get(&position.asset.id).copied() {
                position.current_price = Some(current_price);
                position.current_value = Some(position.quantity * current_price);

                if let Ok(gain_loss) =
                    calculate_gain_loss(position.quantity, position.average_cost, current_price)
                {
                    position.gain_loss_absolute = Some(gain_loss.absolute);
                    position.gain_loss_percentage = Some(gain_loss.percentage);
                }
            }
        }

        Ok(positions)
    }
}

async fn fetch_latest_prices(
    pool: &PgPool,
    positions: &[Position],
) -> Result<HashMap<Uuid, Decimal>, RepositoryError> {
    let price_history_exists: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('public.price_history')::text")
            .fetch_one(pool)
            .await
            .map_err(map_sqlx_error)?;

    if price_history_exists.is_none() {
        return Ok(HashMap::new());
    }

    let asset_ids = positions
        .iter()
        .map(|position| position.asset.id)
        .collect::<Vec<_>>();
    let rows = sqlx::query_as::<_, LatestPriceRow>(
        "SELECT DISTINCT ON (asset_id)
            asset_id,
            close_price
         FROM price_history
         WHERE asset_id = ANY($1)
         ORDER BY asset_id, date DESC, id DESC",
    )
    .bind(asset_ids)
    .fetch_all(pool)
    .await
    .map_err(map_sqlx_error)?;

    Ok(rows
        .into_iter()
        .map(|row| (row.asset_id, row.close_price))
        .collect())
}
