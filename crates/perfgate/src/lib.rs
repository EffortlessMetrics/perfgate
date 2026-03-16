//! # perfgate
//! 
//! High-performance, modular Rust library for performance budgeting and baseline diffing.
//! 
//! This is a facade crate that re-exports functionality from the core perfgate micro-crates.

pub use perfgate_types as types;
pub use perfgate_domain as domain;
pub use perfgate_adapters as adapters;
pub use perfgate_app as app;
pub use perfgate_error as error;
pub use perfgate_validation as validation;
pub use perfgate_stats as stats;
pub use perfgate_significance as significance;
pub use perfgate_budget as budget;
pub use perfgate_render as render;
pub use perfgate_sensor as sensor;
pub use perfgate_export as export;
pub use perfgate_paired as paired;
pub use perfgate_host_detect as host_detect;
pub use perfgate_sha256 as sha256;

// Common re-exports for ergonomic use
pub mod prelude {
    pub use perfgate_app::{CheckUseCase, CompareUseCase, RunBenchUseCase};
    pub use perfgate_domain::{compare_runs, compute_stats};
    pub use perfgate_types::{
        CompareReceipt, ConfigFile, Metric, MetricStatistic, RunReceipt, VerdictStatus,
    };
}
