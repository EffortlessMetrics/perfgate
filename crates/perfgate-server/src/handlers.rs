//! API handlers for the baseline service.
//!
//! This module implements the REST API endpoints for baseline management.

mod baselines;
mod health;

pub use baselines::*;
pub use health::*;
