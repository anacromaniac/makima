//! DTOs for user profile responses.

use chrono::{DateTime, Utc};
use domain::models::user::User;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Public user profile returned by the API.
///
/// Intentionally excludes `password_hash` and `updated_at`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UserResponse {
    /// Unique user identifier.
    pub id: Uuid,
    /// User email address.
    pub email: String,
    /// When the account was created.
    pub created_at: DateTime<Utc>,
}

impl From<User> for UserResponse {
    fn from(user: User) -> Self {
        Self {
            id: user.id,
            email: user.email,
            created_at: user.created_at,
        }
    }
}
