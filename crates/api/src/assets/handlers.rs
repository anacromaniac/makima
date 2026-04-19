//! HTTP handlers for shared asset CRUD and price endpoints.

use application::{assets::AssetError, prices::PriceError};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use domain::{AssetFilters, PaginatedResult, PaginationParams, UpdateAsset};
use garde::Validate;
use serde::{Deserialize, Serialize};

use crate::{
    assets::dto::{
        ApiAssetClass, AssetResponse, CreateAssetRequest, ManualPriceEntryRequest,
        PriceRecordResponse, UpdateAssetRequest,
    },
    auth::AuthenticatedUser,
    state::AppState,
};

fn asset_error_response(error: AssetError) -> Response {
    let (status, code, message): (StatusCode, &'static str, String) = match error {
        AssetError::NotFound => (
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "Asset not found".to_string(),
        ),
        AssetError::DuplicateIsin => (
            StatusCode::CONFLICT,
            "ASSET_ALREADY_EXISTS",
            "Asset with this ISIN already exists".to_string(),
        ),
        AssetError::Repository(error) => {
            tracing::error!("Asset repository error: {error}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                "Internal server error".to_string(),
            )
        }
    };

    (
        status,
        Json(serde_json::json!({ "code": code, "message": message })),
    )
        .into_response()
}

/// Unified error for handlers that validate input before calling the service.
#[derive(Debug)]
pub(crate) enum AssetHandlerError {
    Validation(String),
    Service(AssetError),
}

impl IntoResponse for AssetHandlerError {
    fn into_response(self) -> Response {
        match self {
            Self::Validation(message) => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "code": "VALIDATION_ERROR", "message": message })),
            )
                .into_response(),
            Self::Service(error) => asset_error_response(error),
        }
    }
}

impl From<AssetError> for AssetHandlerError {
    fn from(error: AssetError) -> Self {
        Self::Service(error)
    }
}

fn price_error_response(error: PriceError) -> Response {
    let (status, code, message): (StatusCode, &'static str, String) = match error {
        PriceError::NotFound => (
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "Asset not found".to_string(),
        ),
        PriceError::MissingYahooTicker => (
            StatusCode::BAD_REQUEST,
            "MISSING_YAHOO_TICKER",
            "Asset does not have a Yahoo Finance ticker".to_string(),
        ),
        PriceError::ExternalService(error) => {
            (StatusCode::BAD_GATEWAY, "EXTERNAL_SERVICE_ERROR", error)
        }
        PriceError::Repository(error) => {
            tracing::error!("Price repository error: {error}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                "Internal server error".to_string(),
            )
        }
    };

    (
        status,
        Json(serde_json::json!({ "code": code, "message": message })),
    )
        .into_response()
}

/// Unified error for asset-price handlers.
#[derive(Debug)]
pub(crate) enum AssetPriceHandlerError {
    Validation(String),
    Service(PriceError),
}

impl IntoResponse for AssetPriceHandlerError {
    fn into_response(self) -> Response {
        match self {
            Self::Validation(message) => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "code": "VALIDATION_ERROR", "message": message })),
            )
                .into_response(),
            Self::Service(error) => price_error_response(error),
        }
    }
}

impl From<PriceError> for AssetPriceHandlerError {
    fn from(error: PriceError) -> Self {
        Self::Service(error)
    }
}

/// Query parameters for paginated and filterable asset list endpoints.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct AssetListQuery {
    /// Page number (1-based, default 1).
    #[serde(default = "default_page")]
    pub page: u32,
    /// Items per page (default 25, max 100).
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Optional asset class filter.
    pub asset_class: Option<ApiAssetClass>,
    /// Optional case-insensitive name substring search.
    pub search: Option<String>,
}

fn default_page() -> u32 {
    1
}

fn default_limit() -> u32 {
    25
}

impl AssetListQuery {
    fn pagination(&self) -> PaginationParams {
        PaginationParams {
            page: self.page.max(1),
            limit: self.limit.clamp(1, 100),
        }
    }

    fn filters(&self) -> AssetFilters {
        AssetFilters {
            asset_class: self.asset_class.map(Into::into),
            name_search: self.search.clone().filter(|value| !value.trim().is_empty()),
        }
    }
}

/// Pagination metadata included in asset list responses.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[schema(title = "AssetPaginationMetaResponse")]
pub struct PaginationMetaResponse {
    /// Current page (1-based).
    pub page: u32,
    /// Items per page.
    pub limit: u32,
    /// Total number of items across all pages.
    pub total_items: u64,
    /// Total number of pages.
    pub total_pages: u32,
}

/// Paginated asset list response.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[schema(title = "PaginatedAssetResponse")]
pub struct PaginatedAssetResponse {
    /// Assets on the current page.
    pub data: Vec<AssetResponse>,
    /// Pagination metadata.
    pub pagination: PaginationMetaResponse,
}

