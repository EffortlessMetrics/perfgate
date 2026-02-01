//! Perfgate workspace-level test utilities.
//!
//! This crate exists solely to support workspace-level integration tests,
//! particularly the BDD/cucumber tests in `tests/cucumber.rs`.
//!
//! The actual perfgate functionality is in the workspace member crates:
//! - `perfgate-types`: Shared types and JSON schemas
//! - `perfgate-domain`: Pure business logic
//! - `perfgate-adapters`: I/O adapters
//! - `perfgate-app`: Application use cases
//! - `perfgate` (perfgate-cli): CLI interface
