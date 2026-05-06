//! Compatibility wrapper for perfgate's absorbed error contract.
//!
//! Error types now live in [`perfgate_types::error`]. This crate remains as a
//! workspace-only migration shim while internal tests move off
//! `perfgate_error` imports.

pub use perfgate_types::error::*;
