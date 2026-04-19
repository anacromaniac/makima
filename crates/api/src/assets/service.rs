//! Asset service — business logic for shared asset CRUD and ticker lookup.

use async_trait::async_trait;
use domain::{
    Asset, AssetFilters, AssetRepository, NewAsset, PaginatedResult, PaginationParams,
    RepositoryError, UpdateAsset,
};
use price_fetcher::openfigi::OpenFigiClient;

/// Lookup adapter used to map an ISIN to a Yahoo Finance ticker.
#[async_trait]
pub trait AssetTickerLookup: Send + Sync {
    /// Return the mapped Yahoo Finance ticker, or `None` when no mapping is
    /// available or the upstream service fails.
    async fn lookup_yahoo_ticker(&self, isin: &str) -> Option<String>;
}

#[async_trait]
impl AssetTickerLookup for OpenFigiClient {
    async fn lookup_yahoo_ticker(&self, isin: &str) -> Option<String> {
        OpenFigiClient::lookup_yahoo_ticker(self, isin).await
    }
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

/// Create a shared asset and backfill its Yahoo ticker via OpenFIGI when
/// `yahoo_ticker` is not provided.
pub async fn create(
    repo: &dyn AssetRepository,
    ticker_lookup: &dyn AssetTickerLookup,
    mut new_asset: NewAsset,
) -> Result<Asset, AssetError> {
    if new_asset.yahoo_ticker.is_none() {
        new_asset.yahoo_ticker = ticker_lookup.lookup_yahoo_ticker(&new_asset.isin).await;
    }

    repo.create(&new_asset).await.map_err(|error| match error {
        RepositoryError::Conflict(_) => AssetError::DuplicateIsin,
        other => AssetError::Repository(other),
    })
}

/// List shared assets with pagination and optional filters.
pub async fn list(
    repo: &dyn AssetRepository,
    pagination: &PaginationParams,
    filters: &AssetFilters,
) -> Result<PaginatedResult<Asset>, AssetError> {
    repo.list(pagination, filters).await.map_err(Into::into)
}

/// Return a single asset by ISIN.
pub async fn get(repo: &dyn AssetRepository, isin: &str) -> Result<Asset, AssetError> {
    repo.find_by_isin(isin).await?.ok_or(AssetError::NotFound)
}

/// Update an existing asset by ISIN.
pub async fn update(
    repo: &dyn AssetRepository,
    isin: &str,
    update: UpdateAsset,
) -> Result<Asset, AssetError> {
    let asset = repo.find_by_isin(isin).await?.ok_or(AssetError::NotFound)?;

    repo.update(asset.id, &update).await.map_err(Into::into)
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
