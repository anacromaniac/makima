//! Binary entry point for the Makima API server.

use api::{AppConfig, MIGRATOR, build_app, build_app_state};
use db::repositories::{asset_repo::PgAssetRepository, price_repo::PgPriceRepository};
use price_fetcher::{job::start_price_update_job, yahoo::YahooFinanceClient};
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;

/// Main entry point for the Makima API server.
///
/// Initialization order:
/// 1. Load `.env` file (must happen before tracing init)
/// 2. Initialize structured JSON logging
/// 3. Load configuration
/// 4. Create the database pool
/// 5. Run migrations
/// 6. Build and serve the Axum application
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::from_filename(".env").ok();

    tracing_subscriber::fmt()
        .json()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tracing::info!("Makima starting...");

    let config = AppConfig::load_from_env()?;
    let server_addr = config.server_address();

    tracing::info!("Connecting to PostgreSQL...");
    let pool = PgPoolOptions::new()
        .max_connections(config.database_pool_max_size)
        .connect(&config.database_url)
        .await
        .map_err(|e| format!("Failed to connect to database: {e}"))?;

    tracing::info!("Running migrations...");
    MIGRATOR
        .run(&pool)
        .await
        .map_err(|e| format!("Migration failed: {e}"))?;

    let cron_expression =
        std::env::var("PRICE_UPDATE_CRON").unwrap_or_else(|_| "0 0 22 * *".to_string());
    let yahoo_client = YahooFinanceClient::from_env()
        .map_err(|e| format!("Failed to construct Yahoo Finance client: {e}"))?;
    let _price_scheduler = start_price_update_job(
        &cron_expression,
        std::sync::Arc::new(PgAssetRepository::new(pool.clone())),
        std::sync::Arc::new(PgPriceRepository::new(pool.clone())),
        yahoo_client,
    )
    .await
    .map_err(|e| format!("Failed to start price update job: {e}"))?;

    let app_state = build_app_state(pool, config.jwt_secret);
    let app = build_app(app_state, &config.cors_allowed_origins);

    let listener = TcpListener::bind(&server_addr).await?;
    tracing::info!("Makima server listening on {}", listener.local_addr()?);

    axum::serve(listener, app).await?;

    Ok(())
}
