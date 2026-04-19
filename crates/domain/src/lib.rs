//! # Domain Crate
//!
//! This crate contains core domain models, business logic, and trait definitions.
//! It has zero dependencies on frameworks or database infrastructure.
//!
//! ## Purpose
//! - Defines all domain models (User, Portfolio, Transaction, Asset, etc.)
//! - Contains pure business logic and calculations
//! - Defines traits that other crates (like `db`) implement
//!
//! ## Design Principles
//! - No framework dependencies (no sqlx, no axum)
//! - No database dependencies
//! - Pure Rust code focused on business rules

pub mod error;
pub mod logic;
pub mod models;
pub mod traits;

// Convenience re-exports — consumers can write `domain::User` instead of
// `domain::models::user::User`.
pub use error::{DomainError, RepositoryError};
pub use logic::*;
pub use models::*;
pub use traits::{
    AssetRepository, BrokerImporter, PortfolioRepository, RefreshTokenRepository, UserRepository,
};