impl From<PaginatedResult<domain::Asset>> for PaginatedAssetResponse {
    fn from(result: PaginatedResult<domain::Asset>) -> Self {
        Self {
            data: result.data.into_iter().map(AssetResponse::from).collect(),
            pagination: PaginationMetaResponse {
                page: result.pagination.page,
                limit: result.pagination.limit,
                total_items: result.pagination.total_items,
                total_pages: result.pagination.total_pages,
            },
        }
    }
}

/// Query parameters for price-history requests.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct AssetPriceHistoryQuery {
    /// Page number (1-based, default 1).
    #[serde(default = "default_page")]
    pub page: u32,
    /// Items per page (default 25, max 100).
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Optional inclusive lower bound.
    pub from_date: Option<chrono::NaiveDate>,
    /// Optional inclusive upper bound.
    pub to_date: Option<chrono::NaiveDate>,
}

impl AssetPriceHistoryQuery {
    fn pagination(&self) -> PaginationParams {
        PaginationParams {
            page: self.page.max(1),
            limit: self.limit.clamp(1, 100),
        }
    }
}

/// Paginated price-history response.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[schema(title = "PaginatedPriceHistoryResponse")]
pub struct PaginatedPriceHistoryResponse {
    /// Price-history rows on the current page.
    pub data: Vec<PriceRecordResponse>,
    /// Pagination metadata.
    pub pagination: PaginationMetaResponse,
}

impl From<PaginatedResult<domain::PriceRecord>> for PaginatedPriceHistoryResponse {
    fn from(result: PaginatedResult<domain::PriceRecord>) -> Self {
        Self {
            data: result
                .data
                .into_iter()
                .map(PriceRecordResponse::from)
                .collect(),
            pagination: PaginationMetaResponse {
                page: result.pagination.page,
                limit: result.pagination.limit,
                total_items: result.pagination.total_items,
                total_pages: result.pagination.total_pages,
            },
        }
    }
}

/// Build the assets sub-router mounted under `/api/v1/assets`.
pub fn assets_router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/assets", get(list_assets).post(create_asset))
        .route("/api/v1/assets/{isin}", get(get_asset).put(update_asset))
        .route(
            "/api/v1/assets/{isin}/refresh-price",
            axum::routing::post(refresh_asset_price),
        )
        .route(
            "/api/v1/assets/{isin}/price",
            axum::routing::put(put_manual_asset_price),
        )
        .route(
            "/api/v1/assets/{isin}/price-history",
            get(list_asset_price_history),
        )
}

/// List shared assets with pagination and optional filters.
#[utoipa::path(
    get,
    path = "/api/v1/assets",
    params(AssetListQuery),
    responses(
        (status = 200, description = "Paginated list of assets", body = PaginatedAssetResponse),
        (status = 401, description = "Missing or invalid token"),
    ),
    security(("bearer_auth" = [])),
    tag = "assets"
)]
#[tracing::instrument(skip(state, _auth_user))]
pub(crate) async fn list_assets(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Query(query): Query<AssetListQuery>,
) -> Result<Json<PaginatedAssetResponse>, AssetHandlerError> {
    let result = state
        .asset_service
        .list(&query.pagination(), &query.filters())
        .await?;
    Ok(Json(PaginatedAssetResponse::from(result)))
}

/// Return a single shared asset by ISIN.
#[utoipa::path(
    get,
    path = "/api/v1/assets/{isin}",
    params(("isin" = String, Path, description = "Asset ISIN")),
    responses(
        (status = 200, description = "Asset details", body = AssetResponse),
        (status = 401, description = "Missing or invalid token"),
        (status = 404, description = "Asset not found"),
    ),
    security(("bearer_auth" = [])),
    tag = "assets"
)]
#[tracing::instrument(skip(state, _auth_user))]
pub(crate) async fn get_asset(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(isin): Path<String>,
) -> Result<Json<AssetResponse>, AssetHandlerError> {
    application::assets::is_valid_isin(&isin, &())
        .map_err(|error| AssetHandlerError::Validation(error.to_string()))?;
    let asset = state.asset_service.get(&isin).await?;
    Ok(Json(AssetResponse::from(asset)))
}

/// Create a new shared asset. If `yahoo_ticker` is omitted, OpenFIGI lookup is attempted.
#[utoipa::path(
    post,
    path = "/api/v1/assets",
    request_body = CreateAssetRequest,
    responses(
        (status = 201, description = "Asset created", body = AssetResponse),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Missing or invalid token"),
        (status = 409, description = "Duplicate ISIN"),
    ),
    security(("bearer_auth" = [])),
    tag = "assets"
)]
#[tracing::instrument(skip(state, _auth_user))]
pub(crate) async fn create_asset(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<CreateAssetRequest>,
) -> Result<(StatusCode, Json<AssetResponse>), AssetHandlerError> {
    body.validate()
        .map_err(|error| AssetHandlerError::Validation(error.to_string()))?;

    let asset = state
        .asset_service
        .create(domain::NewAsset {
            isin: body.isin,
            yahoo_ticker: body.yahoo_ticker,
            name: body.name,
            asset_class: body.asset_class.into(),
            currency: body.currency,
            exchange: body.exchange,
        })
        .await?;

    Ok((StatusCode::CREATED, Json(AssetResponse::from(asset))))
}

