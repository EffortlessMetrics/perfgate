//! # perfgate
//!
//! High-performance, modular Rust library for performance budgeting and baseline diffing.
//!
//! This is a facade crate that re-exports functionality from the core perfgate micro-crates.
//! Use it when you want a single dependency instead of picking individual sub-crates.
//!
//! See the [GitHub repository](https://github.com/EffortlessMetrics/perfgate) for full
//! documentation and usage examples.
//!
//! # Example
//!
//! ```
//! use perfgate::types::{ToolInfo, Metric, Direction};
//!
//! let tool = ToolInfo { name: "perfgate".into(), version: "1.0.0".into() };
//! assert_eq!(tool.name, "perfgate");
//!
//! assert_eq!(Metric::WallMs.default_direction(), Direction::Lower);
//! ```

pub use perfgate_adapters as adapters;
pub use perfgate_app as app;
pub use perfgate_budget as budget;
pub use perfgate_domain as domain;
pub use perfgate_domain::paired;
pub use perfgate_domain::significance;
pub use perfgate_domain::stats;
pub use perfgate_export as export;
pub use perfgate_host_detect as host_detect;
pub use perfgate_render as render;
pub use perfgate_sensor as sensor;
pub use perfgate_types as types;
pub use perfgate_types::error;
// validation is now part of types
pub use perfgate_types::validation;

/// Integrations for external benchmark formats and CI platforms.
pub mod integrations;

/// Runtime helpers for optional diagnostics and local execution support.
pub mod runtime;

/// Core I/O-free building blocks for performance-gating policy.
pub mod core {
    pub use perfgate_budget as budget;
    pub use perfgate_domain::significance;
    pub use perfgate_domain::stats;
    pub use perfgate_types::fingerprint;
}

/// Deterministic fingerprint helpers.
///
/// Prefer [`crate::core::fingerprint`] in new code; this module preserves the
/// previous facade spelling during the 0.16 public-surface migration.
pub mod sha256 {
    pub use perfgate_types::fingerprint::*;
}

// Common re-exports for ergonomic use
pub mod prelude {
    pub use perfgate_app::{CheckUseCase, CompareUseCase, RunBenchUseCase};
    pub use perfgate_domain::{compare_runs, compute_stats};
    pub use perfgate_types::{
        CompareReceipt, ConfigFile, Metric, MetricStatistic, RunReceipt, VerdictStatus,
    };
}
