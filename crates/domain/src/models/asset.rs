//! Asset domain model and asset class enumeration.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use uuid::Uuid;

/// Classification of a financial instrument.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum AssetClass {
    /// Equities / stocks.
    Stock,
    /// Fixed-income securities.
    Bond,
    /// Raw materials or commodity-tracking instruments.
    Commodity,
    /// Hedge funds, PE, real estate, etc.
    Alternative,
    /// Cryptocurrencies (enum exists; no specific logic in MVP).
    Crypto,
    /// Money-market funds, term deposits, etc.
    CashEquivalent,
}

impl AssetClass {
    /// Return the stable string representation stored in PostgreSQL and exposed
    /// across service boundaries.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stock => "Stock",
            Self::Bond => "Bond",
            Self::Commodity => "Commodity",
            Self::Alternative => "Alternative",
            Self::Crypto => "Crypto",
            Self::CashEquivalent => "CashEquivalent",
        }
    }
}

impl FromStr for AssetClass {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "Stock" => Ok(Self::Stock),
            "Bond" => Ok(Self::Bond),
            "Commodity" => Ok(Self::Commodity),
            "Alternative" => Ok(Self::Alternative),
            "Crypto" => Ok(Self::Crypto),
            "CashEquivalent" => Ok(Self::CashEquivalent),
            _ => Err(format!("unsupported asset class: {value}")),
        }
    }
}

/// A tradeable financial instrument shared across all users.
///
/// Identified primarily by ISIN. Assets are auto-created on first reference
/// (e.g. during broker import) using data from OpenFIGI when available.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    /// Unique identifier (UUID v7).
    pub id: Uuid,
    /// International Securities Identification Number (e.g. `IE00BK5BQT80`).
    pub isin: String,
    /// Yahoo Finance ticker symbol, if mapped.
    pub yahoo_ticker: Option<String>,
    /// Human-readable instrument name.
    pub name: String,
    /// Instrument classification.
    pub asset_class: AssetClass,
    /// Quotation currency (e.g. `EUR`, `USD`).
    pub currency: String,
    /// Exchange where the instrument is listed, if known.
    pub exchange: Option<String>,
    /// When the asset was created.
    pub created_at: DateTime<Utc>,
    /// When the asset was last updated.
    pub updated_at: DateTime<Utc>,
}

/// Data needed to create a new asset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewAsset {
    /// International Securities Identification Number.
    pub isin: String,
    /// Yahoo Finance ticker symbol, if known.
    pub yahoo_ticker: Option<String>,
    /// Human-readable instrument name.
    pub name: String,
    /// Instrument classification.
    pub asset_class: AssetClass,
    /// Quotation currency.
    pub currency: String,
    /// Exchange where the instrument is listed.
    pub exchange: Option<String>,
}

/// Supported filters for listing shared assets.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssetFilters {
    /// Restrict results to a specific asset class.
    pub asset_class: Option<AssetClass>,
    /// Case-insensitive name substring search.
    pub name_search: Option<String>,
}

/// Mutable asset fields accepted by update operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAsset {
    /// Yahoo Finance ticker symbol, if known.
    pub yahoo_ticker: Option<String>,
    /// Human-readable instrument name.
    pub name: String,
    /// Instrument classification.
    pub asset_class: AssetClass,
    /// Quotation currency.
    pub currency: String,
    /// Exchange where the instrument is listed.
    pub exchange: Option<String>,
}
