//! Domain trait definitions — the "ports" in the ports-and-adapters architecture.
//!
//! Traits declared here are implemented by infrastructure crates (`db`,
//! `importer`, `price-fetcher`). The domain and service layers depend only on
//! these abstractions, never on concrete implementations (sqlx, axum, etc.).

use async_trait::async_trait;
use uuid::Uuid;

use crate::error::{DomainError, RepositoryError};
use crate::models::{
    Asset, AssetFilters, NewAsset, NewPortfolio, NewRefreshToken, NewTransaction, PaginatedResult,
    PaginationParams, Portfolio, RefreshToken, Transaction, TransactionFilters, UpdateAsset,
    UpdateTransaction, User,
};

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

/// Persistent storage operations for portfolios.
///
/// Implementations live in the `db` crate. Tests may provide in-memory mocks.
#[async_trait]
pub trait PortfolioRepository: Send + Sync {
    /// Persist a new portfolio record.
    async fn create(&self, new_portfolio: &NewPortfolio) -> Result<Portfolio, RepositoryError>;

    /// Find a portfolio by primary key. Returns `None` if not found.
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Portfolio>, RepositoryError>;

    /// List all portfolios belonging to a user, with pagination.
    async fn find_by_user_id(
        &self,
        user_id: Uuid,
        pagination: &PaginationParams,
    ) -> Result<PaginatedResult<Portfolio>, RepositoryError>;

    /// Update the name and description of an existing portfolio.
    async fn update(
        &self,
        id: Uuid,
        name: &str,
        description: Option<&str>,
    ) -> Result<Portfolio, RepositoryError>;

    /// Hard-delete a portfolio by ID. Cascade deletes its transactions via FK.
    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError>;
}

/// Persistent storage operations for shared assets.
///
/// Assets are global reference data and are not owned by a specific user.
#[async_trait]
pub trait AssetRepository: Send + Sync {
    /// Persist a new asset record.
    async fn create(&self, new_asset: &NewAsset) -> Result<Asset, RepositoryError>;

    /// Find an asset by primary key. Returns `None` if not found.
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Asset>, RepositoryError>;

    /// Find an asset by ISIN. Returns `None` if not found.
    async fn find_by_isin(&self, isin: &str) -> Result<Option<Asset>, RepositoryError>;

    /// List assets using pagination and optional shared filters.
    async fn list(
        &self,
        pagination: &PaginationParams,
        filters: &AssetFilters,
    ) -> Result<PaginatedResult<Asset>, RepositoryError>;

    /// Update the mutable fields of an asset identified by primary key.
    async fn update(&self, id: Uuid, update: &UpdateAsset) -> Result<Asset, RepositoryError>;

    /// Update only the Yahoo Finance ticker for an existing asset.
    async fn update_yahoo_ticker(
        &self,
        id: Uuid,
        yahoo_ticker: Option<&str>,
    ) -> Result<(), RepositoryError>;
}

/// Persistent storage operations for transactions.
#[async_trait]
pub trait TransactionRepository: Send + Sync {
    /// Persist a new transaction record.
    async fn create(
        &self,
        new_transaction: &NewTransaction,
    ) -> Result<Transaction, RepositoryError>;

    /// Find a transaction by primary key. Returns `None` if not found.
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Transaction>, RepositoryError>;

    /// List transactions in a portfolio with pagination and optional filters.
    async fn find_by_portfolio(
        &self,
        portfolio_id: Uuid,
        pagination: &PaginationParams,
        filters: &TransactionFilters,
    ) -> Result<PaginatedResult<Transaction>, RepositoryError>;

    /// Return all transactions for a single asset in a portfolio, ordered chronologically.
    async fn list_by_asset(
        &self,
        portfolio_id: Uuid,
        asset_id: Uuid,
    ) -> Result<Vec<Transaction>, RepositoryError>;

    /// Replace the mutable fields of an existing transaction.
    async fn update(
        &self,
        id: Uuid,
        update: &UpdateTransaction,
    ) -> Result<Transaction, RepositoryError>;

    /// Hard-delete a transaction by ID.
    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError>;

    /// Return the currently held quantity for an asset inside a portfolio.
    async fn get_held_quantity(
        &self,
        portfolio_id: Uuid,
        asset_id: Uuid,
    ) -> Result<rust_decimal::Decimal, RepositoryError>;
}

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
