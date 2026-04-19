//! HTTP handlers for user profile endpoints.

use application::users::UserError;
use axum::{Json, Router, extract::State, http::StatusCode, response::IntoResponse, routing::get};

use crate::{auth::AuthenticatedUser, state::AppState, users::dto::UserResponse};

#[derive(Debug)]
pub(crate) struct UserHandlerError(UserError);

impl IntoResponse for UserHandlerError {
    fn into_response(self) -> axum::response::Response {
        let (status, code, message): (StatusCode, &'static str, String) = match self.0 {
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

impl From<UserError> for UserHandlerError {
    fn from(value: UserError) -> Self {
        Self(value)
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
) -> Result<Json<UserResponse>, UserHandlerError> {
    let user = state.user_service.get_me(auth_user.user_id).await?;

    Ok(Json(UserResponse::from(user)))
}
