//! Workspace-only compatibility wrapper for `perfgate::app`.
//!
//! New code should depend on `perfgate` and import app APIs through
//! `perfgate::app`.

pub use perfgate::app::*;
