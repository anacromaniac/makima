//! HTTP handlers for user profile endpoints.

use axum::{Json, Router, extract::State, http::StatusCode, response::IntoResponse, routing::get};
use domain::error::RepositoryError;
use thiserror::Error;

use crate::{auth::AuthenticatedUser, state::AppState, users::dto::UserResponse};

/// Errors that can occur in user profile operations.
#[derive(Debug, Error)]
pub enum UserError {
    /// The authenticated user's record was not found (should not happen with a valid JWT).
    #[error("User not found")]
    NotFound,
    /// A storage-layer error occurred.
    #[error("Repository error: {0}")]
    Repository(#[from] RepositoryError),
}

impl IntoResponse for UserError {
    fn into_response(self) -> axum::response::Response {
        let (status, code, message): (StatusCode, &'static str, String) = match self {
            UserError::NotFound => (
                StatusCode::NOT_FOUND,
                "NOT_FOUND",
                "User not found".to_string(),
            ),
            UserError::Repository(e) => {
                tracing::error!("User repository error: {e}");
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
}

/// Build the users sub-router mounted under `/api/v1/users`.
pub fn users_router() -> Router<AppState> {
    Router::new().route("/api/v1/users/me", get(get_me))
}

/// Return the authenticated user's profile.
///
/// Requires a valid Bearer JWT. Never returns the password hash.
#[utoipa::path(
    get,
    path = "/api/v1/users/me",
    responses(
        (status = 200, description = "Current user profile", body = UserResponse),
        (status = 401, description = "Missing or invalid token"),
    ),
    security(("bearer_auth" = [])),
    tag = "users"
)]
#[tracing::instrument(skip(state))]
pub(crate) async fn get_me(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<UserResponse>, UserError> {
    let user = state
        .user_repo
        .find_by_id(auth_user.user_id)
        .await?
        .ok_or(UserError::NotFound)?;

    Ok(Json(UserResponse::from(user)))
}
