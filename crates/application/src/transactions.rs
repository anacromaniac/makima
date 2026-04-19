//! Transaction use cases and external lookup ports.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::NaiveDate;
use domain::{
    Asset, AssetClass, AssetRepository, DomainError, NewAsset, NewTransaction, PaginatedResult,
    PaginationParams, PortfolioRepository, RepositoryError, Transaction, TransactionFilters,
    TransactionRepository, TransactionType, UpdateTransaction, aggregate_position,
};
use rust_decimal::Decimal;
use uuid::Uuid;

/// Asset metadata resolved from an external reference-data source such as OpenFIGI.
#[derive(Debug, Clone)]
pub struct ResolvedAssetMetadata {
    /// International Securities Identification Number.
    pub isin: String,
    /// Yahoo Finance ticker symbol, if available.
    pub yahoo_ticker: Option<String>,
    /// Human-readable asset name.
    pub name: String,
    /// Asset classification.
    pub asset_class: AssetClass,
    /// Quotation currency.
    pub currency: String,
    /// Exchange where the asset is listed, if known.
    pub exchange: Option<String>,
}

/// Transaction plus the shared asset fields needed by the API.
#[derive(Debug, Clone)]
pub struct TransactionDetails {
    /// Stored transaction record.
    pub transaction: Transaction,
    /// Asset ISIN.
    pub asset_isin: String,
    /// Asset name.
    pub asset_name: String,
}

/// Data required to create a transaction from the API layer.
#[derive(Debug, Clone)]
pub struct CreateTransactionInput {
    /// Owning portfolio.
    pub portfolio_id: Uuid,
    /// Asset ISIN to resolve or auto-create.
    pub asset_isin: String,
    /// Transaction kind.
    pub transaction_type: TransactionType,
    /// Trade date.
    pub date: NaiveDate,
    /// Settlement date.
    pub settlement_date: Option<NaiveDate>,
    /// Quantity for buy/sell operations.
    pub quantity: Option<Decimal>,
    /// Unit price for buy/sell operations.
    pub unit_price: Option<Decimal>,
    /// Brokerage commission.
    pub commission: Decimal,
    /// Transaction currency.
    pub currency: String,
    /// Optional caller-provided rate to EUR.
    pub exchange_rate_to_base: Option<Decimal>,
    /// Gross distribution amount for dividend/coupon operations.
    pub gross_amount: Option<Decimal>,
    /// Withheld tax for dividend/coupon operations.
    pub tax_withheld: Option<Decimal>,
    /// Net distribution amount for dividend/coupon operations.
    pub net_amount: Option<Decimal>,
    /// Optional notes.
    pub notes: Option<String>,
}

/// Data required to update a transaction from the API layer.
#[derive(Debug, Clone)]
pub struct UpdateTransactionInput {
    /// Asset ISIN to resolve or auto-create.
    pub asset_isin: String,
    /// Transaction kind.
    pub transaction_type: TransactionType,
    /// Trade date.
    pub date: NaiveDate,
    /// Settlement date.
    pub settlement_date: Option<NaiveDate>,
    /// Quantity for buy/sell operations.
    pub quantity: Option<Decimal>,
    /// Unit price for buy/sell operations.
    pub unit_price: Option<Decimal>,
    /// Brokerage commission.
    pub commission: Decimal,
    /// Transaction currency.
    pub currency: String,
    /// Optional caller-provided rate to EUR.
    pub exchange_rate_to_base: Option<Decimal>,
    /// Gross distribution amount for dividend/coupon operations.
    pub gross_amount: Option<Decimal>,
    /// Withheld tax for dividend/coupon operations.
    pub tax_withheld: Option<Decimal>,
    /// Net distribution amount for dividend/coupon operations.
    pub net_amount: Option<Decimal>,
    /// Optional notes.
    pub notes: Option<String>,
}

/// External lookup used to resolve an unknown ISIN into full asset metadata.
#[async_trait]
pub trait AssetMetadataLookup: Send + Sync {
    /// Resolve the provided ISIN into metadata suitable for asset creation.
    async fn lookup_asset_metadata(&self, isin: &str) -> Option<ResolvedAssetMetadata>;
}

/// External lookup used to resolve the FX rate from a transaction currency to EUR.
#[async_trait]
pub trait ExchangeRateLookup: Send + Sync {
    /// Resolve the latest available rate from `currency` to EUR.
    async fn lookup_rate_to_eur(&self, currency: &str) -> Option<Decimal>;
}

