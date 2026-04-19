//! Portfolio analytics use cases.

use std::sync::Arc;

use domain::logic::AllocationEntry;
use domain::{
    AssetClass, ExchangeRateRepository, PortfolioRepository, PositionRepository, RepositoryError,
    calculate_allocation, calculate_gain_loss,
};
use rust_decimal::Decimal;
use uuid::Uuid;

const PERCENTAGE_SCALE: u32 = 8;

/// One asset-class slice in the portfolio allocation summary.
#[derive(Debug, Clone)]
pub struct AssetAllocationSummary {
    /// Asset class represented by this slice.
    pub asset_class: AssetClass,
    /// Total current value for the class in EUR.
    pub value: Decimal,
    /// Percentage share of the total portfolio value.
    pub percentage: Decimal,
}

/// Calculated analytics for a single portfolio.
#[derive(Debug, Clone)]
pub struct PortfolioSummary {
    /// Total portfolio value in EUR.
    pub total_value: Option<Decimal>,
    /// Absolute gain/loss in EUR.
    pub total_gain_loss_absolute: Option<Decimal>,
    /// Gain/loss percentage relative to the EUR cost basis.
    pub total_gain_loss_percentage: Option<Decimal>,
    /// Allocation grouped by asset class.
    pub asset_allocation: Vec<AssetAllocationSummary>,
    /// Non-fatal issues encountered while building the summary.
    pub warnings: Vec<String>,
}

/// Errors that can occur while computing portfolio analytics.
#[derive(Debug, thiserror::Error)]
pub enum AnalyticsError {
    /// Portfolio not found, or it belongs to another user.
    #[error("portfolio not found")]
    NotFound,
    /// Repository failure.
    #[error("repository error: {0}")]
    Repository(#[from] RepositoryError),
}

/// Application service for portfolio analytics.
#[derive(Clone)]
pub struct AnalyticsService {
    portfolio_repo: Arc<dyn PortfolioRepository>,
    position_repo: Arc<dyn PositionRepository>,
    exchange_rate_repo: Arc<dyn ExchangeRateRepository>,
}

impl AnalyticsService {
    /// Create a new analytics service.
    pub fn new(
        portfolio_repo: Arc<dyn PortfolioRepository>,
        position_repo: Arc<dyn PositionRepository>,
        exchange_rate_repo: Arc<dyn ExchangeRateRepository>,
    ) -> Self {
        Self {
            portfolio_repo,
            position_repo,
            exchange_rate_repo,
        }
    }

    /// Build the portfolio summary after verifying ownership.
    pub async fn summary(
        &self,
        user_id: Uuid,
        portfolio_id: Uuid,
    ) -> Result<PortfolioSummary, AnalyticsError> {
        self.portfolio_repo
            .find_by_id(portfolio_id)
            .await?
            .filter(|portfolio| portfolio.user_id == user_id)
            .ok_or(AnalyticsError::NotFound)?;

        let positions = self.position_repo.list_by_portfolio(portfolio_id).await?;
        let open_positions = positions.into_iter().filter(|position| !position.closed);

        let mut warnings = Vec::new();
        let mut allocation_entries = Vec::new();
        let mut total_value = Decimal::ZERO;
        let mut total_cost_basis = Decimal::ZERO;
        let mut valued_positions = 0_usize;

        for position in open_positions {
            let Some(current_value) = position.current_value else {
                warnings.push(format!(
                    "Missing current price for asset {} ({})",
                    position.asset.isin, position.asset.name
                ));
                continue;
            };

            let rate_to_eur = self
                .resolve_rate_to_eur(&position.asset.currency)
                .await?
                .ok_or_else(|| {
                    warnings.push(format!(
                        "Missing EUR exchange rate for asset {} ({}) in {}",
                        position.asset.isin, position.asset.name, position.asset.currency
                    ));
                });

            let Ok(rate_to_eur) = rate_to_eur else {
                continue;
            };

            let current_value_eur = current_value * rate_to_eur;
            let cost_basis_eur = position.quantity * position.average_cost * rate_to_eur;

            total_value += current_value_eur;
            total_cost_basis += cost_basis_eur;
            valued_positions += 1;
            allocation_entries.push(AllocationEntry {
                asset_class: position.asset.asset_class,
                value: current_value_eur,
            });
        }

        if valued_positions == 0 {
            return Ok(if warnings.is_empty() {
                PortfolioSummary {
                    total_value: Some(Decimal::ZERO),
                    total_gain_loss_absolute: Some(Decimal::ZERO),
                    total_gain_loss_percentage: Some(Decimal::ZERO),
                    asset_allocation: Vec::new(),
                    warnings,
                }
            } else {
                PortfolioSummary {
                    total_value: None,
                    total_gain_loss_absolute: None,
                    total_gain_loss_percentage: None,
                    asset_allocation: Vec::new(),
                    warnings,
                }
            });
        }

        let gain_loss = calculate_gain_loss(Decimal::ONE, total_cost_basis, total_value).ok();
        let asset_allocation =
            build_allocation_summaries(&allocation_entries, total_value, PERCENTAGE_SCALE);

        Ok(PortfolioSummary {
            total_value: Some(total_value),
            total_gain_loss_absolute: gain_loss.as_ref().map(|value| value.absolute),
            total_gain_loss_percentage: gain_loss.map(|value| value.percentage),
            asset_allocation,
            warnings,
        })
    }

