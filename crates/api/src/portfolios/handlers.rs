//! HTTP handlers for portfolio CRUD endpoints.

use application::{analytics::AnalyticsError, portfolios::PortfolioError};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use domain::{PaginatedResult, PaginationParams};
use garde::Validate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::AuthenticatedUser,
    portfolios::dto::{
        AssetAllocationEntry, CreatePortfolioRequest, PortfolioResponse, PortfolioSummaryResponse,
        UpdatePortfolioRequest,
    },
    state::AppState,
};

// ── Error types ───────────────────────────────────────────────────────────────

fn portfolio_error_response(error: PortfolioError) -> Response {
    let (status, code, message): (StatusCode, &'static str, String) = match error {
        PortfolioError::NotFound => (
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "Portfolio not found".to_string(),
        ),
        PortfolioError::Repository(e) => {
            tracing::error!("Portfolio repository error: {e}");
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

fn analytics_error_response(error: AnalyticsError) -> Response {
    let (status, code, message): (StatusCode, &'static str, String) = match error {
        AnalyticsError::NotFound => (
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "Portfolio not found".to_string(),
        ),
        AnalyticsError::Repository(error) => {
            tracing::error!("Portfolio analytics repository error: {error}");
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

/// Unified error for handlers that perform validation before calling the service.
#[derive(Debug)]
pub(crate) enum PortfolioHandlerError {
    Validation(String),
    Service(PortfolioError),
    Analytics(AnalyticsError),
}

impl IntoResponse for PortfolioHandlerError {
    fn into_response(self) -> Response {
        match self {
            Self::Validation(msg) => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "code": "VALIDATION_ERROR", "message": msg })),
            )
                .into_response(),
            Self::Service(e) => portfolio_error_response(e),
            Self::Analytics(e) => analytics_error_response(e),
        }
    }
}

impl From<PortfolioError> for PortfolioHandlerError {
    fn from(e: PortfolioError) -> Self {
        Self::Service(e)
    }
}

impl From<AnalyticsError> for PortfolioHandlerError {
    fn from(e: AnalyticsError) -> Self {
        Self::Analytics(e)
    }
}

// ── Pagination ────────────────────────────────────────────────────────────────

/// Query parameters for paginated list endpoints.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct PaginationQuery {
    /// Page number (1-based, default 1).
    #[serde(default = "default_page")]
    pub page: u32,
    /// Items per page (default 25, max 100).
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_page() -> u32 {
    1
}
fn default_limit() -> u32 {
    25
}

impl From<PaginationQuery> for PaginationParams {
    fn from(q: PaginationQuery) -> Self {
        PaginationParams {
            page: q.page.max(1),
            limit: q.limit.clamp(1, 100),
        }
    }
}

/// Pagination metadata included in list responses.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
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

/// Paginated portfolio list response.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PaginatedPortfolioResponse {
    /// Portfolios on the current page.
    pub data: Vec<PortfolioResponse>,
    /// Pagination metadata.
    pub pagination: PaginationMetaResponse,
}

impl From<PaginatedResult<domain::Portfolio>> for PaginatedPortfolioResponse {
    fn from(r: PaginatedResult<domain::Portfolio>) -> Self {
        Self {
            data: r.data.into_iter().map(PortfolioResponse::from).collect(),
            pagination: PaginationMetaResponse {
                page: r.pagination.page,
                limit: r.pagination.limit,
                total_items: r.pagination.total_items,
                total_pages: r.pagination.total_pages,
            },
        }
    }
}

// ── Router ────────────────────────────────────────────────────────────────────

/// Build the portfolios sub-router mounted under `/api/v1/portfolios`.
pub fn portfolios_router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/portfolios",
            get(list_portfolios).post(create_portfolio),
        )
        .route(
            "/api/v1/portfolios/{id}",
            get(get_portfolio)
                .put(update_portfolio)
                .delete(delete_portfolio),
        )
        .route(
            "/api/v1/portfolios/{id}/summary",
            get(get_portfolio_summary),
        )
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// List all portfolios belonging to the authenticated user.
#[utoipa::path(
    get,
    path = "/api/v1/portfolios",
    params(PaginationQuery),
    responses(
        (status = 200, description = "Paginated list of portfolios", body = PaginatedPortfolioResponse),
        (status = 401, description = "Missing or invalid token"),
    ),
    security(("bearer_auth" = [])),
    tag = "portfolios"
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn list_portfolios(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Query(pagination): Query<PaginationQuery>,
) -> Result<Json<PaginatedPortfolioResponse>, PortfolioHandlerError> {
    let params = PaginationParams::from(pagination);
    let result = state
        .portfolio_service
        .list(auth_user.user_id, &params)
        .await?;
    Ok(Json(PaginatedPortfolioResponse::from(result)))
}

