//! Asset use cases and application-specific external ports.

use std::sync::Arc;

use async_trait::async_trait;
use domain::{
    Asset, AssetFilters, AssetRepository, NewAsset, PaginatedResult, PaginationParams,
    RepositoryError, UpdateAsset,
};
use uuid::Uuid;

/// Lookup adapter used to map an ISIN to a Yahoo Finance ticker.
#[async_trait]
pub trait AssetTickerLookup: Send + Sync {
    /// Return the mapped Yahoo Finance ticker, or `None` when no mapping is
    /// available or the upstream service fails.
    async fn lookup_yahoo_ticker(&self, isin: &str) -> Option<String>;
}

/// Backfill adapter used when a Yahoo ticker becomes available for an asset.
#[async_trait]
pub trait AssetPriceBackfill: Send + Sync {
    /// Fetch and store historical prices for the given asset.
    async fn backfill_asset_prices(
        &self,
        asset_id: Uuid,
        ticker: &str,
    ) -> Result<u64, domain::DomainError>;
}

/// Errors that can occur during asset operations.
#[derive(Debug, thiserror::Error)]
pub enum AssetError {
    /// Asset not found.
    #[error("asset not found")]
    NotFound,
    /// The ISIN already exists.
    #[error("asset with this ISIN already exists")]
    DuplicateIsin,
    /// Underlying repository failure.
    #[error("repository error: {0}")]
    Repository(#[from] RepositoryError),
}

/// Application service for shared asset workflows.
#[derive(Clone)]
pub struct AssetService {
    repo: Arc<dyn AssetRepository>,
    ticker_lookup: Arc<dyn AssetTickerLookup>,
    price_backfill: Arc<dyn AssetPriceBackfill>,
}

impl AssetService {
    /// Create a new asset service.
    pub fn new(
        repo: Arc<dyn AssetRepository>,
        ticker_lookup: Arc<dyn AssetTickerLookup>,
        price_backfill: Arc<dyn AssetPriceBackfill>,
    ) -> Self {
        Self {
            repo,
            ticker_lookup,
            price_backfill,
        }
    }

    /// Create a shared asset and backfill its Yahoo ticker when missing.
    pub async fn create(&self, mut new_asset: NewAsset) -> Result<Asset, AssetError> {
        if new_asset.yahoo_ticker.is_none() {
            new_asset.yahoo_ticker = self
                .ticker_lookup
                .lookup_yahoo_ticker(&new_asset.isin)
                .await;
        }

        let asset = self
            .repo
            .create(&new_asset)
            .await
            .map_err(|error| match error {
                RepositoryError::Conflict(_) => AssetError::DuplicateIsin,
                other => AssetError::Repository(other),
            })?;

        self.try_backfill_if_ticker_available(asset.id, asset.yahoo_ticker.as_deref())
            .await;

        Ok(asset)
    }

    /// List shared assets with pagination and optional filters.
    pub async fn list(
        &self,
        pagination: &PaginationParams,
        filters: &AssetFilters,
    ) -> Result<PaginatedResult<Asset>, AssetError> {
        self.repo
            .list(pagination, filters)
            .await
            .map_err(Into::into)
    }

    /// Return a single shared asset by ISIN.
    pub async fn get(&self, isin: &str) -> Result<Asset, AssetError> {
        self.repo
            .find_by_isin(isin)
            .await?
            .ok_or(AssetError::NotFound)
    }

    /// Update an existing asset by ISIN.
    pub async fn update(&self, isin: &str, update: UpdateAsset) -> Result<Asset, AssetError> {
        let existing_asset = self
            .repo
            .find_by_isin(isin)
            .await?
            .ok_or(AssetError::NotFound)?;

        let updated_asset = self
            .repo
            .update(existing_asset.id, &update)
            .await
            .map_err(AssetError::from)?;

        if existing_asset.yahoo_ticker.is_none() && updated_asset.yahoo_ticker.is_some() {
            self.try_backfill_if_ticker_available(
                updated_asset.id,
                updated_asset.yahoo_ticker.as_deref(),
            )
            .await;
        }

        Ok(updated_asset)
    }

    async fn try_backfill_if_ticker_available(&self, asset_id: Uuid, ticker: Option<&str>) {
        let Some(ticker) = ticker else {
            return;
        };

        if let Err(error) = self
            .price_backfill
            .backfill_asset_prices(asset_id, ticker)
            .await
        {
            tracing::warn!(
                asset_id = %asset_id,
                ticker,
                error = %error,
                "price-history backfill failed"
            );
        }
    }
}

/// Return `true` when the string looks like a valid ISIN.
pub fn is_valid_isin(value: &str, _: &()) -> garde::Result {
    if value.len() != 12 {
        return Err(garde::Error::new("ISIN must be exactly 12 characters"));
    }

    let mut chars = value.chars();
    if !chars.by_ref().take(2).all(|char| char.is_ascii_uppercase()) {
        return Err(garde::Error::new(
            "ISIN must start with a 2-letter country code",
        ));
    }

    let middle: Vec<char> = value.chars().skip(2).take(9).collect();
    if !middle.iter().all(|char| char.is_ascii_alphanumeric()) {
        return Err(garde::Error::new(
            "ISIN characters 3-11 must be alphanumeric",
        ));
    }

    if !value
        .chars()
        .last()
        .is_some_and(|char| char.is_ascii_digit())
    {
        return Err(garde::Error::new("ISIN must end with a check digit"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::is_valid_isin;

    #[test]
    fn test_is_valid_isin_accepts_uppercase_format() {
        assert!(is_valid_isin("IE00BK5BQT80", &()).is_ok());
    }

    #[test]
    fn test_is_valid_isin_rejects_bad_country_code() {
        assert!(is_valid_isin("1E00BK5BQT80", &()).is_err());
    }

    #[test]
    fn test_is_valid_isin_rejects_non_digit_check_digit() {
        assert!(is_valid_isin("IE00BK5BQT8X", &()).is_err());
    }
}
