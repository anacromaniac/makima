//! Broker import use cases.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use chrono::Utc;
use domain::{
    Asset, AssetClass, AssetRepository, BrokerImportParseError, BrokerImportRepository,
    BrokerImporter, DomainError, NewAsset, NewTransaction, PortfolioRepository, RepositoryError,
    Transaction, TransactionRepository, TransactionType, aggregate_position,
};
use rust_decimal::Decimal;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    assets::AssetPriceBackfill,
    transactions::{AssetMetadataLookup, ExchangeRateLookup, ResolvedAssetMetadata},
};

/// Successful broker import summary returned to the API layer.
#[derive(Debug, Clone)]
pub struct ImportSummary {
    /// Number of persisted transactions.
    pub transactions_imported: u64,
    /// ISINs auto-created during the import.
    pub assets_created: Vec<String>,
    /// Non-fatal warnings collected during the import.
    pub warnings: Vec<String>,
}

/// Errors that can occur during a broker import workflow.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    /// The requested broker parser is not supported.
    #[error("unsupported broker: {0}")]
    InvalidBroker(String),
    /// Portfolio not found, or owned by another user.
    #[error("portfolio not found")]
    PortfolioNotFound,
    /// The uploaded file contains invalid rows.
    #[error(transparent)]
    Parse(#[from] BrokerImportParseError),
    /// Domain-level validation failed during import preparation.
    #[error("validation error: {0}")]
    Validation(String),
    /// Persistence failure.
    #[error("repository error: {0}")]
    Repository(#[from] RepositoryError),
}

/// Application service for broker file imports.
#[derive(Clone)]
pub struct ImportService {
    portfolio_repo: Arc<dyn PortfolioRepository>,
    asset_repo: Arc<dyn AssetRepository>,
    transaction_repo: Arc<dyn TransactionRepository>,
    broker_import_repo: Arc<dyn BrokerImportRepository>,
    asset_metadata_lookup: Arc<dyn AssetMetadataLookup>,
    exchange_rate_lookup: Arc<dyn ExchangeRateLookup>,
    price_backfill: Arc<dyn AssetPriceBackfill>,
    importers: HashMap<String, Arc<dyn BrokerImporter + Send + Sync>>,
}

impl ImportService {
    /// Create a new import service.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        portfolio_repo: Arc<dyn PortfolioRepository>,
        asset_repo: Arc<dyn AssetRepository>,
        transaction_repo: Arc<dyn TransactionRepository>,
        broker_import_repo: Arc<dyn BrokerImportRepository>,
        asset_metadata_lookup: Arc<dyn AssetMetadataLookup>,
        exchange_rate_lookup: Arc<dyn ExchangeRateLookup>,
        price_backfill: Arc<dyn AssetPriceBackfill>,
        importers: HashMap<String, Arc<dyn BrokerImporter + Send + Sync>>,
    ) -> Self {
        Self {
            portfolio_repo,
            asset_repo,
            transaction_repo,
            broker_import_repo,
            asset_metadata_lookup,
            exchange_rate_lookup,
            price_backfill,
            importers,
        }
    }

    /// Import a broker file into a portfolio owned by `user_id`.
    pub async fn import(
        &self,
        user_id: Uuid,
        broker: &str,
        portfolio_id: Uuid,
        file_bytes: &[u8],
    ) -> Result<ImportSummary, ImportError> {
        self.portfolio_repo
            .find_by_id(portfolio_id)
            .await?
            .filter(|portfolio| portfolio.user_id == user_id)
            .ok_or(ImportError::PortfolioNotFound)?;

        let importer = self
            .importers
            .get(&broker.to_ascii_lowercase())
            .ok_or_else(|| ImportError::InvalidBroker(broker.to_string()))?;
        let parsed_rows = importer.parse(file_bytes)?;
        let mut warnings = Vec::new();

        let mut pending_rows = Vec::new();
        let mut upload_hashes = HashSet::new();
        for row in parsed_rows {
            let import_hash = compute_import_hash(&row);
            if !upload_hashes.insert(import_hash.clone()) {
                warnings.push(format!(
                    "Skipped duplicate transaction in uploaded file for ISIN {} on {}",
                    row.isin, row.date
                ));
                continue;
            }
            pending_rows.push((row, import_hash));
        }

        let existing_hashes = self
            .broker_import_repo
            .find_existing_import_hashes(
                portfolio_id,
                &pending_rows
                    .iter()
                    .map(|(_, hash)| hash.clone())
                    .collect::<Vec<_>>(),
            )
            .await?
            .into_iter()
            .collect::<HashSet<_>>();

        let filtered_rows = pending_rows
            .into_iter()
            .filter_map(|(row, hash)| {
                if existing_hashes.contains(&hash) {
                    warnings.push(format!(
                        "Skipped duplicate transaction already imported for ISIN {} on {}",
                        row.isin, row.date
                    ));
                    None
                } else {
                    Some((row, hash))
                }
            })
            .collect::<Vec<_>>();

        if filtered_rows.is_empty() {
            return Ok(ImportSummary {
                transactions_imported: 0,
                assets_created: Vec::new(),
                warnings,
            });
        }

        let mut assets_created = Vec::new();
        let mut asset_cache = HashMap::<String, Asset>::new();
        let mut prepared_assets = Vec::new();
        let mut prepared_transactions = Vec::new();

        for (row, import_hash) in filtered_rows {
            let asset = self
                .resolve_asset(
                    &row,
                    &mut asset_cache,
                    &mut prepared_assets,
                    &mut assets_created,
                    &mut warnings,
                )
                .await?;
            let exchange_rate_to_base = self
                .resolve_exchange_rate(&row.currency, &row.isin, row.date, &mut warnings)
                .await;

            prepared_transactions.push(domain::PreparedImportTransaction {
                transaction: NewTransaction {
                    portfolio_id,
                    asset_id: asset.id,
                    transaction_type: row.transaction_type,
                    date: row.date,
                    settlement_date: row.settlement_date,
                    quantity: row.quantity,
                    unit_price: row.unit_price,
                    commission: row.commission,
                    currency: row.currency,
                    exchange_rate_to_base,
                    gross_amount: row.gross_amount,
                    tax_withheld: row.tax_withheld,
                    net_amount: row.net_amount,
                    notes: row.notes,
                    import_hash: Some(import_hash),
                },
            });
        }

        prepared_transactions.sort_by(|left, right| {
            left.transaction
                .date
                .cmp(&right.transaction.date)
                .then_with(|| {
                    transaction_type_rank(left.transaction.transaction_type)
                        .cmp(&transaction_type_rank(right.transaction.transaction_type))
                })
                .then_with(|| left.transaction.asset_id.cmp(&right.transaction.asset_id))
        });

        self.validate_positions(&prepared_transactions).await?;
        self.broker_import_repo
            .import_batch(&prepared_assets, &prepared_transactions)
            .await?;

        for asset in &prepared_assets {
            if let Some(ticker) = asset.asset.yahoo_ticker.as_deref()
                && let Err(error) = self
                    .price_backfill
                    .backfill_asset_prices(asset.id, ticker)
                    .await
            {
                warnings.push(format!(
                    "Price-history backfill failed for ISIN {}: {error}",
                    asset.asset.isin
                ));
            }
        }

        Ok(ImportSummary {
            transactions_imported: prepared_transactions.len() as u64,
            assets_created,
            warnings,
        })
    }

    async fn resolve_asset(
        &self,
        row: &domain::ParsedBrokerTransaction,
        asset_cache: &mut HashMap<String, Asset>,
        prepared_assets: &mut Vec<domain::PreparedImportAsset>,
        assets_created: &mut Vec<String>,
        warnings: &mut Vec<String>,
    ) -> Result<Asset, ImportError> {
        if let Some(asset) = asset_cache.get(&row.isin) {
            return Ok(asset.clone());
        }

        if let Some(asset) = self.asset_repo.find_by_isin(&row.isin).await? {
            asset_cache.insert(row.isin.clone(), asset.clone());
            return Ok(asset);
        }

        let metadata = self
            .asset_metadata_lookup
            .lookup_asset_metadata(&row.isin)
            .await;
        let new_asset = build_new_asset(row, metadata.as_ref());
        if metadata.is_none() {
            warnings.push(format!(
                "OpenFIGI lookup failed for ISIN {}; imported with broker-provided metadata only",
                row.isin
            ));
        }
        if new_asset.yahoo_ticker.is_none() {
            warnings.push(format!(
                "Asset {} was created without a Yahoo Finance ticker",
                row.isin
            ));
        }

        let asset = Asset {
            id: Uuid::now_v7(),
            isin: new_asset.isin.clone(),
            yahoo_ticker: new_asset.yahoo_ticker.clone(),
            name: new_asset.name.clone(),
            asset_class: new_asset.asset_class,
            currency: new_asset.currency.clone(),
            exchange: new_asset.exchange.clone(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        prepared_assets.push(domain::PreparedImportAsset {
            id: asset.id,
            asset: new_asset,
        });
        assets_created.push(asset.isin.clone());
        asset_cache.insert(asset.isin.clone(), asset.clone());
        Ok(asset)
    }

    async fn resolve_exchange_rate(
        &self,
        currency: &str,
        isin: &str,
        date: chrono::NaiveDate,
        warnings: &mut Vec<String>,
    ) -> Decimal {
        if currency.eq_ignore_ascii_case("EUR") {
            return Decimal::ONE;
        }

        match self.exchange_rate_lookup.lookup_rate_to_eur(currency).await {
            Some(rate) => rate,
            None => {
                warnings.push(format!(
                    "Missing EUR exchange rate for {currency} transaction on {date} (ISIN {isin}); stored as 0"
                ));
                Decimal::ZERO
            }
        }
    }

    async fn validate_positions(
        &self,
        prepared_transactions: &[domain::PreparedImportTransaction],
    ) -> Result<(), ImportError> {
        let mut by_asset = HashMap::<Uuid, Vec<Transaction>>::new();
        for prepared in prepared_transactions {
            by_asset
                .entry(prepared.transaction.asset_id)
                .or_default()
                .push(transaction_from_new(&prepared.transaction));
        }

        for (asset_id, new_transactions) in by_asset {
            let portfolio_id = new_transactions[0].portfolio_id;
            let mut transactions = self
                .transaction_repo
                .list_by_asset(portfolio_id, asset_id)
                .await?;
            transactions.extend(new_transactions);
            transactions.sort_by(|left, right| {
                left.date
                    .cmp(&right.date)
                    .then_with(|| left.created_at.cmp(&right.created_at))
                    .then_with(|| left.id.cmp(&right.id))
            });
            aggregate_position(&transactions).map_err(|error| match error {
                DomainError::InsufficientQuantity { .. } | DomainError::ValidationError(_) => {
                    ImportError::Validation(error.to_string())
                }
                _ => ImportError::Validation(error.to_string()),
            })?;
        }

        Ok(())
    }
}

fn build_new_asset(
    row: &domain::ParsedBrokerTransaction,
    metadata: Option<&ResolvedAssetMetadata>,
) -> NewAsset {
    let metadata = metadata.cloned();
    let resolved = metadata.unwrap_or(ResolvedAssetMetadata {
        isin: row.isin.clone(),
        yahoo_ticker: None,
        name: row.asset_name.clone(),
        asset_class: row.asset_class.unwrap_or(AssetClass::Alternative),
        currency: row.asset_currency.clone(),
        exchange: row.exchange.clone(),
    });

    NewAsset {
        isin: resolved.isin,
        yahoo_ticker: resolved.yahoo_ticker,
        name: resolved.name,
        asset_class: resolved.asset_class,
        currency: resolved.currency,
        exchange: resolved.exchange,
    }
}

fn compute_import_hash(row: &domain::ParsedBrokerTransaction) -> String {
    let payload = format!(
        "{}|{}|{}|{}|{}",
        row.date,
        row.isin,
        row.quantity
            .map(|value| value.normalize().to_string())
            .unwrap_or_default(),
        row.unit_price
            .map(|value| value.normalize().to_string())
            .unwrap_or_default(),
        match row.transaction_type {
            TransactionType::Buy => "Buy",
            TransactionType::Sell => "Sell",
            TransactionType::Dividend => "Dividend",
            TransactionType::Coupon => "Coupon",
        }
    );

    let mut hasher = Sha256::new();
    hasher.update(payload.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn transaction_from_new(new_transaction: &NewTransaction) -> Transaction {
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
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn transaction_type_rank(transaction_type: TransactionType) -> u8 {
    match transaction_type {
        TransactionType::Buy => 0,
        TransactionType::Dividend => 1,
        TransactionType::Coupon => 2,
        TransactionType::Sell => 3,
    }
}