/// Errors that can occur during transaction workflows.
#[derive(Debug, thiserror::Error)]
pub enum TransactionError {
    /// Transaction not found, or it belongs to a different user.
    #[error("transaction not found")]
    NotFound,
    /// Portfolio not found, or it belongs to a different user.
    #[error("portfolio not found")]
    PortfolioNotFound,
    /// The requested asset could not be resolved from reference data.
    #[error("unable to resolve asset metadata for ISIN {0}")]
    AssetResolutionFailed(String),
    /// The transaction currency requires a rate but none could be resolved.
    #[error("exchange rate to EUR is required for currency {0}")]
    ExchangeRateRequired(String),
    /// A sell would result in a negative quantity.
    #[error("insufficient quantity: available {available}, requested {requested}")]
    InsufficientQuantity {
        /// Quantity currently available.
        available: Decimal,
        /// Quantity requested by the failing sell.
        requested: Decimal,
    },
    /// Repository failure.
    #[error("repository error: {0}")]
    Repository(#[from] RepositoryError),
}

/// Application service for transaction CRUD and business-rule enforcement.
#[derive(Clone)]
pub struct TransactionService {
    portfolio_repo: Arc<dyn PortfolioRepository>,
    asset_repo: Arc<dyn AssetRepository>,
    transaction_repo: Arc<dyn TransactionRepository>,
    asset_metadata_lookup: Arc<dyn AssetMetadataLookup>,
    exchange_rate_lookup: Arc<dyn ExchangeRateLookup>,
}

impl TransactionService {
    /// Create a new transaction service.
    pub fn new(
        portfolio_repo: Arc<dyn PortfolioRepository>,
        asset_repo: Arc<dyn AssetRepository>,
        transaction_repo: Arc<dyn TransactionRepository>,
        asset_metadata_lookup: Arc<dyn AssetMetadataLookup>,
        exchange_rate_lookup: Arc<dyn ExchangeRateLookup>,
    ) -> Self {
        Self {
            portfolio_repo,
            asset_repo,
            transaction_repo,
            asset_metadata_lookup,
            exchange_rate_lookup,
        }
    }

    /// List transactions in a portfolio after verifying ownership.
    pub async fn list(
        &self,
        user_id: Uuid,
        portfolio_id: Uuid,
        pagination: &PaginationParams,
        filters: &TransactionFilters,
    ) -> Result<PaginatedResult<TransactionDetails>, TransactionError> {
        self.ensure_portfolio_ownership(user_id, portfolio_id)
            .await?;

        let page = self
            .transaction_repo
            .find_by_portfolio(portfolio_id, pagination, filters)
            .await?;

        let mut data = Vec::with_capacity(page.data.len());
        for transaction in page.data {
            data.push(self.enrich_transaction(transaction).await?);
        }

        Ok(PaginatedResult {
            data,
            pagination: page.pagination,
        })
    }

    /// Return one transaction after verifying portfolio ownership.
    pub async fn get(
        &self,
        user_id: Uuid,
        transaction_id: Uuid,
    ) -> Result<TransactionDetails, TransactionError> {
        let transaction = self
            .transaction_repo
            .find_by_id(transaction_id)
            .await?
            .ok_or(TransactionError::NotFound)?;

        self.ensure_portfolio_ownership(user_id, transaction.portfolio_id)
            .await
            .map_err(|error| match error {
                TransactionError::PortfolioNotFound => TransactionError::NotFound,
                other => other,
            })?;

        self.enrich_transaction(transaction).await
    }

    /// Create a transaction after resolving its asset and FX rate.
    pub async fn create(
        &self,
        user_id: Uuid,
        input: CreateTransactionInput,
    ) -> Result<TransactionDetails, TransactionError> {
        self.ensure_portfolio_ownership(user_id, input.portfolio_id)
            .await?;

        let asset = self.resolve_asset(&input.asset_isin).await?;
        let exchange_rate_to_base = self
            .resolve_exchange_rate(&input.currency, input.exchange_rate_to_base)
            .await?;
        let new_transaction = NewTransaction {
            portfolio_id: input.portfolio_id,
            asset_id: asset.id,
            transaction_type: input.transaction_type,
            date: input.date,
            settlement_date: input.settlement_date,
            quantity: input.quantity,
            unit_price: input.unit_price,
            commission: input.commission,
            currency: input.currency,
            exchange_rate_to_base,
            gross_amount: input.gross_amount,
            tax_withheld: input.tax_withheld,
            net_amount: input.net_amount,
            notes: input.notes,
            import_hash: None,
        };

        self.validate_creation_history(&new_transaction).await?;

        let transaction = self.transaction_repo.create(&new_transaction).await?;

        Ok(TransactionDetails {
            transaction,
            asset_isin: asset.isin,
            asset_name: asset.name,
        })
    }

