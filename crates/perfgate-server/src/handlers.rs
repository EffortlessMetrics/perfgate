//! API handlers for the baseline service.
//!
//! This module implements the REST API endpoints for baseline management.

pub mod baselines;
pub mod dashboard;
pub mod health;

pub use baselines::*;
pub use dashboard::*;
pub use health::*;
