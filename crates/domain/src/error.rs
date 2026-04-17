//! Domain error types.
//!
//! All business-layer errors are captured here. Infrastructure crates (db, api)
//! map these to their own error types or HTTP responses.

use rust_decimal::Decimal;

/// Errors that originate from domain-level business rules.
#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    /// The requested resource was not found.
    #[error("not found: {0}")]
    NotFound(String),

    /// Input failed domain-level validation.
    #[error("validation error: {0}")]
    ValidationError(String),

    /// An attempt to create a resource that already exists.
    #[error("duplicate entry: {0}")]
    DuplicateEntry(String),

    /// A sell would result in a negative position.
    #[error("insufficient quantity: available {available}, requested {requested}")]
    InsufficientQuantity {
        /// Quantity currently held.
        available: Decimal,
        /// Quantity the user attempted to sell.
        requested: Decimal,
    },

    /// An external service (Yahoo Finance, OpenFIGI) returned an error.
    #[error("external service error: {0}")]
    ExternalServiceError(String),
}
