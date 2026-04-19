//! Atomic persistence adapter for broker imports.

use async_trait::async_trait;
use chrono::Utc;
use domain::{
    BrokerImportRepository, PreparedImportAsset, PreparedImportTransaction, RepositoryError,
};
use sqlx::PgPool;

use crate::error::map_sqlx_error;

/// PostgreSQL-backed broker import repository.
pub struct PgBrokerImportRepository {
    pool: PgPool,
}

impl PgBrokerImportRepository {
    /// Create a new repository backed by the given connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BrokerImportRepository for PgBrokerImportRepository {
    async fn find_existing_import_hashes(
        &self,
        portfolio_id: uuid::Uuid,
        import_hashes: &[String],
    ) -> Result<Vec<String>, RepositoryError> {
        if import_hashes.is_empty() {
            return Ok(Vec::new());
        }

        sqlx::query_scalar::<_, String>(
            "SELECT import_hash
             FROM transactions
             WHERE portfolio_id = $1
               AND import_hash = ANY($2)
               AND import_hash IS NOT NULL",
        )
        .bind(portfolio_id)
        .bind(import_hashes)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)
    }

    async fn import_batch(
        &self,
        assets: &[PreparedImportAsset],
        transactions: &[PreparedImportTransaction],
    ) -> Result<(), RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx_error)?;

        for asset in assets {
            let now = Utc::now();
            sqlx::query(
                "INSERT INTO assets (
                    id, isin, yahoo_ticker, name, asset_class, currency, exchange, created_at, updated_at
                )
                VALUES ($1, $2, $3, $4, $5::asset_class, $6, $7, $8, $8)",
            )
            .bind(asset.id)
            .bind(&asset.asset.isin)
            .bind(&asset.asset.yahoo_ticker)
            .bind(&asset.asset.name)
            .bind(asset.asset.asset_class.as_str())
            .bind(&asset.asset.currency)
            .bind(&asset.asset.exchange)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx_error)?;
        }

        for prepared in transactions {
            let transaction = &prepared.transaction;
            let now = Utc::now();
            sqlx::query(
                "INSERT INTO transactions (
                    id, portfolio_id, asset_id, transaction_type, date, settlement_date, quantity,
                    unit_price, commission, currency, exchange_rate_to_base, gross_amount,
                    tax_withheld, net_amount, notes, import_hash, created_at, updated_at
                )
                VALUES (
                    $1, $2, $3, $4::transaction_type, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                    $14, $15, $16, $17, $17
                )",
            )
            .bind(uuid::Uuid::now_v7())
            .bind(transaction.portfolio_id)
            .bind(transaction.asset_id)
            .bind(match transaction.transaction_type {
                domain::TransactionType::Buy => "Buy",
                domain::TransactionType::Sell => "Sell",
                domain::TransactionType::Dividend => "Dividend",
                domain::TransactionType::Coupon => "Coupon",
            })
            .bind(transaction.date)
            .bind(transaction.settlement_date)
            .bind(transaction.quantity)
            .bind(transaction.unit_price)
            .bind(transaction.commission)
            .bind(&transaction.currency)
            .bind(transaction.exchange_rate_to_base)
            .bind(transaction.gross_amount)
            .bind(transaction.tax_withheld)
            .bind(transaction.net_amount)
            .bind(&transaction.notes)
            .bind(&transaction.import_hash)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx_error)?;
        }

        tx.commit().await.map_err(map_sqlx_error)
    }
}
