//! Price record domain model and price source enumeration.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// How a price entry was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PriceSource {
    /// Fetched from Yahoo Finance.
    #[serde(rename = "yahoo")]
    Yahoo,
    /// Entered or overridden by a user.
    #[serde(rename = "manual")]
    Manual,
}

/// A single closing-price observation for an asset on a given date.
///
/// Each daily fetch inserts a new row; prices are never overwritten. This
/// accumulates history that can be used for analytics and charting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceRecord {
    /// Unique identifier (UUID v7).
    pub id: Uuid,
    /// Asset this price belongs to.
    pub asset_id: Uuid,
    /// Date of the price observation.
    pub date: NaiveDate,
    /// Closing price.
    pub close_price: Decimal,
    /// Currency of the price.
    pub currency: String,
    /// How this price was obtained.
    pub source: PriceSource,
}

/// Data needed to insert a new price record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewPriceRecord {
    /// Asset this price belongs to.
    pub asset_id: Uuid,
    /// Date of the price observation.
    pub date: NaiveDate,
    /// Closing price.
    pub close_price: Decimal,
    /// Currency of the price.
    pub currency: String,
    /// How this price was obtained.
    pub source: PriceSource,
}
