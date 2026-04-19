//! Request and response DTOs for position endpoints.

use domain::Position;
use rust_decimal::Decimal;
use serde::Serialize;
use uuid::Uuid;

use crate::assets::dto::AssetResponse;

/// A derived position returned by the API.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct PositionResponse {
    /// Asset identifier.
    pub asset_id: Uuid,
    /// Shared asset details.
    pub asset: AssetResponse,
    /// Quantity currently held.
    #[schema(value_type = String)]
    pub quantity: Decimal,
    /// Weighted-average cost per unit.
    #[schema(value_type = String)]
    pub average_cost: Decimal,
    /// Latest known market price, if available.
    #[schema(value_type = Option<String>)]
    pub current_price: Option<Decimal>,
    /// Current market value, if available.
    #[schema(value_type = Option<String>)]
    pub current_value: Option<Decimal>,
    /// Absolute gain/loss, if available.
    #[schema(value_type = Option<String>)]
    pub gain_loss_absolute: Option<Decimal>,
    /// Percentage gain/loss, if available.
    #[schema(value_type = Option<String>)]
    pub gain_loss_percentage: Option<Decimal>,
    /// Whether the position is closed.
    pub closed: bool,
}

impl From<Position> for PositionResponse {
    fn from(value: Position) -> Self {
        Self {
            asset_id: value.asset.id,
            asset: AssetResponse::from(value.asset),
            quantity: value.quantity,
            average_cost: value.average_cost,
            current_price: value.current_price,
            current_value: value.current_value,
            gain_loss_absolute: value.gain_loss_absolute,
            gain_loss_percentage: value.gain_loss_percentage,
            closed: value.closed,
        }
    }
}
