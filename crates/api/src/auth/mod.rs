//! Authentication module: JWT, refresh tokens, registration, login, and password change.

pub mod dto;
pub mod handlers;
pub mod jwt;
pub mod service;
pub mod tokens;

use axum::{Json, extract::FromRequestParts, http::StatusCode};
use uuid::Uuid;

use crate::state::AppState;

/// Extractor that requires a valid Bearer JWT in the `Authorization` header.
///
/// Used by protected endpoints to obtain the authenticated user's ID without
/// hitting the database on every request.
pub struct AuthenticatedUser {
    /// The authenticated user's UUID, extracted from the JWT `sub` claim.
    pub user_id: Uuid,
}

impl<S> FromRequestParts<S> for AuthenticatedUser
where
    S: Send + Sync,
    AppState: axum::extract::FromRef<S>,
{
    type Rejection = (StatusCode, Json<serde_json::Value>);

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        use axum::extract::FromRef;

        let app_state = AppState::from_ref(state);

        let auth_header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({
                        "code": "UNAUTHORIZED",
                        "message": "Missing Authorization header"
                    })),
                )
            })?;

        let token = auth_header.strip_prefix("Bearer ").ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "code": "UNAUTHORIZED",
                    "message": "Authorization header must use Bearer scheme"
                })),
            )
        })?;

        let claims = jwt::verify_access_token(token, &app_state.jwt_secret).map_err(|_| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "code": "UNAUTHORIZED",
                    "message": "Invalid or expired access token"
                })),
            )
        })?;

        Ok(AuthenticatedUser {
            user_id: claims.sub,
        })
    }
}