/// Update an existing shared asset by ISIN.
#[utoipa::path(
    put,
    path = "/api/v1/assets/{isin}",
    params(("isin" = String, Path, description = "Asset ISIN")),
    request_body = UpdateAssetRequest,
    responses(
        (status = 200, description = "Updated asset", body = AssetResponse),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Missing or invalid token"),
        (status = 404, description = "Asset not found"),
    ),
    security(("bearer_auth" = [])),
    tag = "assets"
)]
#[tracing::instrument(skip(state, _auth_user))]
pub(crate) async fn update_asset(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(isin): Path<String>,
    Json(body): Json<UpdateAssetRequest>,
) -> Result<Json<AssetResponse>, AssetHandlerError> {
    application::assets::is_valid_isin(&isin, &())
        .map_err(|error| AssetHandlerError::Validation(error.to_string()))?;
    body.validate()
        .map_err(|error| AssetHandlerError::Validation(error.to_string()))?;

    let asset = state
        .asset_service
        .update(
            &isin,
            UpdateAsset {
                yahoo_ticker: body.yahoo_ticker,
                name: body.name,
                asset_class: body.asset_class.into(),
                currency: body.currency,
                exchange: body.exchange,
            },
        )
        .await?;

    Ok(Json(AssetResponse::from(asset)))
}

/// Force a Yahoo price refresh for a single asset.
#[utoipa::path(
    post,
    path = "/api/v1/assets/{isin}/refresh-price",
    params(("isin" = String, Path, description = "Asset ISIN")),
    responses(
        (status = 200, description = "Refreshed price", body = PriceRecordResponse),
        (status = 400, description = "Validation or missing Yahoo ticker"),
        (status = 401, description = "Missing or invalid token"),
        (status = 404, description = "Asset not found"),
    ),
    security(("bearer_auth" = [])),
    tag = "assets"
)]
#[tracing::instrument(skip(state, _auth_user))]
pub(crate) async fn refresh_asset_price(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(isin): Path<String>,
) -> Result<Json<PriceRecordResponse>, AssetPriceHandlerError> {
    application::assets::is_valid_isin(&isin, &())
        .map_err(|error| AssetPriceHandlerError::Validation(error.to_string()))?;

    let record = state.price_service.refresh(&isin).await?;
    Ok(Json(PriceRecordResponse::from(record)))
}

/// Insert or override a manual daily close price for a single asset.
#[utoipa::path(
    put,
    path = "/api/v1/assets/{isin}/price",
    params(("isin" = String, Path, description = "Asset ISIN")),
    request_body = ManualPriceEntryRequest,
    responses(
        (status = 200, description = "Stored manual price", body = PriceRecordResponse),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Missing or invalid token"),
        (status = 404, description = "Asset not found"),
    ),
    security(("bearer_auth" = [])),
    tag = "assets"
)]
#[tracing::instrument(skip(state, _auth_user))]
pub(crate) async fn put_manual_asset_price(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(isin): Path<String>,
    Json(body): Json<ManualPriceEntryRequest>,
) -> Result<Json<PriceRecordResponse>, AssetPriceHandlerError> {
    application::assets::is_valid_isin(&isin, &())
        .map_err(|error| AssetPriceHandlerError::Validation(error.to_string()))?;
    body.validate()
        .map_err(|error| AssetPriceHandlerError::Validation(error.to_string()))?;

    let record = state
        .price_service
        .store_manual_price(&isin, body.date, body.close_price, body.currency)
        .await?;

    Ok(Json(PriceRecordResponse::from(record)))
}

/// List stored price history for a single asset.
#[utoipa::path(
    get,
    path = "/api/v1/assets/{isin}/price-history",
    params(
        ("isin" = String, Path, description = "Asset ISIN"),
        AssetPriceHistoryQuery
    ),
    responses(
        (status = 200, description = "Paginated price history", body = PaginatedPriceHistoryResponse),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Missing or invalid token"),
        (status = 404, description = "Asset not found"),
    ),
    security(("bearer_auth" = [])),
    tag = "assets"
)]
#[tracing::instrument(skip(state, _auth_user))]
pub(crate) async fn list_asset_price_history(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(isin): Path<String>,
    Query(query): Query<AssetPriceHistoryQuery>,
) -> Result<Json<PaginatedPriceHistoryResponse>, AssetPriceHandlerError> {
    application::assets::is_valid_isin(&isin, &())
        .map_err(|error| AssetPriceHandlerError::Validation(error.to_string()))?;

    let result = state
        .price_service
        .list_history(&isin, query.from_date, query.to_date, &query.pagination())
        .await?;

    Ok(Json(PaginatedPriceHistoryResponse::from(result)))
}
