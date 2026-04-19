//! Broker import domain models.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::{AssetClass, NewAsset, NewTransaction, TransactionType};

/// A row-level validation error produced while parsing a broker export file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrokerImportRowError {
    /// One-based row number in the source file.
    pub row: u32,
    /// Human-readable validation message.
    pub message: String,
}

/// Structured parser error containing every invalid row discovered in a file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, thiserror::Error)]
#[error("broker import file contains invalid rows")]
pub struct BrokerImportParseError {
    /// Validation errors collected across the full file.
    pub row_errors: Vec<BrokerImportRowError>,
}

/// A normalized transaction parsed from a broker export before persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedBrokerTransaction {
    /// Trade date.
    pub date: NaiveDate,
    /// Settlement date, if present in the source export.
    pub settlement_date: Option<NaiveDate>,
    /// Asset ISIN used to resolve or create the shared asset.
    pub isin: String,
    /// Human-readable instrument name from the source export.
    pub asset_name: String,
    /// Best-effort asset class inferred from the source export.
    pub asset_class: Option<AssetClass>,
    /// Instrument currency from the source export.
    pub asset_currency: String,
    /// Exchange/venue from the source export, when available.
    pub exchange: Option<String>,
    /// Transaction type.
    pub transaction_type: TransactionType,
    /// Quantity for buy/sell operations.
    pub quantity: Option<Decimal>,
    /// Unit price for buy/sell operations.
    pub unit_price: Option<Decimal>,
    /// Brokerage commission.
    pub commission: Decimal,
    /// Transaction currency.
    pub currency: String,
    /// Gross distribution amount for dividend/coupon operations.
    pub gross_amount: Option<Decimal>,
    /// Tax withheld for dividend/coupon operations.
    pub tax_withheld: Option<Decimal>,
    /// Net distribution amount for dividend/coupon operations.
    pub net_amount: Option<Decimal>,
    /// Optional free-form notes preserved from the source row.
    pub notes: Option<String>,
}

/// An asset prepared for creation during a broker import transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedImportAsset {
    /// Pre-generated identifier so imported transactions can reference it.
    pub id: Uuid,
    /// Asset payload to insert.
    pub asset: NewAsset,
}

/// A broker-import transaction prepared for atomic persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedImportTransaction {
    /// Transaction payload to insert.
    pub transaction: NewTransaction,
}
