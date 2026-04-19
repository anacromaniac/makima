//! HTTP handlers for transaction CRUD endpoints.

use application::transactions::{CreateTransactionInput, TransactionError, UpdateTransactionInput};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use domain::{PaginatedResult, PaginationParams, TransactionFilters};
use garde::Validate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::AuthenticatedUser,
    state::AppState,
    transactions::dto::{
        ApiTransactionType, CreateTransactionRequest, TransactionResponse,
        UpdateTransactionRequest, validate_transaction_request,
    },
};

fn transaction_error_response(error: TransactionError) -> Response {
    let (status, code, message): (StatusCode, &'static str, String) = match error {
        TransactionError::NotFound | TransactionError::PortfolioNotFound => (
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "Transaction not found".to_string(),
        ),
        TransactionError::AssetResolutionFailed(isin) => (
            StatusCode::BAD_REQUEST,
            "ASSET_RESOLUTION_FAILED",
            format!("Unable to resolve asset metadata for ISIN {isin}"),
        ),
        TransactionError::ExchangeRateRequired(currency) => (
            StatusCode::BAD_REQUEST,
            "EXCHANGE_RATE_REQUIRED",
            format!("Exchange rate to EUR is required for currency {currency}"),
        ),
        TransactionError::InsufficientQuantity {
            available,
            requested,
        } => (
            StatusCode::BAD_REQUEST,
            "INSUFFICIENT_QUANTITY",
            format!("Insufficient quantity: available {available}, requested {requested}"),
        ),
        TransactionError::Repository(error) => {
            tracing::error!("Transaction repository error: {error}");
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
pub(crate) enum TransactionHandlerError {
    Validation(String),
    Service(TransactionError),
}

impl IntoResponse for TransactionHandlerError {
    fn into_response(self) -> Response {
        match self {
            Self::Validation(message) => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "code": "VALIDATION_ERROR", "message": message })),
            )
                .into_response(),
            Self::Service(error) => transaction_error_response(error),
        }
    }
}

impl From<TransactionError> for TransactionHandlerError {
    fn from(error: TransactionError) -> Self {
        Self::Service(error)
    }
}

/// Query parameters for paginated and filterable transaction lists.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct TransactionListQuery {
    /// Page number (1-based, default 1).
    #[serde(default = "default_page")]
    pub page: u32,
    /// Items per page (default 25, max 100).
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Optional transaction type filter.
    pub transaction_type: Option<ApiTransactionType>,
    /// Optional asset filter.
    pub asset_id: Option<Uuid>,
    /// Optional inclusive date lower bound.
    pub date_from: Option<chrono::NaiveDate>,
    /// Optional inclusive date upper bound.
    pub date_to: Option<chrono::NaiveDate>,
}

fn default_page() -> u32 {
    1
}

fn default_limit() -> u32 {
    25
}

impl TransactionListQuery {
    fn pagination(&self) -> PaginationParams {
        PaginationParams {
            page: self.page.max(1),
            limit: self.limit.clamp(1, 100),
        }
    }

    fn filters(&self) -> TransactionFilters {
        TransactionFilters {
            transaction_type: self.transaction_type.map(Into::into),
            asset_id: self.asset_id,
            date_from: self.date_from,
            date_to: self.date_to,
        }
    }
}

/// Pagination metadata included in transaction list responses.
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

/// Paginated transaction list response.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PaginatedTransactionResponse {
    /// Transactions on the current page.
    pub data: Vec<TransactionResponse>,
    /// Pagination metadata.
    pub pagination: PaginationMetaResponse,
}

