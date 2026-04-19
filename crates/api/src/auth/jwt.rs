//! JWT access token creation and verification.

pub use application::auth::Claims;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};

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
