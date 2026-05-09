//! # perfgate
//!
//! High-performance, modular Rust library for performance budgeting and baseline diffing.
//!
//! This is the primary public library crate for perfgate. It keeps the
//! clean-architecture seams as modules without making every seam a public
//! package.
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

pub mod app;
pub mod domain;

pub use domain::budget;
pub use domain::paired;
pub use domain::significance;
pub use domain::stats;
pub use perfgate_types as types;
pub use perfgate_types::error;
// validation is now part of types
pub use perfgate_types::validation;

/// Compatibility path for runtime process and host adapters.
///
/// Prefer [`crate::runtime`] in new code.
pub mod adapters {
    pub use crate::runtime::*;
}

/// Integrations for external benchmark formats and CI platforms.
pub mod integrations;

/// Optional helpers for emitting probe JSONL.
///
/// Enable the `probe` feature to use the explicit JSONL helpers, or
/// `probe-tracing` to add the optional tracing layer:
///
/// ```toml
/// perfgate = { version = "0.15", features = ["probe"] }
/// perfgate = { version = "0.15", features = ["probe-tracing"] }
/// ```
#[cfg(feature = "probe")]
pub mod probe;

/// Runtime helpers for optional diagnostics and local execution support.
pub mod runtime;

/// Presentation helpers for rendering human- and CI-facing output.
pub mod presentation {
    /// CSV, JSONL, HTML, Prometheus, and JUnit export helpers.
    pub use crate::app::export;

    /// Markdown, annotation, and summary rendering.
    pub use crate::app::render;

    /// Sensor report generation for cockpit-style integrations.
    pub use crate::app::sensor;

    /// Summary table rendering.
    ///
    /// Prefer [`render::summary`] in new code; this preserves the documented
    /// presentation summary path during the 0.16 public-surface migration.
    pub mod summary {
        pub use crate::app::render::summary::*;
    }
}

/// CSV, JSONL, HTML, Prometheus, and JUnit export helpers.
///
/// Prefer [`crate::presentation::export`] in new code; this module preserves
/// the previous facade spelling during the 0.16 public-surface migration.
pub use app::export;

/// Sensor report generation for cockpit-style integrations.
///
/// Prefer [`crate::presentation::sensor`] in new code; this module preserves
/// the previous facade spelling during the 0.16 public-surface migration.
pub use app::sensor;

/// Markdown, annotation, and summary rendering.
///
/// Prefer [`crate::presentation::render`] in new code; this module preserves
/// the previous facade spelling during the 0.16 public-surface migration.
pub use app::render;

/// Core I/O-free building blocks for performance-gating policy.
pub mod core {
    pub use crate::domain::budget;
    pub use crate::domain::significance;
    pub use crate::domain::stats;
    pub use perfgate_types::fingerprint;
}

/// Deterministic fingerprint helpers.
///
/// Prefer [`crate::core::fingerprint`] in new code; this module preserves the
/// previous facade spelling during the 0.16 public-surface migration.
pub mod sha256 {
    pub use perfgate_types::fingerprint::*;
}

/// Host mismatch detection helpers.
///
/// Prefer [`crate::domain::host`] in new code; this module preserves the
/// previous facade spelling during the 0.16 public-surface migration.
pub mod host_detect {
    pub use crate::domain::host::*;
}

// Common re-exports for ergonomic use
pub mod prelude {
    pub use crate::app::{CheckUseCase, CompareUseCase, RunBenchUseCase};
    pub use crate::domain::{compare_runs, compute_stats};
    #[cfg(feature = "probe-tracing")]
    pub use crate::probe::TracingProbeLayer;
    #[cfg(feature = "probe")]
    pub use crate::probe::{ProbeEvent, ProbeJsonlWriter, ProbeTimer, probe_event, probe_timer};
    pub use perfgate_types::{
        CompareReceipt, ConfigFile, Metric, MetricStatistic, RunReceipt, VerdictStatus,
    };
}
