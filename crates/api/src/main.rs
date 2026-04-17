//! # API Crate
//!
//! This crate contains the Axum HTTP handlers, router, middleware, and request/response DTOs.
//! It serves as the entry point for the Makima backend application.
//!
//! ## Dependencies
//! - `domain`: Contains core domain models and business logic
//! - `db`: Provides repository implementations for database access

use axum::{
    Router,
    extract::State,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Json},
    routing::get,
};
use serde::{Deserialize, Serialize};
use sqlx::migrate::Migrator;
use sqlx::postgres::PgPoolOptions;
use std::env;
use std::time::Duration;
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

/// Embedded migrations using `sqlx::migrate!()`.
/// Migrations are located in the `migrations/` directory at the project root.
/// Path is relative to this crate's Cargo.toml location (crates/api/).
static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");

/// OpenAPI specification for the Makima API.
#[derive(OpenApi)]
#[openapi(
    paths(health, ready),
    components(schemas(HealthResponse, ReadyResponse)),
    info(
        title = "Makima API",
        version = "0.1.0",
        description = "Investment tracking backend API"
    )
)]
struct ApiDoc;

/// Application configuration loaded from environment variables.
#[derive(Debug)]
struct AppConfig {
    /// PostgreSQL connection URL.
    database_url: String,
    /// Maximum pool size for sqlx connections.
    database_pool_max_size: u32,
    /// Server listen host.
    server_host: String,
    /// Server listen port.
    server_port: u16,
    /// CORS allowed origins (comma-separated).
    cors_allowed_origins: Vec<String>,
}

impl AppConfig {
    /// Load configuration from environment variables.
    ///
    /// Note: `.env` file should be loaded before calling this function
    /// (done in `main` before initializing tracing).
    ///
    /// # Errors
    /// Returns an error if required environment variables are missing or invalid.
    fn load_from_env() -> Result<Self, Box<dyn std::error::Error>> {
        let database_url = env::var("DATABASE_URL")
            .map_err(|_| "DATABASE_URL environment variable is required")?;

        let database_pool_max_size = env::var("DATABASE_POOL_MAX_SIZE")
            .unwrap_or_else(|_| "5".to_string())
            .parse()
            .map_err(|e| format!("Invalid DATABASE_POOL_MAX_SIZE: {e}"))?;

        let server_host = env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());

        let server_port = env::var("SERVER_PORT")
            .unwrap_or_else(|_| "3000".to_string())
            .parse()
            .map_err(|e| format!("Invalid SERVER_PORT: {e}"))?;

        let cors_allowed_origins = env::var("CORS_ALLOWED_ORIGINS")
            .unwrap_or_else(|_| "http://localhost:3000".to_string())
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();

        Ok(Self {
            database_url,
            database_pool_max_size,
            server_host,
            server_port,
            cors_allowed_origins,
        })
    }

    /// Get the full server address.
    fn server_address(&self) -> String {
        format!("{}:{}", self.server_host, self.server_port)
    }
}

/// Health check response.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
struct HealthResponse {
    /// Health status.
    status: String,
}

/// Readiness check response.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
struct ReadyResponse {
    /// Readiness status.
    status: String,
}

/// Handler for the `/health` endpoint.
///
/// Returns 200 if the service is running. This is a liveness check.
#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "Service is healthy", body = HealthResponse)
    )
)]
#[tracing::instrument]
async fn health() -> impl IntoResponse {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

/// Handler for the `/ready` endpoint.
///
/// Returns 200 if the database is reachable, 503 otherwise.
/// This is a readiness check.
#[utoipa::path(
    get,
    path = "/ready",
    responses(
        (status = 200, description = "Service is ready", body = ReadyResponse),
        (status = 503, description = "Service is unavailable", body = ReadyResponse)
    )
)]
#[tracing::instrument(skip(pool))]
async fn ready(State(pool): State<sqlx::PgPool>) -> impl IntoResponse {
    match sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&pool)
        .await
    {
        Ok(_) => (
            StatusCode::OK,
            Json(ReadyResponse {
                status: "ready".to_string(),
            }),
        ),
        Err(e) => {
            tracing::error!("Database readiness check failed: {e}");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ReadyResponse {
                    status: "unavailable".to_string(),
                }),
            )
        }
    }
}

/// Custom middleware to add security headers to all responses.
///
/// This adds:
/// - X-Content-Type-Options: nosniff
/// - X-Frame-Options: DENY
/// - Referrer-Policy: no-referrer
async fn security_headers_middleware(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let mut response = next.run(request).await;

    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
        .headers_mut()
        .insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );

    response
}

fn build_cors_layer(origins: &[String]) -> CorsLayer {
    let origins: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|s| HeaderValue::try_from(s).ok())
        .collect();

    if origins.is_empty() {
        CorsLayer::permissive()
    } else {
        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods(Any)
            .allow_headers(Any)
    }
}

/// Main entry point for the Makima API server.
///
/// Initialization order:
/// 1. Load `.env` file (must happen before tracing init)
/// 2. Initialize structured JSON logging (reads RUST_LOG env var)
/// 3. Load configuration
/// 4. Create database connection pool
/// 5. Run migrations
/// 6. Build and start Axum server with middleware stack
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load .env file before initializing tracing (so RUST_LOG is available)
    dotenvy::from_filename(".env").ok();

    // Initialize structured JSON logging
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tracing::info!("Makima starting...");

    // Load configuration
    let config = AppConfig::load_from_env()?;
    tracing::info!("Configuration loaded");

    // Create database connection pool
    tracing::info!("Connecting to PostgreSQL...");
    let pool = PgPoolOptions::new()
        .max_connections(config.database_pool_max_size)
        .connect(&config.database_url)
        .await
        .map_err(|e| format!("Failed to connect to database: {e}"))?;

    tracing::info!("Makima connected to database");

    // Run migrations
    tracing::info!("Running migrations...");
    MIGRATOR
        .run(&pool)
        .await
        .map_err(|e| format!("Migration failed: {e}"))?;

    tracing::info!("Migrations completed successfully");

    // Build CORS layer
    let cors_layer = build_cors_layer(&config.cors_allowed_origins);

    // Build middleware stack (outermost first)
    let middleware = ServiceBuilder::new()
        // Set request ID and propagate it
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(PropagateRequestIdLayer::x_request_id())
        // Trace requests and responses with request ID
        .layer(TraceLayer::new_for_http())
        // Compression (gzip/deflate)
        .layer(CompressionLayer::new())
        // CORS
        .layer(cors_layer)
        // 30 second timeout
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(30),
        ));

    // Build the router
    let app = Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .merge(SwaggerUi::new("/swagger-ui").url("/api-doc/openapi.json", ApiDoc::openapi()))
        .with_state(pool)
        .layer(middleware)
        .layer(axum::middleware::from_fn(security_headers_middleware));

    // Bind to the configured address
    let addr = config.server_address();
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("Makima server listening on {}", listener.local_addr()?);

    // Start the server
    axum::serve(listener, app).await?;

    Ok(())
}
