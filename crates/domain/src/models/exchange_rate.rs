//! Exchange rate domain model.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A currency exchange rate for a specific date.
///
/// Used to convert foreign-currency transactions and positions to the
/// portfolio base currency (EUR). Rates are fetched from Yahoo Finance
/// using pairs like `EURUSD=X`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeRate {
    /// Unique identifier (UUID v7).
    pub id: Uuid,
    /// Source currency code (e.g. `USD`).
    pub from_currency: String,
    /// Target currency code (e.g. `EUR`).
    pub to_currency: String,
    /// Date of the rate.
    pub date: NaiveDate,
    /// Conversion rate: 1 unit of `from_currency` = `rate` units of `to_currency`.
    pub rate: Decimal,
}

/// Data needed to insert a new exchange rate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewExchangeRate {
    /// Source currency code.
    pub from_currency: String,
    /// Target currency code.
    pub to_currency: String,
    /// Date of the rate.
    pub date: NaiveDate,
    /// Conversion rate.
    pub rate: Decimal,
}
