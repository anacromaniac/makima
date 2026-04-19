//! PostgreSQL implementation of the [`domain::TransactionRepository`] port.

use async_trait::async_trait;
use chrono::{NaiveDate, Utc};
use domain::{
    NewTransaction, PaginatedResult, PaginationMeta, PaginationParams, RepositoryError,
    Transaction, TransactionFilters, TransactionRepository, TransactionType, UpdateTransaction,
};
use rust_decimal::Decimal;
use sqlx::{Error as SqlxError, PgPool};
use uuid::Uuid;

/// Internal row type mirroring the `transactions` table.
#[derive(sqlx::FromRow)]
struct TransactionRow {
    id: Uuid,
    portfolio_id: Uuid,
    asset_id: Uuid,
    transaction_type: String,
    date: NaiveDate,
    settlement_date: Option<NaiveDate>,
    quantity: Option<Decimal>,
    unit_price: Option<Decimal>,
    commission: Decimal,
    currency: String,
    exchange_rate_to_base: Decimal,
    gross_amount: Option<Decimal>,
    tax_withheld: Option<Decimal>,
    net_amount: Option<Decimal>,
    notes: Option<String>,
    import_hash: Option<String>,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
}

impl TryFrom<TransactionRow> for Transaction {
    type Error = RepositoryError;

