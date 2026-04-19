//! DTOs for broker import endpoints.

use serde::{Deserialize, Serialize};

/// Row-level import error exposed by the API.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ImportRowErrorResponse {
    /// One-based row number in the uploaded file.
    pub row: u32,
    /// Human-readable validation message.
    pub message: String,
}

/// Successful broker import response.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ImportResponse {
    /// Number of imported transactions.
    pub transactions_imported: u64,
    /// ISINs auto-created during the import.
    pub assets_created: Vec<String>,
    /// Non-fatal warnings collected during the import.
    pub warnings: Vec<String>,
}

/// Error body returned when the import file contains invalid rows.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ImportErrorResponse {
    /// Stable machine-readable error code.
    pub code: String,
    /// Human-readable message.
    pub message: String,
    /// Row-level parser errors.
    pub row_errors: Vec<ImportRowErrorResponse>,
}