    /// Update a transaction after verifying ownership and re-checking no-short-sell rules.
    pub async fn update(
        &self,
        user_id: Uuid,
        transaction_id: Uuid,
        input: UpdateTransactionInput,
    ) -> Result<TransactionDetails, TransactionError> {
        let existing = self
            .transaction_repo
            .find_by_id(transaction_id)
            .await?
            .ok_or(TransactionError::NotFound)?;

        self.ensure_portfolio_ownership(user_id, existing.portfolio_id)
            .await
            .map_err(|error| match error {
                TransactionError::PortfolioNotFound => TransactionError::NotFound,
                other => other,
            })?;

        let asset = self.resolve_asset(&input.asset_isin).await?;
        let exchange_rate_to_base = self
            .resolve_exchange_rate(&input.currency, input.exchange_rate_to_base)
            .await?;
        let update = UpdateTransaction {
            asset_id: asset.id,
            transaction_type: input.transaction_type,
            date: input.date,
            settlement_date: input.settlement_date,
            quantity: input.quantity,
            unit_price: input.unit_price,
            commission: input.commission,
            currency: input.currency,
            exchange_rate_to_base,
            gross_amount: input.gross_amount,
            tax_withheld: input.tax_withheld,
            net_amount: input.net_amount,
            notes: input.notes,
        };

        self.validate_update_history(&existing, &update).await?;

        let transaction = self
            .transaction_repo
            .update(transaction_id, &update)
            .await?;

        Ok(TransactionDetails {
            transaction,
            asset_isin: asset.isin,
            asset_name: asset.name,
        })
    }

    /// Delete a transaction after verifying ownership and downstream quantity safety.
    pub async fn delete(
        &self,
        user_id: Uuid,
        transaction_id: Uuid,
    ) -> Result<(), TransactionError> {
        let existing = self
            .transaction_repo
            .find_by_id(transaction_id)
            .await?
            .ok_or(TransactionError::NotFound)?;

        self.ensure_portfolio_ownership(user_id, existing.portfolio_id)
            .await
            .map_err(|error| match error {
                TransactionError::PortfolioNotFound => TransactionError::NotFound,
                other => other,
            })?;

        self.validate_delete_history(&existing).await?;
        self.transaction_repo.delete(transaction_id).await?;
        Ok(())
    }

    async fn ensure_portfolio_ownership(
        &self,
        user_id: Uuid,
        portfolio_id: Uuid,
    ) -> Result<(), TransactionError> {
        self.portfolio_repo
            .find_by_id(portfolio_id)
            .await?
            .filter(|portfolio| portfolio.user_id == user_id)
            .map(|_| ())
            .ok_or(TransactionError::PortfolioNotFound)
    }

    async fn enrich_transaction(
        &self,
        transaction: Transaction,
    ) -> Result<TransactionDetails, TransactionError> {
        let asset = self
            .asset_repo
            .find_by_id(transaction.asset_id)
            .await?
            .ok_or_else(|| {
                TransactionError::Repository(RepositoryError::Internal(format!(
                    "missing asset {} for transaction {}",
                    transaction.asset_id, transaction.id
                )))
            })?;

        Ok(TransactionDetails {
            transaction,
            asset_isin: asset.isin,
            asset_name: asset.name,
        })
    }

    async fn resolve_asset(&self, isin: &str) -> Result<Asset, TransactionError> {
        if let Some(asset) = self.asset_repo.find_by_isin(isin).await? {
            return Ok(asset);
        }

        let metadata = self
            .asset_metadata_lookup
            .lookup_asset_metadata(isin)
            .await
            .ok_or_else(|| TransactionError::AssetResolutionFailed(isin.to_string()))?;

        match self
            .asset_repo
            .create(&NewAsset {
                isin: metadata.isin,
                yahoo_ticker: metadata.yahoo_ticker,
                name: metadata.name,
                asset_class: metadata.asset_class,
                currency: metadata.currency,
                exchange: metadata.exchange,
            })
            .await
        {
            Ok(asset) => Ok(asset),
            Err(RepositoryError::Conflict(_)) => {
                self.asset_repo.find_by_isin(isin).await?.ok_or_else(|| {
                    TransactionError::Repository(RepositoryError::Internal(format!(
                        "asset {isin} conflicted during creation but could not be reloaded"
                    )))
                })
            }
            Err(error) => Err(TransactionError::Repository(error)),
        }
    }

    async fn resolve_exchange_rate(
        &self,
        currency: &str,
        provided_rate: Option<Decimal>,
    ) -> Result<Decimal, TransactionError> {
        if currency.eq_ignore_ascii_case("EUR") {
            return Ok(Decimal::ONE);
        }

        if let Some(rate) = provided_rate {
            return Ok(rate);
        }

        self.exchange_rate_lookup
            .lookup_rate_to_eur(currency)
            .await
            .ok_or_else(|| TransactionError::ExchangeRateRequired(currency.to_string()))
    }

