//! JWT access token creation and verification.

use chrono::Utc;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Claims embedded in every JWT access token.
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// Subject: the authenticated user's UUID.
    pub sub: Uuid,
    /// Expiry as a Unix timestamp (seconds since epoch).
    pub exp: i64,
}

/// Sign a new access token for `user_id`, valid for 1 hour.
///
/// # Errors
/// Returns an error if signing fails (e.g. invalid secret encoding).
pub fn create_access_token(
    user_id: Uuid,
    secret: &str,
) -> Result<String, jsonwebtoken::errors::Error> {
    let exp = (Utc::now() + chrono::Duration::hours(1)).timestamp();
    let claims = Claims { sub: user_id, exp };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

/// Verify a JWT access token and return its claims.
///
/// # Errors
/// Returns an error if the token is invalid, expired, or has a bad signature.
pub fn verify_access_token(
    token: &str,
    secret: &str,
) -> Result<Claims, jsonwebtoken::errors::Error> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )?;
    Ok(data.claims)
}
