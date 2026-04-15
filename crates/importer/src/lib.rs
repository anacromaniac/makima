//! # Importer Crate
//!
//! This crate contains broker file parsers for importing transaction data.
//!
//! ## Supported Brokers
//! - Fineco
//! - BG Saxo
//!
//! ## Dependencies
//! - `domain`: Contains domain models and the `BrokerImporter` trait
//!
//! ## Design
//! - Pluggable parser system via `BrokerImporter` trait
//! - Parses Excel (.xlsx) files using calamine
//! - Returns normalized transaction lists
