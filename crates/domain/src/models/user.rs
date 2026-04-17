//! User domain model.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// An authenticated user of the system.
///
/// The [`Debug`](std::fmt::Debug) implementation masks `password_hash` so it
/// never appears in logs or debug output.
#[derive(Clone, Serialize, Deserialize)]
pub struct User {
    /// Unique identifier (UUID v7).
    pub id: Uuid,
    /// User email address. Unique across the system.
    pub email: String,
    /// Argon2 password hash. Never logged or serialized to API responses.
    #[serde(skip_serializing)]
    pub password_hash: String,
    /// When the user was created.
    pub created_at: DateTime<Utc>,
    /// When the user was last updated.
    pub updated_at: DateTime<Utc>,
}

impl std::fmt::Debug for User {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("User")
            .field("id", &self.id)
            .field("email", &self.email)
            .field("password_hash", &"[REDACTED]")
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

/// Data needed to register a new user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewUser {
    /// Email address.
    pub email: String,
    /// Plaintext password — hashed before persistence.
    pub password: String,
}
