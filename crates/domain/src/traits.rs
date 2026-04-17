//! Domain trait definitions.
//!
//! Traits declared here are implemented by infrastructure crates (e.g. `db`,
//! `importer`, `price-fetcher`). The domain crate depends only on these
//! abstractions, never on concrete implementations.