    async fn resolve_rate_to_eur(
        &self,
        currency: &str,
    ) -> Result<Option<Decimal>, RepositoryError> {
        if currency.eq_ignore_ascii_case("EUR") {
            return Ok(Some(Decimal::ONE));
        }

        Ok(self
            .exchange_rate_repo
            .find_latest(&currency.to_ascii_uppercase(), "EUR")
            .await?
            .map(|rate| rate.rate))
    }
}

fn build_allocation_summaries(
    entries: &[AllocationEntry],
    total_value: Decimal,
    percentage_scale: u32,
) -> Vec<AssetAllocationSummary> {
    let percentages = calculate_allocation(entries);
    let mut values_by_class = Vec::<(AssetClass, Decimal)>::new();

    for entry in entries {
        if let Some(existing) = values_by_class
            .iter_mut()
            .find(|(asset_class, _)| *asset_class == entry.asset_class)
        {
            existing.1 += entry.value;
        } else {
            values_by_class.push((entry.asset_class, entry.value));
        }
    }

    values_by_class.sort_by_key(|(asset_class, _)| asset_class.as_str());

    let mut allocation = values_by_class
        .into_iter()
        .map(|(asset_class, value)| {
            let percentage = percentages
                .iter()
                .find(|entry| entry.asset_class == asset_class)
                .map(|entry| entry.percentage.round_dp(percentage_scale))
                .unwrap_or(Decimal::ZERO);

            AssetAllocationSummary {
                asset_class,
                value,
                percentage,
            }
        })
        .collect::<Vec<_>>();

    let rounded_total: Decimal = allocation.iter().map(|entry| entry.percentage).sum();
    if let Some(last) = allocation.last_mut() {
        last.percentage += Decimal::ONE_HUNDRED - rounded_total;
    }

    debug_assert_eq!(
        allocation.iter().map(|entry| entry.value).sum::<Decimal>(),
        total_value
    );

    allocation
}

#[cfg(test)]
mod tests {
    use domain::logic::AllocationEntry;

    use super::*;

    #[test]
    fn test_build_allocation_summaries_keeps_percentages_at_100() {
        let summaries = build_allocation_summaries(
            &[
                AllocationEntry {
                    asset_class: AssetClass::Stock,
                    value: Decimal::ONE,
                },
                AllocationEntry {
                    asset_class: AssetClass::Bond,
                    value: Decimal::ONE,
                },
                AllocationEntry {
                    asset_class: AssetClass::Commodity,
                    value: Decimal::ONE,
                },
            ],
            Decimal::from(3),
            8,
        );

        assert_eq!(summaries.len(), 3);
        assert_eq!(
            summaries
                .iter()
                .map(|entry| entry.percentage)
                .sum::<Decimal>(),
            Decimal::ONE_HUNDRED
        );
    }
}
