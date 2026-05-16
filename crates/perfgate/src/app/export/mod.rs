//! Export formats for perfgate benchmarks.
//!
//! This module provides functionality for exporting run and compare receipts
//! to various formats suitable for trend analysis and time-series ingestion.
//!
//! Part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.
//!
//! # Supported Formats
//!
//! - **CSV**: RFC 4180 compliant CSV with header row
//! - **JSONL**: JSON Lines format (one JSON object per line)
//! - **HTML**: HTML summary table
//! - **Prometheus**: Prometheus text exposition format
//! - **JUnit**: JUnit XML format (for legacy CI/Jenkins)
//!
//! # Example
//!
//! ```
//! use perfgate::app::export::{ExportFormat, ExportUseCase};
//! use perfgate_types::*;
//! use std::collections::BTreeMap;
//!
//! let receipt = RunReceipt {
//!     schema: RUN_SCHEMA_V1.to_string(),
//!     tool: ToolInfo { name: "perfgate".into(), version: "0.1.0".into() },
//!     run: RunMeta {
//!         id: "r1".into(),
//!         started_at: "2024-01-01T00:00:00Z".into(),
//!         ended_at: "2024-01-01T00:00:01Z".into(),
//!         host: HostInfo { os: "linux".into(), arch: "x86_64".into(),
//!             cpu_count: None, memory_bytes: None, hostname_hash: None },
//!     },
//!     bench: BenchMeta {
//!         name: "bench".into(), cwd: None,
//!         command: vec!["echo".into()], repeat: 1, warmup: 0,
//!         work_units: None, timeout_ms: None,
//!     },
//!     samples: vec![Sample {
//!         wall_ms: 42, exit_code: 0, warmup: false, timed_out: false,
//!         cpu_ms: None, page_faults: None, ctx_switches: None,
//!         max_rss_kb: None, io_read_bytes: None, io_write_bytes: None,
//!         network_packets: None, energy_uj: None, binary_bytes: None, stdout: None, stderr: None,
//!     }],
//!     stats: Stats {
//!         wall_ms: U64Summary::new(42, 42, 42 ),
//!         cpu_ms: None, page_faults: None, ctx_switches: None,
//!         max_rss_kb: None, io_read_bytes: None, io_write_bytes: None,
//!         network_packets: None, energy_uj: None, binary_bytes: None, throughput_per_s: None,
//!     },
//! };
//!
//! // Export a run receipt to CSV
//! let csv = ExportUseCase::export_run(&receipt, ExportFormat::Csv).unwrap();
//! assert!(csv.contains("bench"));
//! ```

use perfgate_types::{CompareReceipt, RunReceipt};

mod escaping;
mod format;
mod mapping;
mod row_builders;
mod rows;
mod serializers;

pub use escaping::csv_escape;
#[cfg(test)]
use escaping::{html_escape, prometheus_escape_label_value};
pub use format::ExportFormat;
pub use rows::{CompareExportRow, RunExportRow};

/// Use case for exporting receipts to different formats.
pub struct ExportUseCase;

impl ExportUseCase {
    /// Export a [`RunReceipt`] to the specified format.
    ///
    /// ```
    /// # use std::collections::BTreeMap;
    /// # use perfgate_types::*;
    /// # use perfgate::app::export::{ExportFormat, ExportUseCase};
    /// let receipt = RunReceipt {
    ///     schema: RUN_SCHEMA_V1.to_string(),
    ///     tool: ToolInfo { name: "perfgate".into(), version: "0.1.0".into() },
    ///     run: RunMeta {
    ///         id: "r1".into(),
    ///         started_at: "2024-01-01T00:00:00Z".into(),
    ///         ended_at: "2024-01-01T00:00:01Z".into(),
    ///         host: HostInfo { os: "linux".into(), arch: "x86_64".into(),
    ///             cpu_count: None, memory_bytes: None, hostname_hash: None },
    ///     },
    ///     bench: BenchMeta {
    ///         name: "bench".into(), cwd: None,
    ///         command: vec!["echo".into()], repeat: 1, warmup: 0,
    ///         work_units: None, timeout_ms: None,
    ///     },
    ///     samples: vec![Sample {
    ///         wall_ms: 42, exit_code: 0, warmup: false, timed_out: false,
    ///         cpu_ms: None, page_faults: None, ctx_switches: None,
    ///         max_rss_kb: None, io_read_bytes: None, io_write_bytes: None,
    ///         network_packets: None, energy_uj: None, binary_bytes: None, stdout: None, stderr: None,
    ///     }],
    ///     stats: Stats {
    ///         wall_ms: U64Summary::new(42, 42, 42 ),
    ///         cpu_ms: None, page_faults: None, ctx_switches: None,
    ///         max_rss_kb: None, io_read_bytes: None, io_write_bytes: None,
    ///         network_packets: None, energy_uj: None, binary_bytes: None, throughput_per_s: None,
    ///     },
    /// };
    /// let csv = ExportUseCase::export_run(&receipt, ExportFormat::Csv).unwrap();
    /// assert!(csv.contains("bench"));
    /// assert!(csv.contains("42"));
    /// ```
    pub fn export_run(receipt: &RunReceipt, format: ExportFormat) -> anyhow::Result<String> {
        let row = row_builders::run_to_row(receipt);

        match format {
            ExportFormat::Csv => serializers::run_row_to_csv(&row),
            ExportFormat::Jsonl => serializers::run_row_to_jsonl(&row),
            ExportFormat::Html => serializers::run_row_to_html(&row),
            ExportFormat::Prometheus => serializers::run_row_to_prometheus(&row),
            ExportFormat::JUnit => serializers::run_row_to_junit_run(receipt, &row),
        }
    }

    pub fn export_compare(
        receipt: &CompareReceipt,
        format: ExportFormat,
    ) -> anyhow::Result<String> {
        let rows = row_builders::compare_to_rows(receipt);

        match format {
            ExportFormat::Csv => serializers::compare_rows_to_csv(&rows),
            ExportFormat::Jsonl => serializers::compare_rows_to_jsonl(&rows),
            ExportFormat::Html => serializers::compare_rows_to_html(&rows),
            ExportFormat::Prometheus => serializers::compare_rows_to_prometheus(&rows),
            ExportFormat::JUnit => serializers::compare_rows_to_junit(receipt, &rows),
        }
    }
}

#[cfg(test)]
mod property_tests;
#[cfg(test)]
mod tests;
