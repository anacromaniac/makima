//! Refresh token domain model.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A long-lived, opaque refresh token used to obtain new access tokens.
///
/// Tokens are stored as SHA-256 hashes. Each token is single-use: after a
/// refresh, the old token is invalidated and a new pair is issued.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshToken {
    /// Unique identifier (UUID v7).
    pub id: Uuid,
    /// Owning user.
    pub user_id: Uuid,
    /// SHA-256 hash of the opaque token value.
    pub token_hash: String,
    /// When the token expires.
    pub expires_at: DateTime<Utc>,
    /// Whether the token has been revoked.
    pub revoked: bool,
    /// When the token record was created.
    pub created_at: DateTime<Utc>,
}

/// Data needed to persist a new refresh token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewRefreshToken {
    /// Owning user.
    pub user_id: Uuid,
    /// SHA-256 hash of the opaque token value.
    pub token_hash: String,
    /// When the token expires.
    pub expires_at: DateTime<Utc>,
}
