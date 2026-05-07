//! Compatibility wrapper for runtime process and host adapters.
//!
//! The runtime adapter implementation now lives in `perfgate_app::runtime`
//! and is exposed through the public facade at `perfgate::runtime`.

pub use perfgate_app::runtime::*;
