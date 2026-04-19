//! HTTP handlers for derived position endpoints.

use application::positions::PositionError;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{auth::AuthenticatedUser, positions::dto::PositionResponse, state::AppState};

fn position_error_response(error: PositionError) -> Response {
    let (status, code, message): (StatusCode, &'static str, String) = match error {
        PositionError::NotFound => (
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "Portfolio not found".to_string(),
        ),
        PositionError::Repository(error) => {
            tracing::error!("Position repository error: {error}");
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

/// Handler error for position requests.
#[derive(Debug)]
pub(crate) enum PositionHandlerError {
    Service(PositionError),
}

impl IntoResponse for PositionHandlerError {
    fn into_response(self) -> Response {
        match self {
            Self::Service(error) => position_error_response(error),
        }
    }
}

impl From<PositionError> for PositionHandlerError {
    fn from(error: PositionError) -> Self {
        Self::Service(error)
    }
}

/// Query parameters for position listing.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct PositionListQuery {
    /// Whether closed positions should be included. Defaults to `false`.
    #[serde(default)]
    pub show_closed: bool,
}

/// Build the positions sub-router.
pub fn positions_router() -> Router<AppState> {
    Router::new().route("/api/v1/portfolios/{id}/positions", get(list_positions))
}

/// List derived positions for a single portfolio.
#[utoipa::path(
    get,
    path = "/api/v1/portfolios/{id}/positions",
    params(
        ("id" = Uuid, Path, description = "Portfolio ID"),
        PositionListQuery
    ),
    responses(
        (status = 200, description = "Derived positions in the portfolio", body = [PositionResponse]),
        (status = 401, description = "Missing or invalid token"),
        (status = 404, description = "Portfolio not found"),
    ),
    security(("bearer_auth" = [])),
    tag = "positions"
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn list_positions(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Query(query): Query<PositionListQuery>,
) -> Result<Json<Vec<PositionResponse>>, PositionHandlerError> {
    let positions = state
        .position_service
        .list(auth_user.user_id, id, query.show_closed)
        .await?;

    Ok(Json(
        positions.into_iter().map(PositionResponse::from).collect(),
    ))
}