    fn try_from(row: TransactionRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            portfolio_id: row.portfolio_id,
            asset_id: row.asset_id,
            transaction_type: parse_transaction_type(&row.transaction_type)?,
            date: row.date,
            settlement_date: row.settlement_date,
            quantity: row.quantity,
            unit_price: row.unit_price,
            commission: row.commission,
            currency: row.currency,
            exchange_rate_to_base: row.exchange_rate_to_base,
            gross_amount: row.gross_amount,
            tax_withheld: row.tax_withheld,
            net_amount: row.net_amount,
            notes: row.notes,
            import_hash: row.import_hash,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

/// PostgreSQL-backed implementation of [`TransactionRepository`].
pub struct PgTransactionRepository {
    pool: PgPool,
}

impl PgTransactionRepository {
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

fn parse_transaction_type(value: &str) -> Result<TransactionType, RepositoryError> {
    match value {
        "Buy" => Ok(TransactionType::Buy),
        "Sell" => Ok(TransactionType::Sell),
        "Dividend" => Ok(TransactionType::Dividend),
        "Coupon" => Ok(TransactionType::Coupon),
        other => Err(RepositoryError::Internal(format!(
            "unsupported transaction type: {other}"
        ))),
    }
}

#[async_trait]
impl TransactionRepository for PgTransactionRepository {
    async fn create(
        &self,
        new_transaction: &NewTransaction,
    ) -> Result<Transaction, RepositoryError> {
        let id = Uuid::now_v7();
        let now = Utc::now();
        let mut tx = self.pool.begin().await.map_err(map_sqlx_error)?;

        let row = sqlx::query_as::<_, TransactionRow>(
            "INSERT INTO transactions (
                id, portfolio_id, asset_id, transaction_type, date, settlement_date, quantity,
                unit_price, commission, currency, exchange_rate_to_base, gross_amount,
                tax_withheld, net_amount, notes, import_hash, created_at, updated_at
            )
            VALUES (
                $1, $2, $3, $4::transaction_type, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
                $15, $16, $17, $17
            )
            RETURNING
                id, portfolio_id, asset_id, transaction_type::text AS transaction_type, date,
                settlement_date, quantity, unit_price, commission, currency,
                exchange_rate_to_base, gross_amount, tax_withheld, net_amount, notes, import_hash,
                created_at, updated_at",
        )
        .bind(id)
        .bind(new_transaction.portfolio_id)
        .bind(new_transaction.asset_id)
        .bind(transaction_type_as_str(new_transaction.transaction_type))
        .bind(new_transaction.date)
        .bind(new_transaction.settlement_date)
        .bind(new_transaction.quantity)
        .bind(new_transaction.unit_price)
        .bind(new_transaction.commission)
        .bind(&new_transaction.currency)
        .bind(new_transaction.exchange_rate_to_base)
        .bind(new_transaction.gross_amount)
        .bind(new_transaction.tax_withheld)
        .bind(new_transaction.net_amount)
        .bind(&new_transaction.notes)
        .bind(&new_transaction.import_hash)
        .bind(now)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;

        tx.commit().await.map_err(map_sqlx_error)?;
        row.try_into()
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Transaction>, RepositoryError> {
        let row = sqlx::query_as::<_, TransactionRow>(
            "SELECT
                id, portfolio_id, asset_id, transaction_type::text AS transaction_type, date,
                settlement_date, quantity, unit_price, commission, currency,
                exchange_rate_to_base, gross_amount, tax_withheld, net_amount, notes, import_hash,
                created_at, updated_at
             FROM transactions
             WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        row.map(TryInto::try_into).transpose()
    }

    async fn find_by_portfolio(
        &self,
        portfolio_id: Uuid,
        pagination: &PaginationParams,
        filters: &TransactionFilters,
    ) -> Result<PaginatedResult<Transaction>, RepositoryError> {
        let limit = pagination.limit.min(100) as i64;
        let offset = ((pagination.page.saturating_sub(1)) as i64) * limit;
        let transaction_type = filters.transaction_type.map(transaction_type_as_str);

        let total_items: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM transactions
             WHERE portfolio_id = $1
               AND ($2::transaction_type IS NULL OR transaction_type = $2::transaction_type)
               AND ($3::UUID IS NULL OR asset_id = $3)
               AND ($4::DATE IS NULL OR date >= $4)
               AND ($5::DATE IS NULL OR date <= $5)",
        )
        .bind(portfolio_id)
        .bind(transaction_type)
        .bind(filters.asset_id)
        .bind(filters.date_from)
        .bind(filters.date_to)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let rows = sqlx::query_as::<_, TransactionRow>(
            "SELECT
                id, portfolio_id, asset_id, transaction_type::text AS transaction_type, date,
                settlement_date, quantity, unit_price, commission, currency,
                exchange_rate_to_base, gross_amount, tax_withheld, net_amount, notes, import_hash,
                created_at, updated_at
             FROM transactions
             WHERE portfolio_id = $1
               AND ($2::transaction_type IS NULL OR transaction_type = $2::transaction_type)
               AND ($3::UUID IS NULL OR asset_id = $3)
               AND ($4::DATE IS NULL OR date >= $4)
               AND ($5::DATE IS NULL OR date <= $5)
             ORDER BY date DESC, created_at DESC, id DESC
             LIMIT $6 OFFSET $7",
        )
        .bind(portfolio_id)
        .bind(transaction_type)
        .bind(filters.asset_id)
        .bind(filters.date_from)
        .bind(filters.date_to)
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

    async fn list_by_asset(
        &self,
        portfolio_id: Uuid,
        asset_id: Uuid,
    ) -> Result<Vec<Transaction>, RepositoryError> {
        let rows = sqlx::query_as::<_, TransactionRow>(
            "SELECT
                id, portfolio_id, asset_id, transaction_type::text AS transaction_type, date,
                settlement_date, quantity, unit_price, commission, currency,
                exchange_rate_to_base, gross_amount, tax_withheld, net_amount, notes, import_hash,
                created_at, updated_at
             FROM transactions
             WHERE portfolio_id = $1 AND asset_id = $2
             ORDER BY date ASC, created_at ASC, id ASC",
        )
        .bind(portfolio_id)
        .bind(asset_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn update(
        &self,
        id: Uuid,
        update: &UpdateTransaction,
    ) -> Result<Transaction, RepositoryError> {
        let now = Utc::now();
        let mut tx = self.pool.begin().await.map_err(map_sqlx_error)?;

        let row = sqlx::query_as::<_, TransactionRow>(
            "UPDATE transactions
             SET asset_id = $1,
                 transaction_type = $2::transaction_type,
                 date = $3,
                 settlement_date = $4,
                 quantity = $5,
                 unit_price = $6,
                 commission = $7,
                 currency = $8,
                 exchange_rate_to_base = $9,
                 gross_amount = $10,
                 tax_withheld = $11,
                 net_amount = $12,
                 notes = $13,
                 updated_at = $14
             WHERE id = $15
             RETURNING
                id, portfolio_id, asset_id, transaction_type::text AS transaction_type, date,
                settlement_date, quantity, unit_price, commission, currency,
                exchange_rate_to_base, gross_amount, tax_withheld, net_amount, notes, import_hash,
                created_at, updated_at",
        )
        .bind(update.asset_id)
        .bind(transaction_type_as_str(update.transaction_type))
        .bind(update.date)
        .bind(update.settlement_date)
        .bind(update.quantity)
        .bind(update.unit_price)
        .bind(update.commission)
        .bind(&update.currency)
        .bind(update.exchange_rate_to_base)
        .bind(update.gross_amount)
        .bind(update.tax_withheld)
        .bind(update.net_amount)
        .bind(&update.notes)
        .bind(now)
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;

        tx.commit().await.map_err(map_sqlx_error)?;
        row.try_into()
    }

    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx_error)?;

        sqlx::query("DELETE FROM transactions WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx_error)?;

        tx.commit().await.map_err(map_sqlx_error)?;
        Ok(())
    }

    async fn get_held_quantity(
        &self,
        portfolio_id: Uuid,
        asset_id: Uuid,
    ) -> Result<Decimal, RepositoryError> {
        let quantity = sqlx::query_scalar::<_, Option<Decimal>>(
            "SELECT COALESCE(SUM(
                CASE
                    WHEN transaction_type = 'Buy'::transaction_type THEN COALESCE(quantity, 0)
                    WHEN transaction_type = 'Sell'::transaction_type THEN -COALESCE(quantity, 0)
                    ELSE 0
                END
             ), 0)
             FROM transactions
             WHERE portfolio_id = $1 AND asset_id = $2",
        )
        .bind(portfolio_id)
        .bind(asset_id)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(quantity.unwrap_or(Decimal::ZERO))
    }
}

fn transaction_type_as_str(value: TransactionType) -> &'static str {
    match value {
        TransactionType::Buy => "Buy",
        TransactionType::Sell => "Sell",
        TransactionType::Dividend => "Dividend",
        TransactionType::Coupon => "Coupon",
    }
}
