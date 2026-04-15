//! # Database Crate
//!
//! This crate contains repository implementations, migrations, and PostgreSQL access via sqlx.
//! All database access goes through repository structs in this crate.
//!
//! ## Dependencies
//! - `domain`: Contains domain models that repositories work with
//!
//! ## Design Principles
//! - Repository pattern: each entity has a dedicated repository module
//! - No raw sqlx calls outside repositories
//! - Migrations are embedded in the binary via `sqlx::migrate!()`
