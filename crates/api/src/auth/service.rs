//! Auth service — business logic for registration, login, token refresh, and password change.
//!
//! This module has no axum or sqlx dependencies. It works exclusively through
//! the [`domain::UserRepository`] and [`domain::RefreshTokenRepository`] ports.

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use chrono::{Duration, Utc};
use domain::{NewRefreshToken, RefreshTokenRepository, RepositoryError, UserRepository};
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
    /// Underlying repository error (storage failure).
    #[error("repository error: {0}")]
    Repository(RepositoryError),
    /// Argon2 hashing failure.
    #[error("password hashing error: {0}")]
    HashError(String),
    /// JWT signing/verification failure.
    #[error("jwt error: {0}")]
    JwtError(#[from] jsonwebtoken::errors::Error),
}

/// Create a fresh access + refresh token pair and persist the refresh token.
async fn generate_token_pair(
    token_repo: &dyn RefreshTokenRepository,
    user_id: Uuid,
    jwt_secret: &str,
) -> Result<TokenPair, AuthError> {
    let access_token = jwt::create_access_token(user_id, jwt_secret)?;

    let refresh_token_str = tokens::generate_refresh_token();
    let token_hash = tokens::hash_refresh_token(&refresh_token_str);
    let expires_at = Utc::now() + Duration::days(7);

    token_repo
        .create(&NewRefreshToken {
            user_id,
            token_hash,
            expires_at,
        })
        .await
        .map_err(AuthError::Repository)?;

    Ok(TokenPair {
        access_token,
        refresh_token: refresh_token_str,
    })
}

/// Register a new user: hash password, create user, return token pair.
///
/// Returns [`AuthError::EmailAlreadyExists`] if the email is already taken.
pub async fn register(
    user_repo: &dyn UserRepository,
    token_repo: &dyn RefreshTokenRepository,
    jwt_secret: &str,
    email: &str,
    password: &str,
) -> Result<TokenPair, AuthError> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| AuthError::HashError(e.to_string()))?;

    let user = user_repo
        .create(email, &hash.to_string())
        .await
        .map_err(|e| match e {
            RepositoryError::Conflict(_) => AuthError::EmailAlreadyExists,
            e => AuthError::Repository(e),
        })?;

    generate_token_pair(token_repo, user.id, jwt_secret).await
}

/// Authenticate with email + password and return a token pair.
///
/// Returns [`AuthError::InvalidCredentials`] for any wrong credential to prevent
/// user enumeration.
pub async fn login(
    user_repo: &dyn UserRepository,
    token_repo: &dyn RefreshTokenRepository,
    jwt_secret: &str,
    email: &str,
    password: &str,
) -> Result<TokenPair, AuthError> {
    let user = user_repo
        .find_by_email(email)
        .await
        .map_err(AuthError::Repository)?
        .ok_or(AuthError::InvalidCredentials)?;

    let hash =
        PasswordHash::new(&user.password_hash).map_err(|e| AuthError::HashError(e.to_string()))?;

    Argon2::default()
        .verify_password(password.as_bytes(), &hash)
        .map_err(|_| AuthError::InvalidCredentials)?;

    generate_token_pair(token_repo, user.id, jwt_secret).await
}

/// Exchange a valid refresh token for a new token pair (rotation).
///
/// If the incoming token is already revoked (stolen token detection), **all**
/// refresh tokens for that user are immediately revoked and
/// [`AuthError::TokenRevoked`] is returned.
pub async fn refresh(
    token_repo: &dyn RefreshTokenRepository,
    jwt_secret: &str,
    refresh_token: &str,
) -> Result<TokenPair, AuthError> {
    let token_hash = tokens::hash_refresh_token(refresh_token);

    let record = token_repo
        .find_by_hash(&token_hash)
        .await
        .map_err(AuthError::Repository)?
        .ok_or(AuthError::InvalidToken)?;

    if record.expires_at < Utc::now() {
        return Err(AuthError::InvalidToken);
    }

    // Stolen token detection: a previously rotated token was re-used.
    if record.revoked {
        token_repo
            .revoke_all_for_user(record.user_id)
            .await
            .map_err(AuthError::Repository)?;
        return Err(AuthError::TokenRevoked);
    }

    token_repo
        .revoke(record.id)
        .await
        .map_err(AuthError::Repository)?;

    generate_token_pair(token_repo, record.user_id, jwt_secret).await
}

/// Change the authenticated user's password and revoke all refresh tokens.
///
/// Returns [`AuthError::InvalidCredentials`] if `old_password` is wrong.
pub async fn change_password(
    user_repo: &dyn UserRepository,
    token_repo: &dyn RefreshTokenRepository,
    user_id: Uuid,
    old_password: &str,
    new_password: &str,
) -> Result<(), AuthError> {
    let user = user_repo
        .find_by_id(user_id)
        .await
        .map_err(AuthError::Repository)?
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

    user_repo
        .update_password(user_id, &new_hash.to_string())
        .await
        .map_err(AuthError::Repository)?;

    token_repo
        .revoke_all_for_user(user_id)
        .await
        .map_err(AuthError::Repository)?;

    Ok(())
}
