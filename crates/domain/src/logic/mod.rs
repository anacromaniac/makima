//! Pure business-logic functions for portfolio calculations.
//!
//! Every function in this module is free of side-effects and framework
//! dependencies. They operate on domain models and primitive types only.

pub mod allocation;
pub mod gain_loss;
pub mod position;

pub use allocation::{AllocationEntry, AllocationPercent, calculate_allocation};
pub use gain_loss::{GainLoss, calculate_gain_loss};
pub use position::{Position, aggregate_position};
