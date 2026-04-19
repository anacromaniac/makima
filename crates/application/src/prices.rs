//! Price-history use cases and external price-fetching ports.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::NaiveDate;
use domain::{
    AssetRepository, NewPriceRecord, PaginatedResult, PaginationParams, PriceRecord,
    PriceRepository, PriceSource, RepositoryError,
};

/// External lookup used to resolve the latest daily close for a Yahoo ticker.
#[async_trait]
pub trait CurrentPriceLookup: Send + Sync {
    /// Fetch the latest daily price for the provided asset/ticker pair.
    async fn fetch_current_price(
        &self,
        asset_id: uuid::Uuid,
        ticker: &str,
    ) -> Result<NewPriceRecord, domain::DomainError>;
}

/// Errors that can occur during price workflows.
#[derive(Debug, thiserror::Error)]
pub enum PriceError {
    /// Asset not found.
    #[error("asset not found")]
    NotFound,
    /// The asset does not have a Yahoo Finance ticker.
    #[error("asset does not have a yahoo ticker")]
    MissingYahooTicker,
    /// Upstream market-data lookup failed.
    #[error("external service error: {0}")]
    ExternalService(String),
    /// Repository failure.
    #[error("repository error: {0}")]
    Repository(#[from] RepositoryError),
}

/// Application service for asset price history and refresh workflows.
#[derive(Clone)]
pub struct PriceService {
    asset_repo: Arc<dyn AssetRepository>,
    price_repo: Arc<dyn PriceRepository>,
    current_price_lookup: Arc<dyn CurrentPriceLookup>,
}

impl PriceService {
    /// Create a new price service.
    pub fn new(
        asset_repo: Arc<dyn AssetRepository>,
        price_repo: Arc<dyn PriceRepository>,
        current_price_lookup: Arc<dyn CurrentPriceLookup>,
    ) -> Self {
        Self {
            asset_repo,
            price_repo,
            current_price_lookup,
        }
    }

    /// Refresh the latest price for an asset identified by ISIN.
    pub async fn refresh(&self, isin: &str) -> Result<PriceRecord, PriceError> {
        let asset = self
            .asset_repo
            .find_by_isin(isin)
            .await?
            .ok_or(PriceError::NotFound)?;
        let ticker = asset
            .yahoo_ticker
            .as_deref()
            .ok_or(PriceError::MissingYahooTicker)?;
        let fetched = self
            .current_price_lookup
            .fetch_current_price(asset.id, ticker)
            .await
            .map_err(|error| PriceError::ExternalService(error.to_string()))?;

        self.price_repo.insert(&fetched).await.map_err(Into::into)
    }

    /// Store a manually entered daily close price for an asset identified by ISIN.
    pub async fn store_manual_price(
        &self,
        isin: &str,
        date: NaiveDate,
        close_price: rust_decimal::Decimal,
        currency: String,
    ) -> Result<PriceRecord, PriceError> {
        let asset = self
            .asset_repo
            .find_by_isin(isin)
            .await?
            .ok_or(PriceError::NotFound)?;

        self.price_repo
            .insert(&NewPriceRecord {
                asset_id: asset.id,
                date,
                close_price,
                currency,
                source: PriceSource::Manual,
            })
            .await
            .map_err(Into::into)
    }

    /// List price history for an asset identified by ISIN.
    pub async fn list_history(
        &self,
        isin: &str,
        from_date: Option<NaiveDate>,
        to_date: Option<NaiveDate>,
        pagination: &PaginationParams,
    ) -> Result<PaginatedResult<PriceRecord>, PriceError> {
        let asset = self
            .asset_repo
            .find_by_isin(isin)
            .await?
            .ok_or(PriceError::NotFound)?;

        self.price_repo
            .find_by_range(asset.id, from_date, to_date, pagination)
            .await
            .map_err(Into::into)
    }
}
