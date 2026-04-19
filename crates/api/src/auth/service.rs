//! Auth service — business logic for registration, login, token refresh, and password change.

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use axum::{Json, http::StatusCode, response::IntoResponse};
use chrono::{Duration, Utc};
use db::repositories::{refresh_token_repo::RefreshTokenRepository, user_repo::UserRepository};
use domain::NewRefreshToken;
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::{jwt, tokens};

/// Token pair returned on successful authentication or refresh.
#[derive(Debug)]
pub struct TokenPair {
    /// Short-lived JWT access token.
    pub access_token: String,
    /// Long-lived opaque refresh token.
    pub refresh_token: String,
}

/// Errors that can occur during auth operations.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// Registration attempted with an email that already exists.
    #[error("email already registered")]
    EmailAlreadyExists,
    /// Login attempted with wrong email or password.
    #[error("invalid credentials")]
    InvalidCredentials,
    /// Refresh token not found or expired.
    #[error("token is expired or invalid")]
    InvalidToken,
    /// A previously rotated (stolen) refresh token was presented.
    #[error("token has been revoked")]
    TokenRevoked,
    /// Underlying database error.
    #[error("database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
    /// Argon2 hashing failure.
    #[error("password hashing error: {0}")]
    HashError(String),
    /// JWT signing/verification failure.
    #[error("jwt error: {0}")]
    JwtError(#[from] jsonwebtoken::errors::Error),
}

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
            AuthError::DatabaseError(e) => {
                tracing::error!("Auth database error: {e}");
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

/// Create a fresh access + refresh token pair and persist the refresh token.
async fn generate_token_pair(
    pool: &PgPool,
    user_id: Uuid,
    jwt_secret: &str,
) -> Result<TokenPair, AuthError> {
    let access_token = jwt::create_access_token(user_id, jwt_secret)?;

    let refresh_token_str = tokens::generate_refresh_token();
    let token_hash = tokens::hash_refresh_token(&refresh_token_str);
    let expires_at = Utc::now() + Duration::days(7);

    RefreshTokenRepository::create(
        pool,
        &NewRefreshToken {
            user_id,
            token_hash,
            expires_at,
        },
    )
    .await?;

    Ok(TokenPair {
        access_token,
        refresh_token: refresh_token_str,
    })
}

/// Register a new user: validate, hash password, create user, return token pair.
///
/// Returns [`AuthError::EmailAlreadyExists`] if the email is already taken.
pub async fn register(
    pool: &PgPool,
    jwt_secret: &str,
    email: &str,
    password: &str,
) -> Result<TokenPair, AuthError> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| AuthError::HashError(e.to_string()))?;

    let user = UserRepository::create(pool, email, &hash.to_string())
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db_err) = e
                && db_err.code().as_deref() == Some("23505")
            {
                return AuthError::EmailAlreadyExists;
            }
            AuthError::DatabaseError(e)
        })?;

    generate_token_pair(pool, user.id, jwt_secret).await
}

/// Authenticate with email + password and return a token pair.
///
/// Returns [`AuthError::InvalidCredentials`] for any wrong credential — email not found
/// and wrong password produce the same error to prevent user enumeration.
pub async fn login(
    pool: &PgPool,
    jwt_secret: &str,
    email: &str,
    password: &str,
) -> Result<TokenPair, AuthError> {
    let user = UserRepository::find_by_email(pool, email)
        .await?
        .ok_or(AuthError::InvalidCredentials)?;

    let hash =
        PasswordHash::new(&user.password_hash).map_err(|e| AuthError::HashError(e.to_string()))?;

    Argon2::default()
        .verify_password(password.as_bytes(), &hash)
        .map_err(|_| AuthError::InvalidCredentials)?;

    generate_token_pair(pool, user.id, jwt_secret).await
}

/// Exchange a valid refresh token for a new token pair (rotation).
///
/// If the incoming token is already revoked (stolen token detection), **all** refresh
/// tokens for that user are immediately revoked and [`AuthError::TokenRevoked`] is returned.
pub async fn refresh(
    pool: &PgPool,
    jwt_secret: &str,
    refresh_token: &str,
) -> Result<TokenPair, AuthError> {
    let token_hash = tokens::hash_refresh_token(refresh_token);

    let record = RefreshTokenRepository::find_by_hash(pool, &token_hash)
        .await?
        .ok_or(AuthError::InvalidToken)?;

    if record.expires_at < Utc::now() {
        return Err(AuthError::InvalidToken);
    }

    // Stolen token detection: a previously rotated token was re-used.
    if record.revoked {
        RefreshTokenRepository::revoke_all_for_user(pool, record.user_id).await?;
        return Err(AuthError::TokenRevoked);
    }

    RefreshTokenRepository::revoke(pool, record.id).await?;
    generate_token_pair(pool, record.user_id, jwt_secret).await
}

/// Change the authenticated user's password and revoke all refresh tokens.
///
/// Returns [`AuthError::InvalidCredentials`] if `old_password` is wrong.
pub async fn change_password(
    pool: &PgPool,
    user_id: Uuid,
    old_password: &str,
    new_password: &str,
) -> Result<(), AuthError> {
    let user = UserRepository::find_by_id(pool, user_id)
        .await?
        .ok_or(AuthError::InvalidCredentials)?;

    let hash =
        PasswordHash::new(&user.password_hash).map_err(|e| AuthError::HashError(e.to_string()))?;

    Argon2::default()
        .verify_password(old_password.as_bytes(), &hash)
        .map_err(|_| AuthError::InvalidCredentials)?;

    let salt = SaltString::generate(&mut OsRng);
    let new_hash = Argon2::default()
        .hash_password(new_password.as_bytes(), &salt)
        .map_err(|e| AuthError::HashError(e.to_string()))?;

    UserRepository::update_password(pool, user_id, &new_hash.to_string()).await?;
    RefreshTokenRepository::revoke_all_for_user(pool, user_id).await?;

    Ok(())
}
