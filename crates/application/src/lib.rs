//! # Application Crate
//!
//! This crate contains use-case orchestration and application-specific ports.
//! It sits between the domain core and concrete adapters such as HTTP, SQL,
//! and external API clients.

pub mod assets;
pub mod auth;
pub mod portfolios;
pub mod transactions;
pub mod users;
