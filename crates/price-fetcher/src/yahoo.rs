//! Yahoo Finance client for exchange-rate lookups.

use rust_decimal::Decimal;
use serde::Deserialize;

const YAHOO_QUOTE_URL: &str = "https://query1.finance.yahoo.com/v7/finance/quote";

/// HTTP client for Yahoo Finance quote requests.
#[derive(Clone)]
pub struct YahooFinanceClient {
    http: reqwest::Client,
    base_url: String,
}

impl YahooFinanceClient {
    /// Build a client using environment-based configuration.
    ///
    /// `YAHOO_FINANCE_BASE_URL` is intended for tests and defaults to the public API.
    pub fn from_env() -> Result<Self, reqwest::Error> {
        let base_url =
            std::env::var("YAHOO_FINANCE_BASE_URL").unwrap_or_else(|_| YAHOO_QUOTE_URL.to_string());
        Self::new(base_url)
    }

    /// Build a client with an explicit quote endpoint base URL.
    pub fn new(base_url: impl Into<String>) -> Result<Self, reqwest::Error> {
        let http = reqwest::Client::builder().build()?;
        Ok(Self {
            http,
            base_url: base_url.into(),
        })
    }

    /// Resolve the latest available FX rate from `from_currency` to `to_currency`.
    pub async fn lookup_exchange_rate(
        &self,
        from_currency: &str,
        to_currency: &str,
    ) -> Option<Decimal> {
        if from_currency.eq_ignore_ascii_case(to_currency) {
            return Some(Decimal::ONE);
        }

        let symbol = format!(
            "{}{}=X",
            from_currency.to_ascii_uppercase(),
            to_currency.to_ascii_uppercase()
        );

        let response = match self
            .http
            .get(&self.base_url)
            .query(&[("symbols", symbol.as_str())])
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                tracing::warn!("Yahoo Finance FX request failed for {symbol}: {error}");
                return None;
            }
        };

        if !response.status().is_success() {
            tracing::warn!(
                "Yahoo Finance returned status {} for FX symbol {symbol}",
                response.status()
            );
            return None;
        }

        let payload = match response.json::<QuoteResponse>().await {
            Ok(payload) => payload,
            Err(error) => {
                tracing::warn!("Yahoo Finance FX parsing failed for {symbol}: {error}");
                return None;
            }
        };

        payload
            .quote_response
            .result
            .first()
            .and_then(|result| Decimal::from_f64_retain(result.regular_market_price))
    }
}

#[derive(Debug, Deserialize)]
struct QuoteResponse {
    #[serde(rename = "quoteResponse")]
    quote_response: QuoteResult,
}

#[derive(Debug, Deserialize)]
struct QuoteResult {
    result: Vec<QuoteItem>,
}

#[derive(Debug, Deserialize)]
struct QuoteItem {
    #[serde(rename = "regularMarketPrice")]
    regular_market_price: f64,
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::YahooFinanceClient;

    #[tokio::test]
    async fn test_lookup_exchange_rate_returns_one_for_same_currency() {
        let client = YahooFinanceClient::new("http://localhost").expect("client should build");

        let rate = client.lookup_exchange_rate("EUR", "EUR").await;

        assert_eq!(rate, Some(Decimal::ONE));
    }
}
