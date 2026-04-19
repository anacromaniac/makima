//! Authentication use cases and token helpers.

use std::sync::Arc;

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use chrono::{Duration, Utc};
use domain::{NewRefreshToken, RefreshTokenRepository, RepositoryError, UserRepository};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Claims embedded in every JWT access token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject: the authenticated user's UUID.
    pub sub: Uuid,
    /// Expiry as a Unix timestamp (seconds since epoch).
    pub exp: i64,
}

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
    /// JWT signing failure.
    #[error("jwt error: {0}")]
    JwtError(#[from] jsonwebtoken::errors::Error),
}

/// Application service that manages authentication and session workflows.
#[derive(Clone)]
pub struct AuthService {
    user_repo: Arc<dyn UserRepository>,
    token_repo: Arc<dyn RefreshTokenRepository>,
    jwt_secret: String,
}

impl AuthService {
    /// Create a new auth service.
    pub fn new(
        user_repo: Arc<dyn UserRepository>,
        token_repo: Arc<dyn RefreshTokenRepository>,
        jwt_secret: String,
    ) -> Self {
        Self {
            user_repo,
            token_repo,
            jwt_secret,
        }
    }

    /// Register a new user and return an access + refresh token pair.
    pub async fn register(&self, email: &str, password: &str) -> Result<TokenPair, AuthError> {
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|error| AuthError::HashError(error.to_string()))?;

        let user = self
            .user_repo
            .create(email, &hash.to_string())
            .await
            .map_err(|error| match error {
                RepositoryError::Conflict(_) => AuthError::EmailAlreadyExists,
                other => AuthError::Repository(other),
            })?;

        self.generate_token_pair(user.id).await
    }

    /// Authenticate with email + password and return a token pair.
    pub async fn login(&self, email: &str, password: &str) -> Result<TokenPair, AuthError> {
        let user = self
            .user_repo
            .find_by_email(email)
            .await
            .map_err(AuthError::Repository)?
            .ok_or(AuthError::InvalidCredentials)?;

        let hash = PasswordHash::new(&user.password_hash)
            .map_err(|error| AuthError::HashError(error.to_string()))?;

        Argon2::default()
            .verify_password(password.as_bytes(), &hash)
            .map_err(|_| AuthError::InvalidCredentials)?;

        self.generate_token_pair(user.id).await
    }

    /// Rotate a valid refresh token into a new token pair.
    pub async fn refresh(&self, refresh_token: &str) -> Result<TokenPair, AuthError> {
        let token_hash = hash_refresh_token(refresh_token);

        let record = self
            .token_repo
            .find_by_hash(&token_hash)
            .await
            .map_err(AuthError::Repository)?
            .ok_or(AuthError::InvalidToken)?;

        if record.expires_at < Utc::now() {
            return Err(AuthError::InvalidToken);
        }

        if record.revoked {
            self.token_repo
                .revoke_all_for_user(record.user_id)
                .await
                .map_err(AuthError::Repository)?;
            return Err(AuthError::TokenRevoked);
        }

        self.token_repo
            .revoke(record.id)
            .await
            .map_err(AuthError::Repository)?;

        self.generate_token_pair(record.user_id).await
    }

    /// Change a user's password and revoke all active refresh tokens.
    pub async fn change_password(
        &self,
        user_id: Uuid,
        old_password: &str,
        new_password: &str,
    ) -> Result<(), AuthError> {
        let user = self
            .user_repo
            .find_by_id(user_id)
            .await
            .map_err(AuthError::Repository)?
            .ok_or(AuthError::InvalidCredentials)?;

        let hash = PasswordHash::new(&user.password_hash)
            .map_err(|error| AuthError::HashError(error.to_string()))?;

        Argon2::default()
            .verify_password(old_password.as_bytes(), &hash)
            .map_err(|_| AuthError::InvalidCredentials)?;

        let salt = SaltString::generate(&mut OsRng);
        let new_hash = Argon2::default()
            .hash_password(new_password.as_bytes(), &salt)
            .map_err(|error| AuthError::HashError(error.to_string()))?;

        self.user_repo
            .update_password(user_id, &new_hash.to_string())
            .await
            .map_err(AuthError::Repository)?;

        self.token_repo
            .revoke_all_for_user(user_id)
            .await
            .map_err(AuthError::Repository)?;

        Ok(())
    }

    /// Create and persist a fresh token pair for `user_id`.
    async fn generate_token_pair(&self, user_id: Uuid) -> Result<TokenPair, AuthError> {
        let access_token = create_access_token(user_id, &self.jwt_secret)?;
        let refresh_token = generate_refresh_token();
        let token_hash = hash_refresh_token(&refresh_token);
        let expires_at = Utc::now() + Duration::days(7);

        self.token_repo
            .create(&NewRefreshToken {
                user_id,
                token_hash,
                expires_at,
            })
            .await
            .map_err(AuthError::Repository)?;

        Ok(TokenPair {
            access_token,
            refresh_token,
        })
    }
}

/// Sign a new access token for `user_id`, valid for 1 hour.
pub fn create_access_token(
    user_id: Uuid,
    secret: &str,
) -> Result<String, jsonwebtoken::errors::Error> {
    let exp = (Utc::now() + Duration::hours(1)).timestamp();
    let claims = Claims { sub: user_id, exp };

    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

/// Generate a cryptographically random opaque refresh token string.
pub fn generate_refresh_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            use std::fmt::Write as _;
            write!(output, "{byte:02x}").expect("write to String is infallible");
            output
        })
}

/// Return the SHA-256 hex digest of a refresh token string.
pub fn hash_refresh_token(token: &str) -> String {
    let hash = Sha256::digest(token.as_bytes());
    hash.iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            use std::fmt::Write as _;
            write!(output, "{byte:02x}").expect("write to String is infallible");
            output
        })
}
