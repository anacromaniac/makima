//! HTTP handlers for broker import endpoints.

use application::import::{ImportError, ImportSummary};
use axum::{
    Json, Router,
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    auth::AuthenticatedUser,
    import::dto::{ImportErrorResponse, ImportResponse, ImportRowErrorResponse},
    state::AppState,
};

/// Query string for broker import requests.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ImportQuery {
    /// Target portfolio identifier.
    pub portfolio_id: Uuid,
}

/// Build the broker import sub-router.
pub fn import_router() -> Router<AppState> {
    Router::new().route("/api/v1/import/{broker}", post(import_transactions))
}

/// Import a broker file into an owned portfolio.
#[utoipa::path(
    post,
    path = "/api/v1/import/{broker}",
    params(
        ("broker" = String, Path, description = "Broker identifier: fineco | bgsaxo"),
        ImportQuery
    ),
    responses(
        (status = 200, description = "Import completed", body = ImportResponse),
        (status = 400, description = "Invalid broker or invalid file rows", body = ImportErrorResponse),
        (status = 401, description = "Missing or invalid token"),
        (status = 404, description = "Portfolio not found")
    ),
    security(("bearer_auth" = [])),
    tag = "import"
)]
#[tracing::instrument(skip(state, multipart))]
pub(crate) async fn import_transactions(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(broker): Path<String>,
    Query(query): Query<ImportQuery>,
    mut multipart: Multipart,
) -> Result<Json<ImportResponse>, ImportHandlerError> {
    let mut file_bytes = None;
    while let Some(field) = multipart.next_field().await.map_err(|error| {
        ImportHandlerError::Validation(format!("invalid multipart body: {error}"))
    })? {
        if field.name() == Some("file") {
            file_bytes = Some(field.bytes().await.map_err(|error| {
                ImportHandlerError::Validation(format!("unable to read uploaded file: {error}"))
            })?);
            break;
        }
    }

    let file_bytes = file_bytes.ok_or_else(|| {
        ImportHandlerError::Validation("multipart field 'file' is required".to_string())
    })?;

    let summary = state
        .import_service
        .import(auth_user.user_id, &broker, query.portfolio_id, &file_bytes)
        .await?;

    Ok(Json(ImportResponse::from(summary)))
}

/// Unified handler error for broker import endpoints.
#[derive(Debug)]
pub(crate) enum ImportHandlerError {
    Validation(String),
    Service(ImportError),
}

impl From<ImportError> for ImportHandlerError {
    fn from(error: ImportError) -> Self {
        Self::Service(error)
    }
}

impl IntoResponse for ImportHandlerError {
    fn into_response(self) -> Response {
        match self {
            Self::Validation(message) => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "code": "VALIDATION_ERROR", "message": message })),
            )
                .into_response(),
            Self::Service(error) => match error {
                ImportError::InvalidBroker(broker) => (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "code": "INVALID_BROKER",
                        "message": format!("Unsupported broker: {broker}")
                    })),
                )
                    .into_response(),
                ImportError::PortfolioNotFound => (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({
                        "code": "NOT_FOUND",
                        "message": "Portfolio not found"
                    })),
                )
                    .into_response(),
                ImportError::Parse(error) => (
                    StatusCode::BAD_REQUEST,
                    Json(ImportErrorResponse {
                        code: "IMPORT_PARSE_ERROR".to_string(),
                        message: "Import file contains invalid rows".to_string(),
                        row_errors: error
                            .row_errors
                            .into_iter()
                            .map(|error| ImportRowErrorResponse {
                                row: error.row,
                                message: error.message,
                            })
                            .collect(),
                    }),
                )
                    .into_response(),
                ImportError::Validation(message) => (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "code": "VALIDATION_ERROR",
                        "message": message
                    })),
                )
                    .into_response(),
                ImportError::Repository(error) => {
                    tracing::error!("Broker import repository error: {error}");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({
                            "code": "INTERNAL_ERROR",
                            "message": "Internal server error"
                        })),
                    )
                        .into_response()
                }
            },
        }
    }
}

impl From<ImportSummary> for ImportResponse {
    fn from(value: ImportSummary) -> Self {
        Self {
            transactions_imported: value.transactions_imported,
            assets_created: value.assets_created,
            warnings: value.warnings,
        }
    }
}
