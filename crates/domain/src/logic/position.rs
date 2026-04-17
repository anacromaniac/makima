//! Position aggregation from transaction lists.

use rust_decimal::Decimal;

use crate::error::DomainError;
use crate::models::{Transaction, TransactionType};

/// Aggregated position derived from a sequence of buy/sell transactions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    /// Total quantity currently held.
    pub quantity: Decimal,
    /// Volume-weighted average cost per unit.
    pub average_cost: Decimal,
    /// Whether the position is closed (quantity is zero).
    pub closed: bool,
}

/// Aggregate a list of transactions into a single [`Position`].
///
/// Only `Buy` and `Sell` transactions are considered; `Dividend` and `Coupon`
/// entries are ignored. The average cost is computed using the weighted-average
/// method: buys increase the cost basis, sells reduce quantity but do not change
/// the per-unit average.
///
/// # Errors
///
/// Returns [`DomainError::InsufficientQuantity`] if a sell would drive the
/// held quantity below zero (short-selling is not allowed).
///
/// Returns [`DomainError::ValidationError`] if the input list is empty.
pub fn aggregate_position(transactions: &[Transaction]) -> Result<Position, DomainError> {
    if transactions.is_empty() {
        return Err(DomainError::ValidationError(
            "cannot aggregate an empty transaction list".into(),
        ));
    }

    let mut quantity = Decimal::ZERO;
    let mut total_cost = Decimal::ZERO;

    for tx in transactions {
        if tx.transaction_type == TransactionType::Dividend
            || tx.transaction_type == TransactionType::Coupon
        {
            continue;
        }

        let tx_qty = tx.quantity.unwrap_or(Decimal::ZERO);
        let tx_price = tx.unit_price.unwrap_or(Decimal::ZERO);

        match tx.transaction_type {
            TransactionType::Buy => {
                total_cost += tx_qty * tx_price;
                quantity += tx_qty;
            }
            TransactionType::Sell => {
                if tx_qty > quantity {
                    return Err(DomainError::InsufficientQuantity {
                        available: quantity,
                        requested: tx_qty,
                    });
                }
                // Weighted-average method: sell reduces quantity but keeps
                // the per-unit average cost unchanged.
                total_cost -= tx_qty * (total_cost / quantity);
                quantity -= tx_qty;
            }
            _ => unreachable!("dividend/coupon already skipped"),
        }
    }

    let average_cost = if quantity > Decimal::ZERO {
        total_cost / quantity
    } else {
        Decimal::ZERO
    };

    Ok(Position {
        quantity,
        average_cost,
        closed: quantity == Decimal::ZERO,
    })
}

#[cfg(test)]
mod tests {
    use chrono::{NaiveDate, Utc};
    use rust_decimal::Decimal;
    use uuid::Uuid;

    use super::*;
    use crate::models::TransactionType;

    fn buy(quantity: Decimal, unit_price: Decimal) -> Transaction {
        Transaction {
            id: Uuid::now_v7(),
            portfolio_id: Uuid::now_v7(),
            asset_id: Uuid::now_v7(),
            transaction_type: TransactionType::Buy,
            date: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            settlement_date: None,
            quantity: Some(quantity),
            unit_price: Some(unit_price),
            commission: Decimal::ZERO,
            currency: "EUR".into(),
            exchange_rate_to_base: Decimal::ONE,
            gross_amount: None,
            tax_withheld: None,
            net_amount: None,
            notes: None,
            import_hash: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn sell(quantity: Decimal, unit_price: Decimal) -> Transaction {
        Transaction {
            id: Uuid::now_v7(),
            portfolio_id: Uuid::now_v7(),
            asset_id: Uuid::now_v7(),
            transaction_type: TransactionType::Sell,
            date: NaiveDate::from_ymd_opt(2025, 1, 2).unwrap(),
            settlement_date: None,
            quantity: Some(quantity),
            unit_price: Some(unit_price),
            commission: Decimal::ZERO,
            currency: "EUR".into(),
            exchange_rate_to_base: Decimal::ONE,
            gross_amount: None,
            tax_withheld: None,
            net_amount: None,
            notes: None,
            import_hash: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_aggregate_buy_only_returns_correct_quantity_and_average() {
        let txs = vec![
            buy(Decimal::new(10, 0), Decimal::new(100, 0)),
            buy(Decimal::new(20, 0), Decimal::new(150, 0)),
        ];
        let pos = aggregate_position(&txs).unwrap();
        assert_eq!(pos.quantity, Decimal::new(30, 0));
        // total cost = 10*100 + 20*150 = 4000, avg = 4000/30 ≈ 133.33
        assert_eq!(
            pos.average_cost,
            Decimal::new(4000, 0) / Decimal::new(30, 0)
        );
        assert!(!pos.closed);
    }

    #[test]
    fn test_aggregate_buy_then_sell_reduces_quantity() {
        let txs = vec![
            buy(Decimal::new(100, 0), Decimal::new(50, 0)),
            sell(Decimal::new(40, 0), Decimal::new(60, 0)),
        ];
        let pos = aggregate_position(&txs).unwrap();
        assert_eq!(pos.quantity, Decimal::new(60, 0));
        // Average cost unchanged at 50
        assert_eq!(pos.average_cost, Decimal::new(50, 0));
        assert!(!pos.closed);
    }

    #[test]
    fn test_aggregate_sell_to_zero_is_closed_position() {
        let txs = vec![
            buy(Decimal::new(10, 0), Decimal::new(100, 0)),
            sell(Decimal::new(10, 0), Decimal::new(120, 0)),
        ];
        let pos = aggregate_position(&txs).unwrap();
        assert_eq!(pos.quantity, Decimal::ZERO);
        assert_eq!(pos.average_cost, Decimal::ZERO);
        assert!(pos.closed);
    }

    #[test]
    fn test_aggregate_sell_exceeding_quantity_returns_error() {
        let txs = vec![
            buy(Decimal::new(5, 0), Decimal::new(100, 0)),
            sell(Decimal::new(10, 0), Decimal::new(110, 0)),
        ];
        let err = aggregate_position(&txs).unwrap_err();
        assert!(matches!(
            err,
            DomainError::InsufficientQuantity {
                available,
                requested,
            } if available == Decimal::new(5, 0) && requested == Decimal::new(10, 0)
        ));
    }

    #[test]
    fn test_aggregate_empty_input_returns_error() {
        let err = aggregate_position(&[]).unwrap_err();
        assert!(matches!(err, DomainError::ValidationError(_)));
    }

    #[test]
    fn test_aggregate_ignores_dividends_and_coupons() {
        let dividend = Transaction {
            id: Uuid::now_v7(),
            portfolio_id: Uuid::now_v7(),
            asset_id: Uuid::now_v7(),
            transaction_type: TransactionType::Dividend,
            date: NaiveDate::from_ymd_opt(2025, 3, 1).unwrap(),
            settlement_date: None,
            quantity: None,
            unit_price: None,
            commission: Decimal::ZERO,
            currency: "EUR".into(),
            exchange_rate_to_base: Decimal::ONE,
            gross_amount: Some(Decimal::new(50, 0)),
            tax_withheld: Some(Decimal::new(10, 0)),
            net_amount: Some(Decimal::new(40, 0)),
            notes: None,
            import_hash: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let txs = vec![buy(Decimal::new(10, 0), Decimal::new(100, 0)), dividend];
        let pos = aggregate_position(&txs).unwrap();
        assert_eq!(pos.quantity, Decimal::new(10, 0));
        assert_eq!(pos.average_cost, Decimal::new(100, 0));
    }
}
