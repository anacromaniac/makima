//! # API Crate
//!
//! This crate contains the Axum HTTP handlers, router, middleware, and request/response DTOs.
//! It also exposes the reusable application builder used by both the production binary and
//! integration tests.

pub mod assets;
pub mod auth;
pub mod import;
pub mod portfolios;
pub mod positions;
mod state;
pub mod transactions;
pub mod users;

use std::sync::Arc;
use std::time::Duration;

use application::{
    assets::{AssetPriceBackfill, AssetService, AssetTickerLookup},
    auth::AuthService,
    import::ImportService,
    portfolios::PortfolioService,
    positions::PositionService,
    prices::{CurrentPriceLookup, PriceService},
    transactions::{
        AssetMetadataLookup, ExchangeRateLookup, ResolvedAssetMetadata, TransactionService,
    },
    users::UserService,
};
use axum::{
    Router,
    extract::State,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Json},
    routing::get,
};
use chrono::Utc;
use db::repositories::exchange_rate_repo::PgExchangeRateRepository;
use domain::{ExchangeRateRepository, NewExchangeRate};
use importer::{bgsaxo::BgSaxoImporter, fineco::FinecoImporter};
use price_fetcher::openfigi::OpenFigiClient;
use price_fetcher::{exchange::YahooExchangeRateFetcher, yahoo::YahooFinanceClient};
use serde::{Deserialize, Serialize};
use sqlx::migrate::Migrator;
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};
use utoipa::{
    OpenApi,
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
};
use utoipa_swagger_ui::SwaggerUi;

pub use state::AppState;

pub trait AssetReferenceLookup: AssetTickerLookup + AssetMetadataLookup {}

impl<T> AssetReferenceLookup for T where T: AssetTickerLookup + AssetMetadataLookup {}

pub trait PriceAdapters: CurrentPriceLookup + AssetPriceBackfill {}

impl<T> PriceAdapters for T where T: CurrentPriceLookup + AssetPriceBackfill {}

struct OpenFigiTickerLookup {
    client: OpenFigiClient,
}

#[async_trait::async_trait]
impl AssetTickerLookup for OpenFigiTickerLookup {
    async fn lookup_yahoo_ticker(&self, isin: &str) -> Option<String> {
        self.client.lookup_yahoo_ticker(isin).await
    }
}

#[async_trait::async_trait]
impl AssetMetadataLookup for OpenFigiTickerLookup {
    async fn lookup_asset_metadata(&self, isin: &str) -> Option<ResolvedAssetMetadata> {
        let asset = self.client.lookup_asset_metadata(isin).await?;
        Some(ResolvedAssetMetadata {
            isin: asset.isin,
            yahoo_ticker: asset.yahoo_ticker,
            name: asset.name,
            asset_class: map_openfigi_asset_class(asset.security_type2.as_deref()),
            currency: asset.currency.unwrap_or_else(|| "EUR".to_string()),
            exchange: asset.exchange,
        })
    }
}

struct YahooExchangeRateLookup {
    fetcher: YahooExchangeRateFetcher,
}

#[async_trait::async_trait]
impl ExchangeRateLookup for YahooExchangeRateLookup {
    async fn lookup_rate_to_eur(&self, currency: &str) -> Option<rust_decimal::Decimal> {
        self.fetcher
            .fetch_rate(currency, "EUR")
            .await
            .map(|rate| rate.rate)
            .map_err(|error| {
                tracing::warn!(
                    from_currency = currency.to_ascii_uppercase(),
                    to_currency = "EUR",
                    "live exchange-rate lookup failed: {error}"
                );
            })
            .ok()
    }
}

struct PersistedExchangeRateLookup<E> {
    exchange_rate_repo: Arc<dyn ExchangeRateRepository>,
    live_lookup: Arc<E>,
}

#[async_trait::async_trait]
impl<E> ExchangeRateLookup for PersistedExchangeRateLookup<E>
where
    E: ExchangeRateLookup + 'static,
{
    async fn lookup_rate_to_eur(&self, currency: &str) -> Option<rust_decimal::Decimal> {
        let normalized_currency = currency.to_ascii_uppercase();

        match self
            .exchange_rate_repo
            .find_latest(&normalized_currency, "EUR")
            .await
        {
            Ok(Some(rate)) => return Some(rate.rate),
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(
                    from_currency = normalized_currency,
                    to_currency = "EUR",
                    "stored exchange-rate lookup failed: {error}"
                );
            }
        }

        let rate = self
            .live_lookup
            .lookup_rate_to_eur(&normalized_currency)
            .await?;
        let new_rate = NewExchangeRate {
            from_currency: normalized_currency,
            to_currency: "EUR".to_string(),
            date: Utc::now().date_naive(),
            rate,
        };

        if let Err(error) = self.exchange_rate_repo.insert(&new_rate).await {
            tracing::warn!(
                from_currency = new_rate.from_currency,
                to_currency = new_rate.to_currency,
                "failed to persist fetched exchange rate: {error}"
            );
        }

        Some(rate)
    }
}

