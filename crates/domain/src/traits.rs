//! Domain trait definitions — the "ports" in the ports-and-adapters architecture.
//!
//! Traits declared here are implemented by infrastructure crates (`db`,
//! `importer`, `price-fetcher`). The domain and service layers depend only on
//! these abstractions, never on concrete implementations (sqlx, axum, etc.).

use async_trait::async_trait;
use uuid::Uuid;

use crate::error::{DomainError, RepositoryError};
use crate::models::{NewRefreshToken, NewTransaction, RefreshToken, User};

// ── Broker import ────────────────────────────────────────────────────────────

/// Parses a broker export file into a list of normalized transactions.
///
/// Each broker parser (Fineco, BG Saxo, …) implements this trait. The raw file
/// bytes are provided directly so the implementation can choose the appropriate
/// decoding strategy (e.g. Excel parsing via calamine).
pub trait BrokerImporter {
    /// Parse `file_bytes` and return a list of [`NewTransaction`] values.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError::ValidationError`] if the file cannot be parsed or
    /// contains invalid rows.
    fn parse(&self, file_bytes: &[u8]) -> Result<Vec<NewTransaction>, DomainError>;
}

// ── Repository ports ─────────────────────────────────────────────────────────

/// Persistent storage operations for user accounts.
///
/// Implementations live in the `db` crate. Tests may provide in-memory mocks.
#[async_trait]
pub trait UserRepository: Send + Sync {
    /// Persist a new user with the given email and pre-hashed password.
    ///
    /// Returns [`RepositoryError::Conflict`] if the email already exists.
    async fn create(&self, email: &str, password_hash: &str) -> Result<User, RepositoryError>;

    /// Find a user by email address. Returns `None` if not found.
    async fn find_by_email(&self, email: &str) -> Result<Option<User>, RepositoryError>;

    /// Find a user by primary key. Returns `None` if not found.
    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, RepositoryError>;

    /// Update the stored password hash for a user.
    async fn update_password(&self, id: Uuid, new_hash: &str) -> Result<(), RepositoryError>;
}

/// Persistent storage operations for refresh tokens.
///
/// Implementations live in the `db` crate. Tests may provide in-memory mocks.
#[async_trait]
pub trait RefreshTokenRepository: Send + Sync {
    /// Persist a new refresh token record.
    async fn create(&self, new_token: &NewRefreshToken) -> Result<RefreshToken, RepositoryError>;

    /// Look up a refresh token by its SHA-256 hash. Returns `None` if not found.
    async fn find_by_hash(&self, token_hash: &str)
    -> Result<Option<RefreshToken>, RepositoryError>;

    /// Mark a single refresh token as revoked.
    async fn revoke(&self, id: Uuid) -> Result<(), RepositoryError>;

    /// Revoke every refresh token belonging to a user (force logout on all devices).
    async fn revoke_all_for_user(&self, user_id: Uuid) -> Result<(), RepositoryError>;
}
