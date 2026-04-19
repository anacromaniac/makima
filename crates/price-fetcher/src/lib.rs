//! # Price Fetcher Crate
//!
//! This crate provides clients for fetching financial data from external APIs.
//!
//! ## Data Sources
//! - Yahoo Finance: Asset prices and exchange rates
//! - OpenFIGI: ISIN to ticker mapping
//!
//! ## Dependencies
//! - `domain`: Contains domain models for prices and exchange rates
//!
//! ## Features
//! - HTTP client via reqwest (rustls-tls only)
//! - Periodic job scheduling via tokio-cron-scheduler
//! - Rate limiting and retry logic for API calls

pub mod backfill;
pub mod job;
pub mod openfigi;
pub mod yahoo;