struct YahooCurrentPriceLookup {
    client: YahooFinanceClient,
}

#[async_trait::async_trait]
impl CurrentPriceLookup for YahooCurrentPriceLookup {
    async fn fetch_current_price(
        &self,
        asset_id: uuid::Uuid,
        ticker: &str,
    ) -> Result<domain::NewPriceRecord, domain::DomainError> {
        self.client.fetch_current_price(asset_id, ticker).await
    }
}

struct YahooPriceBackfill {
    client: YahooFinanceClient,
    price_repo: Arc<dyn domain::PriceRepository>,
}

#[async_trait::async_trait]
impl AssetPriceBackfill for YahooPriceBackfill {
    async fn backfill_asset_prices(
        &self,
        asset_id: uuid::Uuid,
        ticker: &str,
    ) -> Result<u64, domain::DomainError> {
        let years = std::env::var("BACKFILL_YEARS")
            .ok()
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(5);

        price_fetcher::backfill::backfill_asset(
            self.price_repo.clone(),
            &self.client,
            asset_id,
            ticker,
            years,
        )
        .await
    }
}

struct YahooPriceAdapters {
    current: YahooCurrentPriceLookup,
    backfill: YahooPriceBackfill,
}

#[async_trait::async_trait]
impl CurrentPriceLookup for YahooPriceAdapters {
    async fn fetch_current_price(
        &self,
        asset_id: uuid::Uuid,
        ticker: &str,
    ) -> Result<domain::NewPriceRecord, domain::DomainError> {
        self.current.fetch_current_price(asset_id, ticker).await
    }
}

#[async_trait::async_trait]
impl AssetPriceBackfill for YahooPriceAdapters {
    async fn backfill_asset_prices(
        &self,
        asset_id: uuid::Uuid,
        ticker: &str,
    ) -> Result<u64, domain::DomainError> {
        self.backfill.backfill_asset_prices(asset_id, ticker).await
    }
}

/// Embedded sqlx migrations for the workspace.
///
/// The path is resolved relative to `crates/api/Cargo.toml`.
pub static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");

/// Application configuration loaded from environment variables.
#[derive(Debug)]
pub struct AppConfig {
    /// PostgreSQL connection URL.
    pub database_url: String,
    /// Maximum pool size for sqlx connections.
    pub database_pool_max_size: u32,
    /// Server listen host.
    pub server_host: String,
    /// Server listen port.
    pub server_port: u16,
    /// CORS allowed origins (comma-separated).
    pub cors_allowed_origins: Vec<String>,
    /// HS256 signing secret for JWT access tokens.
    pub jwt_secret: String,
}

impl AppConfig {
    /// Load configuration from environment variables.
    ///
    /// # Errors
    /// Returns an error if required environment variables are missing or invalid.
    pub fn load_from_env() -> Result<Self, Box<dyn std::error::Error>> {
        let database_url = std::env::var("DATABASE_URL")
            .map_err(|_| "DATABASE_URL environment variable is required")?;

        let database_pool_max_size = std::env::var("DATABASE_POOL_MAX_SIZE")
            .unwrap_or_else(|_| "5".to_string())
            .parse()
            .map_err(|e| format!("Invalid DATABASE_POOL_MAX_SIZE: {e}"))?;

        let server_host = std::env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());

        let server_port = std::env::var("SERVER_PORT")
            .unwrap_or_else(|_| "3000".to_string())
            .parse()
            .map_err(|e| format!("Invalid SERVER_PORT: {e}"))?;

        let cors_allowed_origins = std::env::var("CORS_ALLOWED_ORIGINS")
            .unwrap_or_else(|_| "http://localhost:3000".to_string())
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();

        let jwt_secret = std::env::var("JWT_SECRET")
            .map_err(|_| "JWT_SECRET environment variable is required")?;

        Ok(Self {
            database_url,
            database_pool_max_size,
            server_host,
            server_port,
            cors_allowed_origins,
            jwt_secret,
        })
    }

    /// Return the `host:port` socket address string used by the HTTP server.
    pub fn server_address(&self) -> String {
        format!("{}:{}", self.server_host, self.server_port)
    }
}