impl From<PaginatedResult<application::transactions::TransactionDetails>>
    for PaginatedTransactionResponse
{
    fn from(result: PaginatedResult<application::transactions::TransactionDetails>) -> Self {
        Self {
            data: result
                .data
                .into_iter()
                .map(TransactionResponse::from)
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

/// Build the transactions sub-router.
pub fn transactions_router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/portfolios/{id}/transactions",
            get(list_transactions).post(create_transaction),
        )
        .route(
            "/api/v1/transactions/{id}",
            get(get_transaction)
                .put(update_transaction)
                .delete(delete_transaction),
        )
}

/// List transactions for a single portfolio.
#[utoipa::path(
    get,
    path = "/api/v1/portfolios/{id}/transactions",
    params(("id" = Uuid, Path, description = "Portfolio ID"), TransactionListQuery),
    responses(
        (status = 200, description = "Paginated list of transactions", body = PaginatedTransactionResponse),
        (status = 401, description = "Missing or invalid token"),
        (status = 404, description = "Portfolio not found"),
    ),
    security(("bearer_auth" = [])),
    tag = "transactions"
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn list_transactions(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Query(query): Query<TransactionListQuery>,
) -> Result<Json<PaginatedTransactionResponse>, TransactionHandlerError> {
    let result = state
        .transaction_service
        .list(auth_user.user_id, id, &query.pagination(), &query.filters())
        .await?;

    Ok(Json(PaginatedTransactionResponse::from(result)))
}

/// Create a transaction in the specified portfolio.
#[utoipa::path(
    post,
    path = "/api/v1/portfolios/{id}/transactions",
    params(("id" = Uuid, Path, description = "Portfolio ID")),
    request_body = CreateTransactionRequest,
    responses(
        (status = 201, description = "Transaction created", body = TransactionResponse),
        (status = 400, description = "Validation or business-rule error"),
        (status = 401, description = "Missing or invalid token"),
        (status = 404, description = "Portfolio not found"),
    ),
    security(("bearer_auth" = [])),
    tag = "transactions"
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn create_transaction(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Json(body): Json<CreateTransactionRequest>,
) -> Result<(StatusCode, Json<TransactionResponse>), TransactionHandlerError> {
    body.validate()
        .map_err(|error| TransactionHandlerError::Validation(error.to_string()))?;
    validate_transaction_request(
        body.transaction_type,
        body.quantity,
        body.unit_price,
        body.gross_amount,
        body.net_amount,
    )
    .map_err(TransactionHandlerError::Validation)?;

    let transaction = state
        .transaction_service
        .create(
            auth_user.user_id,
            CreateTransactionInput {
                portfolio_id: id,
                asset_isin: body.asset_isin,
                transaction_type: body.transaction_type.into(),
                date: body.date,
                settlement_date: body.settlement_date,
                quantity: body.quantity,
                unit_price: body.unit_price,
                commission: body.commission.unwrap_or_default(),
                currency: body.currency,
                exchange_rate_to_base: body.exchange_rate_to_base,
                gross_amount: body.gross_amount,
                tax_withheld: body.tax_withheld,
                net_amount: body.net_amount,
                notes: body.notes,
            },
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(TransactionResponse::from(transaction)),
    ))
}

/// Get one transaction by ID.
#[utoipa::path(
    get,
    path = "/api/v1/transactions/{id}",
    params(("id" = Uuid, Path, description = "Transaction ID")),
    responses(
        (status = 200, description = "Transaction details", body = TransactionResponse),
        (status = 401, description = "Missing or invalid token"),
        (status = 404, description = "Transaction not found"),
    ),
    security(("bearer_auth" = [])),
    tag = "transactions"
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn get_transaction(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<TransactionResponse>, TransactionHandlerError> {
    let transaction = state.transaction_service.get(auth_user.user_id, id).await?;
    Ok(Json(TransactionResponse::from(transaction)))
}

/// Update one transaction by ID.
#[utoipa::path(
    put,
    path = "/api/v1/transactions/{id}",
    params(("id" = Uuid, Path, description = "Transaction ID")),
    request_body = UpdateTransactionRequest,
    responses(
        (status = 200, description = "Updated transaction", body = TransactionResponse),
        (status = 400, description = "Validation or business-rule error"),
        (status = 401, description = "Missing or invalid token"),
        (status = 404, description = "Transaction not found"),
    ),
    security(("bearer_auth" = [])),
    tag = "transactions"
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn update_transaction(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateTransactionRequest>,
) -> Result<Json<TransactionResponse>, TransactionHandlerError> {
    body.validate()
        .map_err(|error| TransactionHandlerError::Validation(error.to_string()))?;
    validate_transaction_request(
        body.transaction_type,
        body.quantity,
        body.unit_price,
        body.gross_amount,
        body.net_amount,
    )
    .map_err(TransactionHandlerError::Validation)?;

    let transaction = state
        .transaction_service
        .update(
            auth_user.user_id,
            id,
            UpdateTransactionInput {
                asset_isin: body.asset_isin,
                transaction_type: body.transaction_type.into(),
                date: body.date,
                settlement_date: body.settlement_date,
                quantity: body.quantity,
                unit_price: body.unit_price,
                commission: body.commission.unwrap_or_default(),
                currency: body.currency,
                exchange_rate_to_base: body.exchange_rate_to_base,
                gross_amount: body.gross_amount,
                tax_withheld: body.tax_withheld,
                net_amount: body.net_amount,
                notes: body.notes,
            },
        )
        .await?;

    Ok(Json(TransactionResponse::from(transaction)))
}

/// Delete one transaction by ID.
#[utoipa::path(
    delete,
    path = "/api/v1/transactions/{id}",
    params(("id" = Uuid, Path, description = "Transaction ID")),
    responses(
        (status = 204, description = "Transaction deleted"),
        (status = 400, description = "Deletion would violate no-short-sell constraints"),
        (status = 401, description = "Missing or invalid token"),
        (status = 404, description = "Transaction not found"),
    ),
    security(("bearer_auth" = [])),
    tag = "transactions"
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn delete_transaction(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, TransactionHandlerError> {
    state
        .transaction_service
        .delete(auth_user.user_id, id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
