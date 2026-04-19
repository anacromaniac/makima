//! Request and response DTOs for transaction endpoints.

use chrono::{DateTime, NaiveDate, Utc};
use domain::TransactionType;
use garde::Validate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// API-facing transaction type enum used by request, query, and response DTOs.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "PascalCase")]
pub enum ApiTransactionType {
    /// Purchase of an asset.
    Buy,
    /// Sale of an asset.
    Sell,
    /// Cash distribution from a stock or fund.
    Dividend,
    /// Interest payment from a bond.
    Coupon,
}

impl From<ApiTransactionType> for TransactionType {
    fn from(value: ApiTransactionType) -> Self {
        match value {
            ApiTransactionType::Buy => Self::Buy,
            ApiTransactionType::Sell => Self::Sell,
            ApiTransactionType::Dividend => Self::Dividend,
            ApiTransactionType::Coupon => Self::Coupon,
        }
    }
}

impl From<TransactionType> for ApiTransactionType {
    fn from(value: TransactionType) -> Self {
        match value {
            TransactionType::Buy => Self::Buy,
            TransactionType::Sell => Self::Sell,
            TransactionType::Dividend => Self::Dividend,
            TransactionType::Coupon => Self::Coupon,
        }
    }
}

/// Request body for `POST /api/v1/portfolios/{id}/transactions`.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
pub struct CreateTransactionRequest {
    /// International Securities Identification Number.
    #[garde(custom(application::assets::is_valid_isin))]
    pub asset_isin: String,
    /// Transaction kind.
    #[garde(skip)]
    pub transaction_type: ApiTransactionType,
    /// Trade date.
    #[garde(skip)]
    pub date: NaiveDate,
    /// Settlement date.
    #[garde(skip)]
    pub settlement_date: Option<NaiveDate>,
    /// Quantity for buy/sell operations.
    #[schema(value_type = Option<String>)]
    #[garde(custom(validate_positive_decimal_option))]
    pub quantity: Option<Decimal>,
    /// Unit price for buy/sell operations.
    #[schema(value_type = Option<String>)]
    #[garde(custom(validate_positive_decimal_option))]
    pub unit_price: Option<Decimal>,
    /// Brokerage commission.
    #[schema(value_type = Option<String>)]
    #[garde(skip)]
    pub commission: Option<Decimal>,
    /// Transaction currency.
    #[garde(length(min = 3, max = 10))]
    pub currency: String,
    /// Optional exchange rate to EUR provided by the caller.
    #[schema(value_type = Option<String>)]
    #[garde(custom(validate_positive_decimal_option))]
    pub exchange_rate_to_base: Option<Decimal>,
    /// Gross distribution amount for dividend/coupon operations.
    #[schema(value_type = Option<String>)]
    #[garde(custom(validate_positive_decimal_option))]
    pub gross_amount: Option<Decimal>,
    /// Withheld tax for dividend/coupon operations.
    #[schema(value_type = Option<String>)]
    #[garde(skip)]
    pub tax_withheld: Option<Decimal>,
    /// Net distribution amount for dividend/coupon operations.
    #[schema(value_type = Option<String>)]
    #[garde(custom(validate_positive_decimal_option))]
    pub net_amount: Option<Decimal>,
    /// Optional notes.
    #[garde(skip)]
    pub notes: Option<String>,
}

/// Request body for `PUT /api/v1/transactions/{id}`.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
pub struct UpdateTransactionRequest {
    /// International Securities Identification Number.
    #[garde(custom(application::assets::is_valid_isin))]
    pub asset_isin: String,
    /// Transaction kind.
    #[garde(skip)]
    pub transaction_type: ApiTransactionType,
    /// Trade date.
    #[garde(skip)]
    pub date: NaiveDate,
    /// Settlement date.
    #[garde(skip)]
    pub settlement_date: Option<NaiveDate>,
    /// Quantity for buy/sell operations.
    #[schema(value_type = Option<String>)]
    #[garde(custom(validate_positive_decimal_option))]
    pub quantity: Option<Decimal>,
    /// Unit price for buy/sell operations.
    #[schema(value_type = Option<String>)]
    #[garde(custom(validate_positive_decimal_option))]
    pub unit_price: Option<Decimal>,
    /// Brokerage commission.
    #[schema(value_type = Option<String>)]
    #[garde(skip)]
    pub commission: Option<Decimal>,
    /// Transaction currency.
    #[garde(length(min = 3, max = 10))]
    pub currency: String,
    /// Optional exchange rate to EUR provided by the caller.
    #[schema(value_type = Option<String>)]
    #[garde(custom(validate_positive_decimal_option))]
    pub exchange_rate_to_base: Option<Decimal>,
    /// Gross distribution amount for dividend/coupon operations.
    #[schema(value_type = Option<String>)]
    #[garde(custom(validate_positive_decimal_option))]
    pub gross_amount: Option<Decimal>,
    /// Withheld tax for dividend/coupon operations.
    #[schema(value_type = Option<String>)]
    #[garde(skip)]
    pub tax_withheld: Option<Decimal>,
    /// Net distribution amount for dividend/coupon operations.
    #[schema(value_type = Option<String>)]
    #[garde(custom(validate_positive_decimal_option))]
    pub net_amount: Option<Decimal>,
    /// Optional notes.
    #[garde(skip)]
    pub notes: Option<String>,
}

