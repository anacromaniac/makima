//! # API Crate
//!
//! This crate contains the Axum HTTP handlers, router, middleware, and request/response DTOs.
//! It serves as the entry point for the Makima backend application.
//!
//! ## Dependencies
//! - `domain`: Contains core domain models and business logic
//! - `db`: Provides repository implementations for database access

use sqlx::migrate::Migrator;
use sqlx::postgres::PgPoolOptions;
use std::env;

/// Embedded migrations using `sqlx::migrate!()`.
/// Migrations are located in the `migrations/` directory at the project root.
/// Path is relative to this crate's Cargo.toml location (crates/api/).
static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");

/// Application configuration loaded from environment variables.
#[derive(Debug)]
struct AppConfig {
    /// PostgreSQL connection URL.
    database_url: String,
    /// Maximum pool size for sqlx connections.
    database_pool_max_size: u32,
}

impl AppConfig {
    /// Load configuration from environment variables.
    ///
    /// # Errors
    /// Returns an error if required environment variables are missing or invalid.
    fn load_from_env() -> Result<Self, Box<dyn std::error::Error>> {
        dotenvy::dotenv().ok();

        let database_url = env::var("DATABASE_URL")
            .map_err(|_| "DATABASE_URL environment variable is required")?;

        let database_pool_max_size = env::var("DATABASE_POOL_MAX_SIZE")
            .unwrap_or_else(|_| "5".to_string())
            .parse()
            .map_err(|e| format!("Invalid DATABASE_POOL_MAX_SIZE: {e}"))?;

        Ok(Self {
            database_url,
            database_pool_max_size,
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Makima starting...");

    // Load configuration
    let config = AppConfig::load_from_env()?;
    println!("Configuration loaded");

    // Create database connection pool
    println!("Connecting to PostgreSQL...");
    let pool = PgPoolOptions::new()
        .max_connections(config.database_pool_max_size)
        .connect(&config.database_url)
        .await
        .map_err(|e| format!("Failed to connect to database: {e}"))?;

    println!("Makima connected to database");

    // Run migrations
    println!("Running migrations...");
    MIGRATOR
        .run(&pool)
        .await
        .map_err(|e| format!("Migration failed: {e}"))?;

    println!("Migrations completed successfully");

    println!("Makima ready");

    // Keep the pool alive
    // TODO: Start the Axum server here in a future task
    drop(pool);

    Ok(())
}
