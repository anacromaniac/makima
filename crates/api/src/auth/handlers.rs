//! HTTP handlers for auth endpoints.

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{post, put},
};
use garde::Validate;

use crate::{
    auth::{
        dto::{
            ChangePasswordRequest, LoginRequest, RefreshRequest, RegisterRequest, TokenResponse,
        },
        service::{self, AuthError},
    },
    state::AppState,
};

use super::AuthenticatedUser;

// ── IntoResponse impl lives here so the service layer stays axum-free ────────

impl IntoResponse for AuthError {
    fn into_response(self) -> axum::response::Response {
        let (status, code, message): (StatusCode, &'static str, String) = match self {
            AuthError::EmailAlreadyExists => (
                StatusCode::CONFLICT,
                "EMAIL_ALREADY_EXISTS",
                "Email already registered".to_string(),
            ),
            AuthError::InvalidCredentials => (
                StatusCode::UNAUTHORIZED,
                "INVALID_CREDENTIALS",
                "Invalid credentials".to_string(),
            ),
            AuthError::InvalidToken => (
                StatusCode::UNAUTHORIZED,
                "INVALID_TOKEN",
                "Token is expired or invalid".to_string(),
            ),
            AuthError::TokenRevoked => (
                StatusCode::UNAUTHORIZED,
                "TOKEN_REVOKED",
                "Token has been revoked — all sessions invalidated".to_string(),
            ),
            AuthError::Repository(e) => {
                tracing::error!("Auth repository error: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "Internal server error".to_string(),
                )
            }
            AuthError::HashError(msg) => {
                tracing::error!("Auth hash error: {msg}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "Internal server error".to_string(),
                )
            }
            AuthError::JwtError(e) => {
                tracing::error!("Auth JWT error: {e}");
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

// ── Router ────────────────────────────────────────────────────────────────────

/// Build the auth sub-router mounted under `/api/v1/auth`.
pub fn auth_router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/auth/register", post(register))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/refresh", post(refresh))
        .route("/api/v1/auth/password", put(change_password))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// Register a new user account and return a token pair.
#[utoipa::path(
    post,
    path = "/api/v1/auth/register",
    request_body = RegisterRequest,
    responses(
        (status = 201, description = "Registration successful", body = TokenResponse),
        (status = 409, description = "Email already registered"),
        (status = 422, description = "Validation error"),
    ),
    tag = "auth"
)]
pub(crate) async fn register(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<impl IntoResponse, AuthError> {
    if let Err(e) = payload.validate() {
        return Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "code": "VALIDATION_ERROR", "message": e.to_string() })),
        )
            .into_response());
    }

    let pair = service::register(
        state.user_repo.as_ref(),
        state.refresh_token_repo.as_ref(),
        &state.jwt_secret,
        &payload.email,
        &payload.password,
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(TokenResponse {
            access_token: pair.access_token,
            refresh_token: pair.refresh_token,
        }),
    )
        .into_response())
}

/// Authenticate with email + password and return a token pair.
#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = TokenResponse),
        (status = 401, description = "Invalid credentials"),
        (status = 422, description = "Validation error"),
    ),
    tag = "auth"
)]
pub(crate) async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<impl IntoResponse, AuthError> {
    if let Err(e) = payload.validate() {
        return Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "code": "VALIDATION_ERROR", "message": e.to_string() })),
        )
            .into_response());
    }

    let pair = service::login(
        state.user_repo.as_ref(),
        state.refresh_token_repo.as_ref(),
        &state.jwt_secret,
        &payload.email,
        &payload.password,
    )
    .await?;

    Ok(Json(TokenResponse {
        access_token: pair.access_token,
        refresh_token: pair.refresh_token,
    })
    .into_response())
}

/// Exchange a refresh token for a new token pair.
#[utoipa::path(
    post,
    path = "/api/v1/auth/refresh",
    request_body = RefreshRequest,
    responses(
        (status = 200, description = "Token refreshed", body = TokenResponse),
        (status = 401, description = "Token expired, invalid, or revoked"),
        (status = 422, description = "Validation error"),
    ),
    tag = "auth"
)]
pub(crate) async fn refresh(
    State(state): State<AppState>,
    Json(payload): Json<RefreshRequest>,
) -> Result<impl IntoResponse, AuthError> {
    if let Err(e) = payload.validate() {
        return Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "code": "VALIDATION_ERROR", "message": e.to_string() })),
        )
            .into_response());
    }

    let pair = service::refresh(
        state.refresh_token_repo.as_ref(),
        &state.jwt_secret,
        &payload.refresh_token,
    )
    .await?;

    Ok(Json(TokenResponse {
        access_token: pair.access_token,
        refresh_token: pair.refresh_token,
    })
    .into_response())
}

/// Change the authenticated user's password and revoke all active sessions.
#[utoipa::path(
    put,
    path = "/api/v1/auth/password",
    request_body = ChangePasswordRequest,
    responses(
        (status = 204, description = "Password changed successfully"),
        (status = 401, description = "Invalid credentials or bad token"),
        (status = 422, description = "Validation error"),
    ),
    security(("bearer_auth" = [])),
    tag = "auth"
)]
pub(crate) async fn change_password(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(payload): Json<ChangePasswordRequest>,
) -> Result<impl IntoResponse, AuthError> {
    if let Err(e) = payload.validate() {
        return Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "code": "VALIDATION_ERROR", "message": e.to_string() })),
        )
            .into_response());
    }

    service::change_password(
        state.user_repo.as_ref(),
        state.refresh_token_repo.as_ref(),
        auth_user.user_id,
        &payload.old_password,
        &payload.new_password,
    )
    .await?;

    Ok(StatusCode::NO_CONTENT.into_response())
}
