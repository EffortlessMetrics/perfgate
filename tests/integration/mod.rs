//! Cross-crate integration tests for perfgate.
//!
//! These tests verify that the various crates in the perfgate ecosystem
//! integrate correctly with each other, covering:
//!
//! - Stats crate → Domain crate integration
//! - Validation → Types integration
//! - Host-detect → App layer integration
//! - Export → Render flow
//! - Sensor report building flow
//! - Budget significance testing flow
//! - Error propagation across crates

mod budget_significance_flow;
mod error_propagation;
mod export_render_flow;
mod host_detect_to_app;
mod sensor_flow;
mod stats_to_domain;
mod validation_to_types;