/// Create a new portfolio owned by the authenticated user. Base currency is always EUR.
#[utoipa::path(
    post,
    path = "/api/v1/portfolios",
    request_body = CreatePortfolioRequest,
    responses(
        (status = 201, description = "Portfolio created", body = PortfolioResponse),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Missing or invalid token"),
    ),
    security(("bearer_auth" = [])),
    tag = "portfolios"
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn create_portfolio(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<CreatePortfolioRequest>,
) -> Result<(StatusCode, Json<PortfolioResponse>), PortfolioHandlerError> {
    body.validate()
        .map_err(|e| PortfolioHandlerError::Validation(e.to_string()))?;

    let portfolio = state
        .portfolio_service
        .create(auth_user.user_id, body.name, body.description)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(PortfolioResponse::from(portfolio)),
    ))
}

/// Get a single portfolio by ID. Returns 404 if not found or not owned by the caller.
#[utoipa::path(
    get,
    path = "/api/v1/portfolios/{id}",
    params(("id" = Uuid, Path, description = "Portfolio ID")),
    responses(
        (status = 200, description = "Portfolio details", body = PortfolioResponse),
        (status = 401, description = "Missing or invalid token"),
        (status = 404, description = "Portfolio not found"),
    ),
    security(("bearer_auth" = [])),
    tag = "portfolios"
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn get_portfolio(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<PortfolioResponse>, PortfolioHandlerError> {
    let portfolio = state.portfolio_service.get(auth_user.user_id, id).await?;
    Ok(Json(PortfolioResponse::from(portfolio)))
}

/// Get analytics summary for a single portfolio in EUR.
#[utoipa::path(
    get,
    path = "/api/v1/portfolios/{id}/summary",
    params(("id" = Uuid, Path, description = "Portfolio ID")),
    responses(
        (status = 200, description = "Portfolio analytics summary", body = PortfolioSummaryResponse),
        (status = 401, description = "Missing or invalid token"),
        (status = 404, description = "Portfolio not found"),
    ),
    security(("bearer_auth" = [])),
    tag = "portfolios"
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn get_portfolio_summary(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<PortfolioSummaryResponse>, PortfolioHandlerError> {
    let summary = state
        .analytics_service
        .summary(auth_user.user_id, id)
        .await?;

    Ok(Json(PortfolioSummaryResponse {
        total_value: summary.total_value,
        total_gain_loss_absolute: summary.total_gain_loss_absolute,
        total_gain_loss_percentage: summary.total_gain_loss_percentage,
        asset_allocation: summary
            .asset_allocation
            .into_iter()
            .map(|entry| AssetAllocationEntry {
                asset_class: entry.asset_class.into(),
                value: entry.value,
                percentage: entry.percentage,
            })
            .collect(),
        warnings: summary.warnings,
    }))
}

/// Update a portfolio's name and description.
#[utoipa::path(
    put,
    path = "/api/v1/portfolios/{id}",
    params(("id" = Uuid, Path, description = "Portfolio ID")),
    request_body = UpdatePortfolioRequest,
    responses(
        (status = 200, description = "Updated portfolio", body = PortfolioResponse),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Missing or invalid token"),
        (status = 404, description = "Portfolio not found"),
    ),
    security(("bearer_auth" = [])),
    tag = "portfolios"
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn update_portfolio(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdatePortfolioRequest>,
) -> Result<Json<PortfolioResponse>, PortfolioHandlerError> {
    body.validate()
        .map_err(|e| PortfolioHandlerError::Validation(e.to_string()))?;

    let portfolio = state
        .portfolio_service
        .update(auth_user.user_id, id, body.name, body.description)
        .await?;

    Ok(Json(PortfolioResponse::from(portfolio)))
}

/// Delete a portfolio and all its transactions (cascade).
#[utoipa::path(
    delete,
    path = "/api/v1/portfolios/{id}",
    params(("id" = Uuid, Path, description = "Portfolio ID")),
    responses(
        (status = 204, description = "Portfolio deleted"),
        (status = 401, description = "Missing or invalid token"),
        (status = 404, description = "Portfolio not found"),
    ),
    security(("bearer_auth" = [])),
    tag = "portfolios"
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn delete_portfolio(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, PortfolioHandlerError> {
    state
        .portfolio_service
        .delete(auth_user.user_id, id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
