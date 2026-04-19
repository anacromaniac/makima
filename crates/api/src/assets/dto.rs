//! Request and response DTOs for asset endpoints.

use chrono::{DateTime, Utc};
use domain::Asset;
use garde::Validate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// API-facing asset class enum used by request and response DTOs.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "PascalCase")]
pub enum ApiAssetClass {
    /// Equities / stocks.
    Stock,
    /// Fixed-income securities.
    Bond,
    /// Raw materials or commodity-tracking instruments.
    Commodity,
    /// Hedge funds, PE, real estate, etc.
    Alternative,
    /// Cryptocurrencies.
    Crypto,
    /// Money-market funds, term deposits, etc.
    CashEquivalent,
}

impl From<ApiAssetClass> for domain::AssetClass {
    fn from(value: ApiAssetClass) -> Self {
        match value {
            ApiAssetClass::Stock => Self::Stock,
            ApiAssetClass::Bond => Self::Bond,
            ApiAssetClass::Commodity => Self::Commodity,
            ApiAssetClass::Alternative => Self::Alternative,
            ApiAssetClass::Crypto => Self::Crypto,
            ApiAssetClass::CashEquivalent => Self::CashEquivalent,
        }
    }
}

impl From<domain::AssetClass> for ApiAssetClass {
    fn from(value: domain::AssetClass) -> Self {
        match value {
            domain::AssetClass::Stock => Self::Stock,
            domain::AssetClass::Bond => Self::Bond,
            domain::AssetClass::Commodity => Self::Commodity,
            domain::AssetClass::Alternative => Self::Alternative,
            domain::AssetClass::Crypto => Self::Crypto,
            domain::AssetClass::CashEquivalent => Self::CashEquivalent,
        }
    }
}

/// Request body for `POST /api/v1/assets`.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
pub struct CreateAssetRequest {
    /// International Securities Identification Number.
    #[garde(custom(crate::assets::service::is_valid_isin))]
    pub isin: String,
    /// Yahoo Finance ticker symbol, if already known.
    #[garde(skip)]
    pub yahoo_ticker: Option<String>,
    /// Human-readable instrument name.
    #[garde(length(min = 1))]
    pub name: String,
    /// Instrument classification.
    #[garde(skip)]
    pub asset_class: ApiAssetClass,
    /// Quotation currency (3-letter ISO code in practice).
    #[garde(length(min = 3, max = 10))]
    pub currency: String,
    /// Exchange where the asset is listed, if known.
    #[garde(skip)]
    pub exchange: Option<String>,
}

/// Request body for `PUT /api/v1/assets/{isin}`.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
pub struct UpdateAssetRequest {
    /// Yahoo Finance ticker symbol, if known.
    #[garde(skip)]
    pub yahoo_ticker: Option<String>,
    /// Human-readable instrument name.
    #[garde(length(min = 1))]
    pub name: String,
    /// Instrument classification.
    #[garde(skip)]
    pub asset_class: ApiAssetClass,
    /// Quotation currency (3-letter ISO code in practice).
    #[garde(length(min = 3, max = 10))]
    pub currency: String,
    /// Exchange where the asset is listed, if known.
    #[garde(skip)]
    pub exchange: Option<String>,
}

/// Asset returned by the API.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct AssetResponse {
    /// Unique asset identifier.
    pub id: Uuid,
    /// International Securities Identification Number.
    pub isin: String,
    /// Yahoo Finance ticker symbol, if mapped.
    pub yahoo_ticker: Option<String>,
    /// Human-readable instrument name.
    pub name: String,
    /// Instrument classification.
    pub asset_class: ApiAssetClass,
    /// Quotation currency.
    pub currency: String,
    /// Exchange where the asset is listed, if known.
    pub exchange: Option<String>,
    /// When the asset was created.
    pub created_at: DateTime<Utc>,
    /// When the asset was last updated.
    pub updated_at: DateTime<Utc>,
}

impl From<Asset> for AssetResponse {
    fn from(asset: Asset) -> Self {
        Self {
            id: asset.id,
            isin: asset.isin,
            yahoo_ticker: asset.yahoo_ticker,
            name: asset.name,
            asset_class: asset.asset_class.into(),
            currency: asset.currency,
            exchange: asset.exchange,
            created_at: asset.created_at,
            updated_at: asset.updated_at,
        }
    }
}
