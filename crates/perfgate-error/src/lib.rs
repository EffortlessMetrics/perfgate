//! Compatibility wrapper for perfgate's absorbed error contract.
//!
//! Error types now live in [`perfgate_types::error`]. This crate remains as a
//! temporary 0.16 compatibility shim for existing `perfgate_error` imports.

pub use perfgate_types::error::*;
