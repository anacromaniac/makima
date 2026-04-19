//! Transaction domain model and transaction type enumeration.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The kind of financial operation a transaction represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum TransactionType {
    /// Purchase of an asset.
    Buy,
    /// Sale of an asset.
    Sell,
    /// Cash distribution from a stock or fund.
    Dividend,
    /// Interest payment from a bond.
    Coupon,
}

/// A single financial transaction within a portfolio.
///
/// Buy/Sell transactions carry `quantity` and `unit_price`. Dividend/Coupon
/// transactions carry `gross_amount`, `tax_withheld`, and `net_amount`.
/// All monetary values use [`Decimal`] — never floating point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    /// Unique identifier (UUID v7).
    pub id: Uuid,
    /// Portfolio this transaction belongs to.
    pub portfolio_id: Uuid,
    /// Asset being traded.
    pub asset_id: Uuid,
    /// Kind of transaction.
    pub transaction_type: TransactionType,
    /// Trade date (no timezone).
    pub date: NaiveDate,
    /// Settlement date, if applicable.
    pub settlement_date: Option<NaiveDate>,
    /// Number of units. `None` for Dividend/Coupon.
    pub quantity: Option<Decimal>,
    /// Price per unit. `None` for Dividend/Coupon.
    pub unit_price: Option<Decimal>,
    /// Brokerage commission. Defaults to 0.
    pub commission: Decimal,
    /// Currency of the transaction amounts.
    pub currency: String,
    /// Exchange rate to the portfolio's base currency at trade time.
    pub exchange_rate_to_base: Decimal,
    /// Gross distribution amount (Dividend/Coupon).
    pub gross_amount: Option<Decimal>,
    /// Tax withheld at source (Dividend/Coupon).
    pub tax_withheld: Option<Decimal>,
    /// Net distribution after tax (Dividend/Coupon).
    pub net_amount: Option<Decimal>,
    /// Free-form user notes.
    pub notes: Option<String>,
    /// SHA-256 hash used for broker-import duplicate detection.
    pub import_hash: Option<String>,
    /// When the transaction record was created.
    pub created_at: DateTime<Utc>,
    /// When the transaction record was last updated.
    pub updated_at: DateTime<Utc>,
}

/// Data needed to record a new transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewTransaction {
    /// Portfolio this transaction belongs to.
    pub portfolio_id: Uuid,
    /// Asset being traded.
    pub asset_id: Uuid,
    /// Kind of transaction.
    pub transaction_type: TransactionType,
    /// Trade date.
    pub date: NaiveDate,
    /// Settlement date, if applicable.
    pub settlement_date: Option<NaiveDate>,
    /// Number of units. `None` for Dividend/Coupon.
    pub quantity: Option<Decimal>,
    /// Price per unit. `None` for Dividend/Coupon.
    pub unit_price: Option<Decimal>,
    /// Brokerage commission.
    pub commission: Decimal,
    /// Currency of the transaction amounts.
    pub currency: String,
    /// Exchange rate to the portfolio's base currency.
    pub exchange_rate_to_base: Decimal,
    /// Gross distribution amount (Dividend/Coupon).
    pub gross_amount: Option<Decimal>,
    /// Tax withheld at source (Dividend/Coupon).
    pub tax_withheld: Option<Decimal>,
    /// Net distribution after tax (Dividend/Coupon).
    pub net_amount: Option<Decimal>,
    /// Free-form user notes.
    pub notes: Option<String>,
    /// SHA-256 hash for broker-import duplicate detection.
    pub import_hash: Option<String>,
}

/// Supported filters for transaction listings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransactionFilters {
    /// Restrict results to a specific transaction type.
    pub transaction_type: Option<TransactionType>,
    /// Restrict results to a specific asset.
    pub asset_id: Option<Uuid>,
    /// Inclusive lower bound for the trade date.
    pub date_from: Option<NaiveDate>,
    /// Inclusive upper bound for the trade date.
    pub date_to: Option<NaiveDate>,
}

/// Mutable transaction fields accepted by update operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTransaction {
    /// Asset being traded.
    pub asset_id: Uuid,
    /// Kind of transaction.
    pub transaction_type: TransactionType,
    /// Trade date.
    pub date: NaiveDate,
    /// Settlement date, if applicable.
    pub settlement_date: Option<NaiveDate>,
    /// Number of units. `None` for Dividend/Coupon.
    pub quantity: Option<Decimal>,
    /// Price per unit. `None` for Dividend/Coupon.
    pub unit_price: Option<Decimal>,
    /// Brokerage commission.
    pub commission: Decimal,
    /// Currency of the transaction amounts.
    pub currency: String,
    /// Exchange rate to the portfolio's base currency.
    pub exchange_rate_to_base: Decimal,
    /// Gross distribution amount (Dividend/Coupon).
    pub gross_amount: Option<Decimal>,
    /// Tax withheld at source (Dividend/Coupon).
    pub tax_withheld: Option<Decimal>,
    /// Net distribution after tax (Dividend/Coupon).
    pub net_amount: Option<Decimal>,
    /// Free-form user notes.
    pub notes: Option<String>,
}