/// Build the shared application state with concrete adapters and application services.
pub fn build_app_state(pool: sqlx::PgPool, jwt_secret: String) -> AppState {
    let openfigi_client =
        OpenFigiClient::from_env().expect("failed to construct OpenFIGI client from environment");
    let yahoo_client = YahooFinanceClient::from_env()
        .expect("failed to construct Yahoo Finance client from environment");
    let price_repo = Arc::new(db::repositories::price_repo::PgPriceRepository::new(
        pool.clone(),
    ));
    let price_adapters = Arc::new(YahooPriceAdapters {
        current: YahooCurrentPriceLookup {
            client: yahoo_client.clone(),
        },
        backfill: YahooPriceBackfill {
            client: yahoo_client.clone(),
            price_repo,
        },
    });
    build_app_state_with_lookup(
        pool,
        jwt_secret,
        Arc::new(OpenFigiTickerLookup {
            client: openfigi_client,
        }),
        Arc::new(YahooExchangeRateLookup {
            fetcher: YahooExchangeRateFetcher::new(yahoo_client),
        }),
        price_adapters,
    )
}

/// Build the shared application state with injected external lookup adapters.
pub fn build_app_state_with_lookup<L, E, P>(
    pool: sqlx::PgPool,
    jwt_secret: String,
    asset_reference_lookup: Arc<L>,
    exchange_rate_lookup: Arc<E>,
    price_adapters: Arc<P>,
) -> AppState
where
    L: AssetReferenceLookup + 'static,
    E: ExchangeRateLookup + 'static,
    P: PriceAdapters + 'static,
{
    let user_repo = Arc::new(db::repositories::user_repo::PgUserRepository::new(
        pool.clone(),
    ));
    let refresh_token_repo =
        Arc::new(db::repositories::refresh_token_repo::PgRefreshTokenRepository::new(pool.clone()));
    let portfolio_repo =
        Arc::new(db::repositories::portfolio_repo::PgPortfolioRepository::new(pool.clone()));
    let asset_repo = Arc::new(db::repositories::asset_repo::PgAssetRepository::new(
        pool.clone(),
    ));
    let position_repo = Arc::new(db::repositories::position_repo::PgPositionRepository::new(
        pool.clone(),
    ));
    let price_repo = Arc::new(db::repositories::price_repo::PgPriceRepository::new(
        pool.clone(),
    ));
    let exchange_rate_repo = Arc::new(PgExchangeRateRepository::new(pool.clone()));
    let persisted_exchange_rate_lookup = Arc::new(PersistedExchangeRateLookup {
        exchange_rate_repo,
        live_lookup: exchange_rate_lookup,
    });
    let transaction_repo =
        Arc::new(db::repositories::transaction_repo::PgTransactionRepository::new(pool.clone()));
    let broker_import_repo =
        Arc::new(db::repositories::broker_import_repo::PgBrokerImportRepository::new(pool.clone()));
    let auth_service = Arc::new(AuthService::new(
        user_repo.clone(),
        refresh_token_repo.clone(),
        jwt_secret.clone(),
    ));
    let asset_service = Arc::new(AssetService::new(
        asset_repo.clone(),
        asset_reference_lookup.clone(),
        price_adapters.clone(),
    ));
    let portfolio_service = Arc::new(PortfolioService::new(portfolio_repo.clone()));
    let position_service = Arc::new(PositionService::new(portfolio_repo.clone(), position_repo));
    let price_service = Arc::new(PriceService::new(
        asset_repo.clone(),
        price_repo,
        price_adapters.clone(),
    ));
    let transaction_service = Arc::new(TransactionService::new(
        portfolio_repo.clone(),
        asset_repo.clone(),
        transaction_repo.clone(),
        asset_reference_lookup.clone(),
        persisted_exchange_rate_lookup.clone(),
    ));
    let import_service = Arc::new(ImportService::new(
        portfolio_repo,
        asset_repo,
        transaction_repo,
        broker_import_repo,
        asset_reference_lookup.clone(),
        persisted_exchange_rate_lookup,
        price_adapters.clone(),
        std::collections::HashMap::from([
            (
                "fineco".to_string(),
                Arc::new(FinecoImporter) as Arc<dyn domain::BrokerImporter + Send + Sync>,
            ),
            (
                "bgsaxo".to_string(),
                Arc::new(BgSaxoImporter) as Arc<dyn domain::BrokerImporter + Send + Sync>,
            ),
        ]),
    ));
    let user_service = Arc::new(UserService::new(user_repo));

    AppState {
        pool,
        auth_service,
        asset_service,
        portfolio_service,
        position_service,
        price_service,
        transaction_service,
        import_service,
        user_service,
        jwt_secret,
    }
}

