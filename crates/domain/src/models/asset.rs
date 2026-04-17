//! Asset domain model and asset class enumeration.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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
