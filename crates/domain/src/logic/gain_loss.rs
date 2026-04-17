//! Gain/loss calculation for positions.

use rust_decimal::Decimal;

use crate::error::DomainError;

/// The result of a gain/loss calculation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GainLoss {
    /// Absolute gain or loss (positive = profit, negative = loss).
    pub absolute: Decimal,
    /// Percentage gain or loss relative to cost basis.
    pub percentage: Decimal,
}

/// Calculate gain/loss given held quantity, average cost, and current price.
///
/// The cost basis is `quantity * average_cost`. The current value is
/// `quantity * current_price`. Percentage is relative to the cost basis.
///
/// # Errors
///
/// Returns the same `Decimal` values back as an `Err` if the average cost or
/// quantity is zero (no basis for a percentage calculation). The caller can
/// decide how to handle this.
pub fn calculate_gain_loss(
    quantity: Decimal,
    average_cost: Decimal,
    current_price: Decimal,
) -> Result<GainLoss, DomainError> {
    if quantity == Decimal::ZERO || average_cost == Decimal::ZERO {
        return Err(DomainError::ValidationError(
            "quantity and average cost must be non-zero for gain/loss calculation".into(),
        ));
    }

    let cost_basis = quantity * average_cost;
    let current_value = quantity * current_price;
    let absolute = current_value - cost_basis;
    let percentage = (absolute / cost_basis) * Decimal::ONE_HUNDRED;

    Ok(GainLoss {
        absolute,
        percentage,
    })
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::*;

    #[test]
    fn test_gain_loss_profit_case() {
        let gl = calculate_gain_loss(
            Decimal::new(10, 0),
            Decimal::new(100, 0),
            Decimal::new(120, 0),
        )
        .unwrap();
        assert_eq!(gl.absolute, Decimal::new(200, 0)); // (120-100)*10
        assert_eq!(gl.percentage, Decimal::new(20, 0)); // 20%
    }

    #[test]
    fn test_gain_loss_loss_case() {
        let gl = calculate_gain_loss(
            Decimal::new(10, 0),
            Decimal::new(100, 0),
            Decimal::new(80, 0),
        )
        .unwrap();
        assert_eq!(gl.absolute, Decimal::new(-200, 0)); // (80-100)*10
        assert_eq!(gl.percentage, Decimal::new(-20, 0)); // -20%
    }

    #[test]
    fn test_gain_loss_zero_gain() {
        let gl = calculate_gain_loss(
            Decimal::new(10, 0),
            Decimal::new(100, 0),
            Decimal::new(100, 0),
        )
        .unwrap();
        assert_eq!(gl.absolute, Decimal::ZERO);
        assert_eq!(gl.percentage, Decimal::ZERO);
    }

    #[test]
    fn test_gain_loss_zero_quantity_returns_err() {
        assert!(
            calculate_gain_loss(Decimal::ZERO, Decimal::new(100, 0), Decimal::new(120, 0)).is_err()
        );
    }

    #[test]
    fn test_gain_loss_zero_average_cost_returns_err() {
        assert!(
            calculate_gain_loss(Decimal::new(10, 0), Decimal::ZERO, Decimal::new(120, 0)).is_err()
        );
    }
}
