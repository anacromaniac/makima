//! Domain trait definitions.
//!
//! Traits declared here are implemented by infrastructure crates (e.g. `db`,
//! `importer`, `price-fetcher`). The domain crate depends only on these
//! abstractions, never on concrete implementations.

use crate::error::DomainError;
use crate::models::NewTransaction;

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
