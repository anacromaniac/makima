//! Historical price backfill helpers.

use chrono::{Datelike, NaiveDate, Utc};
use domain::{DomainError, PriceRepository};
use std::sync::Arc;
use uuid::Uuid;

use crate::yahoo::YahooFinanceClient;

/// Fetch and store recent historical prices for a single asset.
pub async fn backfill_asset(
    price_repo: Arc<dyn PriceRepository>,
    yahoo_client: &YahooFinanceClient,
    asset_id: Uuid,
    ticker: &str,
    years: i64,
) -> Result<u64, DomainError> {
    let to_date = Utc::now().date_naive();
    let from_date = subtract_years(to_date, years);
    let history = yahoo_client
        .fetch_history(asset_id, ticker, from_date, to_date)
        .await?;

    price_repo
        .insert_batch(&history)
        .await
        .map_err(|error| DomainError::ExternalServiceError(error.to_string()))
}

fn subtract_years(date: NaiveDate, years: i64) -> NaiveDate {
    let target_year = date.year_ce().1 as i32 - years as i32;
    NaiveDate::from_ymd_opt(target_year, date.month(), date.day())
        .or_else(|| NaiveDate::from_ymd_opt(target_year, date.month(), 28))
        .unwrap_or(date)
}
