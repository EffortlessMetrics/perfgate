//! Compatibility wrapper for paired benchmarking statistics.
//!
//! The implementation now lives in `perfgate-domain`; this crate remains as a
//! workspace-only migration shim while tests and fuzz targets move to the owner
//! crate.

pub use perfgate_domain::paired::*;
