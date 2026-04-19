//! Yahoo Finance exchange-rate fetching utilities.

use chrono::Utc;
use domain::{DomainError, ExchangeRate};
use uuid::Uuid;

use crate::yahoo::YahooFinanceClient;

/// Fetches current FX rates from Yahoo Finance.
#[derive(Clone)]
pub struct YahooExchangeRateFetcher {
    client: YahooFinanceClient,
}

impl YahooExchangeRateFetcher {
    /// Create a new fetcher backed by the provided Yahoo client.
    pub fn new(client: YahooFinanceClient) -> Self {
        Self { client }
    }

    /// Fetch the current FX rate for a currency pair.
    pub async fn fetch_rate(
        &self,
        from_currency: &str,
        to_currency: &str,
    ) -> Result<ExchangeRate, DomainError> {
        let normalized_from = from_currency.to_ascii_uppercase();
        let normalized_to = to_currency.to_ascii_uppercase();
        let rate = self
            .client
            .lookup_exchange_rate(&normalized_from, &normalized_to)
            .await
            .ok_or_else(|| {
                DomainError::ExternalServiceError(format!(
                    "Yahoo returned no FX rate for {normalized_from}/{normalized_to}"
                ))
            })?;

        Ok(ExchangeRate {
            id: Uuid::now_v7(),
            from_currency: normalized_from,
            to_currency: normalized_to,
            date: Utc::now().date_naive(),
            rate,
        })
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::YahooExchangeRateFetcher;
    use crate::yahoo::YahooFinanceClient;

    #[tokio::test]
    async fn test_fetch_rate_returns_one_for_same_currency() {
        let client = YahooFinanceClient::new("http://localhost", 0).expect("client should build");
        let fetcher = YahooExchangeRateFetcher::new(client);

        let rate = fetcher
            .fetch_rate("eur", "eur")
            .await
            .expect("same-currency lookup should succeed");

        assert_eq!(rate.from_currency, "EUR");
        assert_eq!(rate.to_currency, "EUR");
        assert_eq!(rate.rate, Decimal::ONE);
    }
}
