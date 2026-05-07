//! Workspace-only compatibility wrapper for `perfgate::domain`.
//!
//! New code should depend on `perfgate` and import domain APIs through
//! `perfgate::domain`.

pub use perfgate::domain::*;