/// Transaction returned by the API.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct TransactionResponse {
    /// Unique transaction identifier.
    pub id: Uuid,
    /// Owning portfolio identifier.
    pub portfolio_id: Uuid,
    /// Asset identifier.
    pub asset_id: Uuid,
    /// Asset ISIN.
    pub asset_isin: String,
    /// Asset name.
    pub asset_name: String,
    /// Transaction kind.
    pub transaction_type: ApiTransactionType,
    /// Trade date.
    pub date: NaiveDate,
    /// Settlement date.
    pub settlement_date: Option<NaiveDate>,
    /// Quantity for buy/sell operations.
    #[schema(value_type = Option<String>)]
    pub quantity: Option<Decimal>,
    /// Unit price for buy/sell operations.
    #[schema(value_type = Option<String>)]
    pub unit_price: Option<Decimal>,
    /// Brokerage commission.
    #[schema(value_type = String)]
    pub commission: Decimal,
    /// Transaction currency.
    pub currency: String,
    /// Exchange rate to EUR.
    #[schema(value_type = String)]
    pub exchange_rate_to_base: Decimal,
    /// Gross distribution amount for dividend/coupon operations.
    #[schema(value_type = Option<String>)]
    pub gross_amount: Option<Decimal>,
    /// Withheld tax for dividend/coupon operations.
    #[schema(value_type = Option<String>)]
    pub tax_withheld: Option<Decimal>,
    /// Net distribution amount for dividend/coupon operations.
    #[schema(value_type = Option<String>)]
    pub net_amount: Option<Decimal>,
    /// Optional notes.
    pub notes: Option<String>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

impl From<application::transactions::TransactionDetails> for TransactionResponse {
    fn from(value: application::transactions::TransactionDetails) -> Self {
        Self {
            id: value.transaction.id,
            portfolio_id: value.transaction.portfolio_id,
            asset_id: value.transaction.asset_id,
            asset_isin: value.asset_isin,
            asset_name: value.asset_name,
            transaction_type: value.transaction.transaction_type.into(),
            date: value.transaction.date,
            settlement_date: value.transaction.settlement_date,
            quantity: value.transaction.quantity,
            unit_price: value.transaction.unit_price,
            commission: value.transaction.commission,
            currency: value.transaction.currency,
            exchange_rate_to_base: value.transaction.exchange_rate_to_base,
            gross_amount: value.transaction.gross_amount,
            tax_withheld: value.transaction.tax_withheld,
            net_amount: value.transaction.net_amount,
            notes: value.transaction.notes,
            created_at: value.transaction.created_at,
            updated_at: value.transaction.updated_at,
        }
    }
}

/// Validate conditional transaction fields.
pub fn validate_transaction_request(
    transaction_type: ApiTransactionType,
    quantity: Option<Decimal>,
    unit_price: Option<Decimal>,
    gross_amount: Option<Decimal>,
    net_amount: Option<Decimal>,
) -> Result<(), String> {
    match transaction_type {
        ApiTransactionType::Buy | ApiTransactionType::Sell => {
            if quantity.is_none() || unit_price.is_none() {
                return Err("Buy and Sell transactions require quantity and unit_price".to_string());
            }
        }
        ApiTransactionType::Dividend | ApiTransactionType::Coupon => {
            if gross_amount.is_none() || net_amount.is_none() {
                return Err(
                    "Dividend and Coupon transactions require gross_amount and net_amount"
                        .to_string(),
                );
            }
        }
    }

    Ok(())
}

fn validate_positive_decimal_option(value: &Option<Decimal>, _: &()) -> garde::Result {
    if value.is_some_and(|value| value <= Decimal::ZERO) {
        return Err(garde::Error::new("value must be greater than zero"));
    }

    Ok(())
}
