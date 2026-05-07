//! Compatibility wrapper for baseline service API contracts.
//!
//! The baseline service wire contract now lives in
//! [`perfgate_types::baseline_service`]. This crate remains workspace-only so
//! older internal imports can migrate without keeping `perfgate-api` as a
//! public package surface.

pub use perfgate_types::baseline_service::*;

/// Compatibility path for baseline service auth contract types.
pub mod auth {
    pub use perfgate_types::baseline_service::auth::*;
}
