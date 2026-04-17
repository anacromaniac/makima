//! Asset allocation percentage calculation.

use rust_decimal::Decimal;

use crate::models::AssetClass;

/// A single position's contribution to asset allocation.
pub struct AllocationEntry {
    /// Asset class of the position.
    pub asset_class: AssetClass,
    /// Current market value of the position in base currency.
    pub value: Decimal,
}

/// Allocation percentage for a single asset class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllocationPercent {
    /// Asset class.
    pub asset_class: AssetClass,
    /// Percentage of total portfolio value (0–100).
    pub percentage: Decimal,
}

/// Calculate asset allocation percentages from a list of positions.
///
/// Each entry provides an asset class and a current market value. The function
/// returns the percentage share of each class relative to the total.
///
/// Returns an empty vec when the input is empty, or when the total value is
/// zero.
pub fn calculate_allocation(entries: &[AllocationEntry]) -> Vec<AllocationPercent> {
    let total: Decimal = entries.iter().map(|e| e.value).sum();

    if total == Decimal::ZERO {
        return Vec::new();
    }

    // Accumulate values per asset class using a simple linear scan.
    // For a small number of classes (≤6) a fixed-size array would also work,
    // but this approach stays correct if more classes are added later.
    let mut classes: Vec<(AssetClass, Decimal)> = Vec::new();

    for entry in entries {
        if let Some(existing) = classes.iter_mut().find(|(ac, _)| *ac == entry.asset_class) {
            existing.1 += entry.value;
        } else {
            classes.push((entry.asset_class, entry.value));
        }
    }

    classes
        .into_iter()
        .map(|(asset_class, value)| AllocationPercent {
            asset_class,
            percentage: (value / total) * Decimal::ONE_HUNDRED,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::*;
    use crate::models::AssetClass;

    fn entry(class: AssetClass, value: i32) -> AllocationEntry {
        AllocationEntry {
            asset_class: class,
            value: Decimal::from(value),
        }
    }

    #[test]
    fn test_allocation_single_class_is_100_percent() {
        let entries = vec![entry(AssetClass::Stock, 600), entry(AssetClass::Stock, 400)];
        let alloc = calculate_allocation(&entries);
        assert_eq!(alloc.len(), 1);
        assert_eq!(alloc[0].asset_class, AssetClass::Stock);
        assert_eq!(alloc[0].percentage, Decimal::ONE_HUNDRED);
    }

    #[test]
    fn test_allocation_multiple_classes() {
        let entries = vec![
            entry(AssetClass::Stock, 600),
            entry(AssetClass::Bond, 300),
            entry(AssetClass::Commodity, 100),
        ];
        let alloc = calculate_allocation(&entries);
        assert_eq!(alloc.len(), 3);

        let stock = alloc
            .iter()
            .find(|a| a.asset_class == AssetClass::Stock)
            .unwrap();
        assert_eq!(stock.percentage, Decimal::new(60, 0));

        let bond = alloc
            .iter()
            .find(|a| a.asset_class == AssetClass::Bond)
            .unwrap();
        assert_eq!(bond.percentage, Decimal::new(30, 0));

        let commodity = alloc
            .iter()
            .find(|a| a.asset_class == AssetClass::Commodity)
            .unwrap();
        assert_eq!(commodity.percentage, Decimal::new(10, 0));
    }

    #[test]
    fn test_allocation_empty_portfolio_returns_empty() {
        let alloc = calculate_allocation(&[]);
        assert!(alloc.is_empty());
    }

    #[test]
    fn test_allocation_zero_value_returns_empty() {
        let entries = vec![entry(AssetClass::Stock, 0), entry(AssetClass::Bond, 0)];
        let alloc = calculate_allocation(&entries);
        assert!(alloc.is_empty());
    }
}
