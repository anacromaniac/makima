//! Scheduled price-refresh job for Yahoo Finance.

use std::sync::Arc;

use domain::{AssetRepository, PriceRepository};
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{error, info};

use crate::yahoo::YahooFinanceClient;

/// Run one full price-update cycle immediately.
pub async fn run_price_update_once(
    asset_repo: Arc<dyn AssetRepository>,
    price_repo: Arc<dyn PriceRepository>,
    yahoo_client: YahooFinanceClient,
) {
    let assets = match asset_repo.list_in_use().await {
        Ok(assets) => assets,
        Err(error) => {
            error!("price update job failed to load assets: {error}");
            return;
        }
    };

    let mut updated = 0_u64;
    let mut failed = 0_u64;
    let tracked_assets = assets
        .into_iter()
        .filter_map(|asset| {
            asset
                .yahoo_ticker
                .clone()
                .map(|ticker| (asset.id, ticker, asset.isin))
        })
        .collect::<Vec<_>>();

    for chunk in tracked_assets.chunks(10) {
        let batch = chunk
            .iter()
            .map(|(asset_id, ticker, _)| (*asset_id, ticker.clone()))
            .collect::<Vec<_>>();

        match yahoo_client.fetch_current_prices_for_assets(&batch).await {
            Ok(records) => {
                let records_by_asset = records
                    .into_iter()
                    .map(|record| (record.asset_id, record))
                    .collect::<std::collections::HashMap<_, _>>();

                for (asset_id, ticker, isin) in chunk {
                    match records_by_asset.get(asset_id) {
                        Some(record) => match price_repo.insert(record).await {
                            Ok(_) => updated += 1,
                            Err(error) => {
                                failed += 1;
                                error!(
                                    asset_id = %asset_id,
                                    ticker,
                                    isin,
                                    "failed to store price update: {error}"
                                );
                            }
                        },
                        None => {
                            failed += 1;
                            error!(asset_id = %asset_id, ticker, isin, "Yahoo batch returned no quote");
                        }
                    }
                }
            }
            Err(error) => {
                failed += chunk.len() as u64;
                for (asset_id, ticker, isin) in chunk {
                    error!(asset_id = %asset_id, ticker, isin, "failed to fetch Yahoo price batch: {error}");
                }
            }
        }
    }

    info!(
        updated_assets = updated,
        failed_assets = failed,
        "price update job completed"
    );
}

/// Start the cron-based daily price update job.
pub async fn start_price_update_job(
    cron_expression: &str,
    asset_repo: Arc<dyn AssetRepository>,
    price_repo: Arc<dyn PriceRepository>,
    yahoo_client: YahooFinanceClient,
) -> Result<JobScheduler, tokio_cron_scheduler::JobSchedulerError> {
    let scheduler = JobScheduler::new().await?;
    let job = Job::new_async(cron_expression, move |_uuid, _lock| {
        let asset_repo = asset_repo.clone();
        let price_repo = price_repo.clone();
        let yahoo_client = yahoo_client.clone();

        Box::pin(async move {
            run_price_update_once(asset_repo, price_repo, yahoo_client).await;
        })
    })?;

    scheduler.add(job).await?;
    scheduler.start().await?;
    Ok(scheduler)
}