/// Build the production Axum application with the full middleware stack.
pub fn build_app(app_state: AppState, cors_allowed_origins: &[String]) -> Router {
    let middleware = ServiceBuilder::new()
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(TraceLayer::new_for_http())
        .layer(CompressionLayer::new())
        .layer(build_cors_layer(cors_allowed_origins))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(30),
        ));

    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .merge(auth::handlers::auth_router())
        .merge(users::handlers::users_router())
        .merge(portfolios::handlers::portfolios_router())
        .merge(positions::handlers::positions_router())
        .merge(assets::handlers::assets_router())
        .merge(import::handlers::import_router())
        .merge(transactions::handlers::transactions_router())
        .merge(SwaggerUi::new("/swagger-ui").url("/api-doc/openapi.json", ApiDoc::openapi()))
        .with_state(app_state)
        .layer(middleware)
        .layer(axum::middleware::from_fn(security_headers_middleware))
}

/// OpenAPI specification for the Makima API.
#[derive(OpenApi)]
#[openapi(
    paths(
        health,
        ready,
        auth::handlers::register,
        auth::handlers::login,
        auth::handlers::refresh,
        auth::handlers::change_password,
        users::handlers::get_me,
        assets::handlers::list_assets,
        assets::handlers::get_asset,
        assets::handlers::create_asset,
        assets::handlers::update_asset,
        assets::handlers::refresh_asset_price,
        assets::handlers::put_manual_asset_price,
        assets::handlers::list_asset_price_history,
        portfolios::handlers::list_portfolios,
        portfolios::handlers::create_portfolio,
        portfolios::handlers::get_portfolio,
        portfolios::handlers::update_portfolio,
        portfolios::handlers::delete_portfolio,
        positions::handlers::list_positions,
        import::handlers::import_transactions,
        transactions::handlers::list_transactions,
        transactions::handlers::create_transaction,
        transactions::handlers::get_transaction,
        transactions::handlers::update_transaction,
        transactions::handlers::delete_transaction,
    ),
    components(schemas(
        HealthResponse,
        ReadyResponse,
        auth::dto::RegisterRequest,
        auth::dto::LoginRequest,
        auth::dto::RefreshRequest,
        auth::dto::ChangePasswordRequest,
        auth::dto::TokenResponse,
        users::dto::UserResponse,
        assets::dto::CreateAssetRequest,
        assets::dto::UpdateAssetRequest,
        assets::dto::ManualPriceEntryRequest,
        assets::dto::PriceRecordResponse,
        assets::dto::AssetResponse,
        assets::handlers::PaginatedAssetResponse,
        assets::handlers::PaginatedPriceHistoryResponse,
        assets::handlers::PaginationMetaResponse,
        portfolios::dto::CreatePortfolioRequest,
        portfolios::dto::UpdatePortfolioRequest,
        portfolios::dto::PortfolioResponse,
        portfolios::handlers::PaginatedPortfolioResponse,
        portfolios::handlers::PaginationMetaResponse,
        positions::dto::PositionResponse,
        import::dto::ImportErrorResponse,
        import::dto::ImportResponse,
        import::dto::ImportRowErrorResponse,
        transactions::dto::CreateTransactionRequest,
        transactions::dto::UpdateTransactionRequest,
        transactions::dto::TransactionResponse,
        transactions::handlers::PaginatedTransactionResponse,
        transactions::handlers::PaginationMetaResponse,
    )),
    modifiers(&SecurityAddon),
    info(
        title = "Makima API",
        version = "0.1.0",
        description = "Investment tracking backend API"
    )
)]
struct ApiDoc;

struct SecurityAddon;

fn map_openfigi_asset_class(security_type2: Option<&str>) -> domain::AssetClass {
    match security_type2
        .unwrap_or_default()
        .to_ascii_uppercase()
        .as_str()
    {
        "BOND" | "CORPORATE BOND" | "GOVERNMENT BOND" => domain::AssetClass::Bond,
        "ETF" | "ETP" | "COMMON STOCK" | "PREFERRED STOCK" | "REIT" | "ADR" | "DR" => {
            domain::AssetClass::Stock
        }
        "COMMODITY" => domain::AssetClass::Commodity,
        "CRYPTOCURRENCY" => domain::AssetClass::Crypto,
        "MONEY MARKET" | "CASH" => domain::AssetClass::CashEquivalent,
        _ => domain::AssetClass::Alternative,
    }
}

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer_auth",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("JWT")
                        .build(),
                ),
            );
        }
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

#[utoipa::path(
    get,
    path = "/ready",
    responses(
        (status = 200, description = "Service is ready", body = ReadyResponse),
        (status = 503, description = "Service is unavailable", body = ReadyResponse)
    )
)]
#[tracing::instrument(skip(state))]
async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    match sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.pool)
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
