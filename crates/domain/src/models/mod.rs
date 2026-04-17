//! Domain model definitions.
//!
//! This module contains all domain structs, enums, and new-type wrappers used
//! across the application. Every other crate depends on these types; they must
//! remain free of framework and database imports.

pub mod asset;
pub mod exchange_rate;
pub mod pagination;
pub mod portfolio;
pub mod price;
pub mod refresh_token;
pub mod transaction;
pub mod user;

// Re-export enums and models for convenient access.
pub use asset::{Asset, AssetClass, NewAsset};
pub use exchange_rate::{ExchangeRate, NewExchangeRate};
pub use pagination::{PaginatedResult, PaginationMeta, PaginationParams};
pub use portfolio::{NewPortfolio, Portfolio};
pub use price::{NewPriceRecord, PriceRecord, PriceSource};
pub use refresh_token::{NewRefreshToken, RefreshToken};
pub use transaction::{NewTransaction, Transaction, TransactionType};
pub use user::{NewUser, User};
