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
        service,
    },
    state::AppState,
};

use super::AuthenticatedUser;

/// Build the auth sub-router mounted under `/api/v1/auth`.
pub fn auth_router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/auth/register", post(register))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/refresh", post(refresh))
        .route("/api/v1/auth/password", put(change_password))
}

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
pub async fn register(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<impl IntoResponse, service::AuthError> {
    if let Err(e) = payload.validate() {
        return Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "code": "VALIDATION_ERROR", "message": e.to_string() })),
        )
            .into_response());
    }

    let pair = service::register(
        &state.pool,
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
pub async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<impl IntoResponse, service::AuthError> {
    if let Err(e) = payload.validate() {
        return Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "code": "VALIDATION_ERROR", "message": e.to_string() })),
        )
            .into_response());
    }

    let pair = service::login(
        &state.pool,
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
pub async fn refresh(
    State(state): State<AppState>,
    Json(payload): Json<RefreshRequest>,
) -> Result<impl IntoResponse, service::AuthError> {
    if let Err(e) = payload.validate() {
        return Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "code": "VALIDATION_ERROR", "message": e.to_string() })),
        )
            .into_response());
    }

    let pair = service::refresh(&state.pool, &state.jwt_secret, &payload.refresh_token).await?;

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
pub async fn change_password(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(payload): Json<ChangePasswordRequest>,
) -> Result<impl IntoResponse, service::AuthError> {
    if let Err(e) = payload.validate() {
        return Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "code": "VALIDATION_ERROR", "message": e.to_string() })),
        )
            .into_response());
    }

    service::change_password(
        &state.pool,
        auth_user.user_id,
        &payload.old_password,
        &payload.new_password,
    )
    .await?;

    Ok(StatusCode::NO_CONTENT.into_response())
}
