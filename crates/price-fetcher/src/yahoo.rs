//! Yahoo Finance client for asset prices and exchange-rate lookups.

use std::time::Duration;

use chrono::{DateTime, NaiveDate};
use domain::{DomainError, NewPriceRecord, PriceSource};
use reqwest::{Response, StatusCode};
use rust_decimal::Decimal;
use serde::Deserialize;
use tokio::time::sleep;
use tracing::warn;
use uuid::Uuid;

const DEFAULT_YAHOO_BASE_URL: &str = "https://query1.finance.yahoo.com";
const DEFAULT_REQUEST_DELAY_MS: u64 = 1_500;
const MAX_RETRIES: usize = 3;

/// HTTP client for Yahoo Finance quote and chart requests.
#[derive(Clone)]
pub struct YahooFinanceClient {
    http: reqwest::Client,
    base_url: String,
    request_delay: Duration,
}

impl YahooFinanceClient {
    /// Build a client using environment-based configuration.
    pub fn from_env() -> Result<Self, reqwest::Error> {
        let base_url = std::env::var("YAHOO_FINANCE_BASE_URL")
            .unwrap_or_else(|_| DEFAULT_YAHOO_BASE_URL.to_string());
        let request_delay = std::env::var("YAHOO_REQUEST_DELAY_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(DEFAULT_REQUEST_DELAY_MS);

        Self::new(base_url, request_delay)
    }

    /// Build a client with an explicit base URL and inter-request delay.
    pub fn new(base_url: impl Into<String>, request_delay_ms: u64) -> Result<Self, reqwest::Error> {
        let http = reqwest::Client::builder().build()?;
        Ok(Self {
            http,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            request_delay: Duration::from_millis(request_delay_ms),
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

        let payload = match self
            .fetch_quote_response(std::slice::from_ref(&symbol))
            .await
        {
            Ok(payload) => payload,
            Err(error) => {
                warn!("Yahoo Finance FX lookup failed for {symbol}: {error}");
                return None;
            }
        };

        payload
            .quote_response
            .result
            .first()
            .and_then(|result| Decimal::from_f64_retain(result.regular_market_price))
    }

    /// Fetch the latest daily price for a single ticker.
    pub async fn fetch_current_price(
        &self,
        asset_id: Uuid,
        ticker: &str,
    ) -> Result<NewPriceRecord, DomainError> {
        let prices = self
            .fetch_current_prices_for_assets(&[(asset_id, ticker.to_string())])
            .await?;
        prices.into_iter().next().ok_or_else(|| {
            DomainError::ExternalServiceError(format!(
                "Yahoo returned no quote for ticker {ticker}"
            ))
        })
    }

    /// Fetch the latest daily prices for multiple tickers in a single request.
    pub async fn fetch_current_prices(
        &self,
        asset_id: Uuid,
        tickers: &[String],
    ) -> Result<Vec<NewPriceRecord>, DomainError> {
        let assets = tickers
            .iter()
            .cloned()
            .map(|ticker| (asset_id, ticker))
            .collect::<Vec<_>>();
        self.fetch_current_prices_for_assets(&assets).await
    }

    /// Fetch the latest daily prices for multiple asset/ticker pairs in a single request.
    pub async fn fetch_current_prices_for_assets(
        &self,
        assets: &[(Uuid, String)],
    ) -> Result<Vec<NewPriceRecord>, DomainError> {
        if assets.is_empty() {
            return Ok(Vec::new());
        }

        let payload = self
            .fetch_quote_response(
                &assets
                    .iter()
                    .map(|(_, ticker)| ticker.clone())
                    .collect::<Vec<_>>(),
            )
            .await?;
        let asset_ids_by_symbol = assets
            .iter()
            .map(|(asset_id, ticker)| (ticker.clone(), *asset_id))
            .collect::<std::collections::HashMap<_, _>>();
        let results = payload
            .quote_response
            .result
            .into_iter()
            .filter_map(|quote| {
                let symbol = quote.symbol?;
                let asset_id = asset_ids_by_symbol.get(&symbol).copied()?;
                let price = Decimal::from_f64_retain(quote.regular_market_price)?;
                let timestamp = DateTime::from_timestamp(quote.regular_market_time, 0)?;
                Some(NewPriceRecord {
                    asset_id,
                    date: timestamp.date_naive(),
                    close_price: price,
                    currency: quote.currency.unwrap_or_else(|| "USD".to_string()),
                    source: PriceSource::Yahoo,
                })
            })
            .collect::<Vec<_>>();

        if results.is_empty() {
            return Err(DomainError::ExternalServiceError(
                "Yahoo returned no valid price records".to_string(),
            ));
        }

        Ok(results)
    }

    /// Fetch daily close-price history for a ticker over the provided date range.
    pub async fn fetch_history(
        &self,
        asset_id: Uuid,
        ticker: &str,
        from_date: NaiveDate,
        to_date: NaiveDate,
    ) -> Result<Vec<NewPriceRecord>, DomainError> {
        let period1 = from_date
            .and_hms_opt(0, 0, 0)
            .expect("midnight should always be valid")
            .and_utc()
            .timestamp();
        let period2 = to_date
            .succ_opt()
            .unwrap_or(to_date)
            .and_hms_opt(0, 0, 0)
            .expect("midnight should always be valid")
            .and_utc()
            .timestamp();
        let url = format!("{}/v8/finance/chart/{ticker}", self.base_url);
        let response = self
            .send_with_retry(|| {
                self.http.get(&url).query(&[
                    ("interval", "1d"),
                    ("includeAdjustedClose", "false"),
                    ("events", "history"),
                    ("period1", &period1.to_string()),
                    ("period2", &period2.to_string()),
                ])
            })
            .await?;
        let payload = response
            .json::<ChartResponse>()
            .await
            .map_err(|error| DomainError::ExternalServiceError(error.to_string()))?;
        let result = payload.chart.result.into_iter().next().ok_or_else(|| {
            DomainError::ExternalServiceError(format!(
                "Yahoo returned no chart for ticker {ticker}"
            ))
        })?;
        let currency = result.meta.currency.unwrap_or_else(|| "USD".to_string());
        let closes = result
            .indicators
            .quote
            .into_iter()
            .next()
            .map(|quote| quote.close)
            .unwrap_or_default();

        let history = result
            .timestamp
            .into_iter()
            .zip(closes)
            .filter_map(|(timestamp, close)| {
                let close = close.and_then(Decimal::from_f64_retain)?;
                let observed_at = DateTime::from_timestamp(timestamp, 0)?;
                Some(NewPriceRecord {
                    asset_id,
                    date: observed_at.date_naive(),
                    close_price: close,
                    currency: currency.clone(),
                    source: PriceSource::Yahoo,
                })
            })
            .collect::<Vec<_>>();

        Ok(history)
    }

    async fn fetch_quote_response(&self, tickers: &[String]) -> Result<QuoteResponse, DomainError> {
        let url = format!("{}/v7/finance/quote", self.base_url);
        let symbols = tickers.join(",");
        let response = self
            .send_with_retry(|| self.http.get(&url).query(&[("symbols", symbols.as_str())]))
            .await?;
        response
            .json::<QuoteResponse>()
            .await
            .map_err(|error| DomainError::ExternalServiceError(error.to_string()))
    }

    async fn send_with_retry<F>(&self, mut request: F) -> Result<Response, DomainError>
    where
        F: FnMut() -> reqwest::RequestBuilder,
    {
        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                sleep(self.request_delay).await;
            }

            let response = request()
                .send()
                .await
                .map_err(|error| DomainError::ExternalServiceError(error.to_string()))?;

            if response.status() == StatusCode::TOO_MANY_REQUESTS && attempt < MAX_RETRIES {
                let backoff = 30_u64.saturating_mul(2_u64.pow(attempt as u32));
                warn!(
                    retry_attempt = attempt + 1,
                    backoff_seconds = backoff,
                    "Yahoo Finance rate limit encountered, retrying"
                );
                sleep(Duration::from_secs(backoff)).await;
                continue;
            }

            if !response.status().is_success() {
                return Err(DomainError::ExternalServiceError(format!(
                    "Yahoo returned status {}",
                    response.status()
                )));
            }

            return Ok(response);
        }

        Err(DomainError::ExternalServiceError(
            "Yahoo request exhausted retry budget".to_string(),
        ))
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
    currency: Option<String>,
    symbol: Option<String>,
    #[serde(rename = "regularMarketPrice")]
    regular_market_price: f64,
    #[serde(rename = "regularMarketTime")]
    regular_market_time: i64,
}

#[derive(Debug, Deserialize)]
struct ChartResponse {
    chart: ChartResultEnvelope,
}

#[derive(Debug, Deserialize)]
struct ChartResultEnvelope {
    result: Vec<ChartResult>,
}

#[derive(Debug, Deserialize)]
struct ChartResult {
    meta: ChartMeta,
    timestamp: Vec<i64>,
    indicators: ChartIndicators,
}

#[derive(Debug, Deserialize)]
struct ChartMeta {
    currency: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChartIndicators {
    quote: Vec<ChartQuote>,
}

#[derive(Debug, Deserialize)]
struct ChartQuote {
    close: Vec<Option<f64>>,
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, NaiveDate};
    use rust_decimal::Decimal;

    use super::{ChartResponse, QuoteResponse, YahooFinanceClient};

    #[tokio::test]
    async fn test_lookup_exchange_rate_returns_one_for_same_currency() {
        let client = YahooFinanceClient::new("http://localhost", 0).expect("client should build");

        let rate = client.lookup_exchange_rate("EUR", "EUR").await;

        assert_eq!(rate, Some(Decimal::ONE));
    }

    #[test]
    fn test_quote_response_deserializes_current_price_payload() {
        let payload = serde_json::from_str::<QuoteResponse>(
            r#"{
                "quoteResponse": {
                    "result": [{
                        "currency": "EUR",
                        "regularMarketPrice": 123.45,
                        "regularMarketTime": 1713484800
                    }]
                }
            }"#,
        )
        .expect("quote payload should deserialize");

        assert_eq!(payload.quote_response.result.len(), 1);
        assert_eq!(
            payload.quote_response.result[0].currency.as_deref(),
            Some("EUR")
        );
    }

    #[test]
    fn test_chart_response_deserializes_history_payload() {
        let payload = serde_json::from_str::<ChartResponse>(
            r#"{
                "chart": {
                    "result": [{
                        "meta": { "currency": "USD" },
                        "timestamp": [1713398400, 1713484800],
                        "indicators": {
                            "quote": [{
                                "close": [100.0, 101.5]
                            }]
                        }
                    }]
                }
            }"#,
        )
        .expect("chart payload should deserialize");

        assert_eq!(payload.chart.result.len(), 1);
        assert_eq!(payload.chart.result[0].timestamp.len(), 2);
        assert_eq!(
            DateTime::from_timestamp(payload.chart.result[0].timestamp[0], 0)
                .expect("timestamp should be valid")
                .date_naive(),
            NaiveDate::from_ymd_opt(2024, 4, 18).expect("static date should be valid")
        );
    }
}
