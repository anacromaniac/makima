//! Derived position model.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::models::Asset;

/// A derived portfolio position computed from the transaction history of one asset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    /// Shared asset reference data for the position.
    pub asset: Asset,
    /// Quantity currently held.
    pub quantity: Decimal,
    /// Weighted-average cost per unit in the asset currency.
    pub average_cost: Decimal,
    /// Latest known market price, if available.
    pub current_price: Option<Decimal>,
    /// Current market value, if a price is available.
    pub current_value: Option<Decimal>,
    /// Absolute gain/loss, if a price is available and the position has a non-zero basis.
    pub gain_loss_absolute: Option<Decimal>,
    /// Percentage gain/loss, if a price is available and the position has a non-zero basis.
    pub gain_loss_percentage: Option<Decimal>,
    /// Whether the position is closed.
    pub closed: bool,
}