    async fn validate_creation_history(
        &self,
        new_transaction: &NewTransaction,
    ) -> Result<(), TransactionError> {
        let mut transactions = self
            .transaction_repo
            .list_by_asset(new_transaction.portfolio_id, new_transaction.asset_id)
            .await?;

        transactions.push(transaction_from_new(new_transaction));
        validate_no_short_sell(transactions)
    }

    async fn validate_update_history(
        &self,
        existing: &Transaction,
        update: &UpdateTransaction,
    ) -> Result<(), TransactionError> {
        let mut current_asset_transactions = self
            .transaction_repo
            .list_by_asset(existing.portfolio_id, existing.asset_id)
            .await?;
        replace_transaction(
            &mut current_asset_transactions,
            existing.id,
            existing.asset_id == update.asset_id,
            transaction_from_update(existing, update),
        );
        validate_no_short_sell(current_asset_transactions)?;

        if existing.asset_id != update.asset_id {
            let mut new_asset_transactions = self
                .transaction_repo
                .list_by_asset(existing.portfolio_id, update.asset_id)
                .await?;
            new_asset_transactions.push(transaction_from_update(existing, update));
            validate_no_short_sell(new_asset_transactions)?;
        }

        Ok(())
    }

    async fn validate_delete_history(
        &self,
        existing: &Transaction,
    ) -> Result<(), TransactionError> {
        let mut transactions = self
            .transaction_repo
            .list_by_asset(existing.portfolio_id, existing.asset_id)
            .await?;
        transactions.retain(|transaction| transaction.id != existing.id);

        if transactions.is_empty() {
            return Ok(());
        }

        validate_no_short_sell(transactions)
    }
}

fn replace_transaction(
    transactions: &mut Vec<Transaction>,
    transaction_id: Uuid,
    same_asset: bool,
    replacement: Transaction,
) {
    transactions.retain(|transaction| transaction.id != transaction_id);
    if same_asset {
        transactions.push(replacement);
    }
}

fn validate_no_short_sell(mut transactions: Vec<Transaction>) -> Result<(), TransactionError> {
    if transactions.is_empty() {
        return Ok(());
    }

    transactions.sort_by(|left, right| {
        left.date
            .cmp(&right.date)
            .then_with(|| left.created_at.cmp(&right.created_at))
            .then_with(|| left.id.cmp(&right.id))
    });

    aggregate_position(&transactions)
        .map(|_| ())
        .map_err(|error| match error {
            DomainError::InsufficientQuantity {
                available,
                requested,
            } => TransactionError::InsufficientQuantity {
                available,
                requested,
            },
            DomainError::ValidationError(_) => {
                TransactionError::Repository(RepositoryError::Internal(
                    "unexpected empty transaction history during validation".to_string(),
                ))
            }
            other => TransactionError::Repository(RepositoryError::Internal(other.to_string())),
        })
}

fn transaction_from_new(new_transaction: &NewTransaction) -> Transaction {
    let now = chrono::Utc::now();
    Transaction {
        id: Uuid::now_v7(),
        portfolio_id: new_transaction.portfolio_id,
        asset_id: new_transaction.asset_id,
        transaction_type: new_transaction.transaction_type,
        date: new_transaction.date,
        settlement_date: new_transaction.settlement_date,
        quantity: new_transaction.quantity,
        unit_price: new_transaction.unit_price,
        commission: new_transaction.commission,
        currency: new_transaction.currency.clone(),
        exchange_rate_to_base: new_transaction.exchange_rate_to_base,
        gross_amount: new_transaction.gross_amount,
        tax_withheld: new_transaction.tax_withheld,
        net_amount: new_transaction.net_amount,
        notes: new_transaction.notes.clone(),
        import_hash: new_transaction.import_hash.clone(),
        created_at: now,
        updated_at: now,
    }
}

fn transaction_from_update(existing: &Transaction, update: &UpdateTransaction) -> Transaction {
    Transaction {
        id: existing.id,
        portfolio_id: existing.portfolio_id,
        asset_id: update.asset_id,
        transaction_type: update.transaction_type,
        date: update.date,
        settlement_date: update.settlement_date,
        quantity: update.quantity,
        unit_price: update.unit_price,
        commission: update.commission,
        currency: update.currency.clone(),
        exchange_rate_to_base: update.exchange_rate_to_base,
        gross_amount: update.gross_amount,
        tax_withheld: update.tax_withheld,
        net_amount: update.net_amount,
        notes: update.notes.clone(),
        import_hash: existing.import_hash.clone(),
        created_at: existing.created_at,
        updated_at: chrono::Utc::now(),
    }
}
