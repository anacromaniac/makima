//! Request and response DTOs for auth endpoints.

use garde::Validate;
use serde::{Deserialize, Serialize};

/// Request body for `POST /api/v1/auth/register` and `POST /api/v1/auth/login`.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
pub struct RegisterRequest {
    /// User's email address.
    #[garde(email)]
    pub email: String,
    /// Plaintext password (minimum 8 characters).
    #[garde(length(min = 8))]
    pub password: String,
}

/// Request body for `POST /api/v1/auth/login`.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
pub struct LoginRequest {
    /// User's email address.
    #[garde(email)]
    pub email: String,
    /// Plaintext password.
    #[garde(length(min = 8))]
    pub password: String,
}

/// Request body for `POST /api/v1/auth/refresh`.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
pub struct RefreshRequest {
    /// Opaque refresh token string.
    #[garde(length(min = 1))]
    pub refresh_token: String,
}

/// Request body for `PUT /api/v1/auth/password`.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
pub struct ChangePasswordRequest {
    /// Current password (used for verification).
    #[garde(length(min = 8))]
    pub old_password: String,
    /// New password to set.
    #[garde(length(min = 8))]
    pub new_password: String,
}

/// Response body containing an access/refresh token pair.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TokenResponse {
    /// Short-lived JWT access token (1 hour).
    pub access_token: String,
    /// Long-lived opaque refresh token (7 days).
    pub refresh_token: String,
}
