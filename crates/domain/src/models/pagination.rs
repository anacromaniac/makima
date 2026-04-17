//! Pagination types for list endpoints.

use serde::{Deserialize, Serialize};

/// Parameters controlling page selection in a paginated listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationParams {
    /// 1-based page number.
    pub page: u32,
    /// Maximum number of items per page.
    pub limit: u32,
}

/// Metadata describing the pagination state of a response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationMeta {
    /// Current page number (1-based).
    pub page: u32,
    /// Requested page size.
    pub limit: u32,
    /// Total number of items across all pages.
    pub total_items: u64,
    /// Total number of pages.
    pub total_pages: u32,
}

/// A paginated collection of items.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResult<T> {
    /// Items on the current page.
    pub data: Vec<T>,
    /// Pagination metadata.
    pub pagination: PaginationMeta,
}
