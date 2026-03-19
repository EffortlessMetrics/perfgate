//! Conversion from PerfgateReport to SensorReport envelope.
//!
//! This module provides `run_sensor_check()`, a library-linkable convenience function
//! so the cockpit binary can `use perfgate_app::run_sensor_check`.
//!
//! The sensor report building functionality is provided by the `perfgate-sensor` crate.
//! This module re-exports those types and functions for backward compatibility.

// Re-export sensor building functionality from perfgate-sensor
pub use perfgate_sensor::{
    BenchOutcome, SensorReportBuilder, default_engine_capability, sensor_fingerprint,
};

use crate::{CheckRequest, CheckUseCase, Clock};
use perfgate_adapters::{HostProbe, ProcessRunner};
use perfgate_error::{AdapterError, ConfigValidationError, IoError, PerfgateError};
use perfgate_types::{
    BASELINE_REASON_NO_BASELINE, ConfigFile, ERROR_KIND_EXEC, ERROR_KIND_IO, ERROR_KIND_PARSE,
    HostMismatchPolicy, MAX_FINDINGS_DEFAULT, RunReceipt, STAGE_BASELINE_RESOLVE,
    STAGE_CONFIG_PARSE, STAGE_RUN_COMMAND, STAGE_WRITE_ARTIFACTS, SensorReport, ToolInfo,
    validate_bench_name,
};

/// Options for `run_sensor_check`.
#[derive(Debug, Clone)]
pub struct SensorCheckOptions {
    pub require_baseline: bool,
    pub fail_on_warn: bool,
    pub env: Vec<(String, String)>,
    pub output_cap_bytes: usize,
    pub allow_nonzero: bool,
    pub host_mismatch_policy: HostMismatchPolicy,
    pub max_findings: Option<usize>,
}

impl Default for SensorCheckOptions {
    fn default() -> Self {
        Self {
            require_baseline: false,
            fail_on_warn: false,
            env: Vec::new(),
            output_cap_bytes: 8192,
            allow_nonzero: false,
            host_mismatch_policy: HostMismatchPolicy::Warn,
            max_findings: Some(MAX_FINDINGS_DEFAULT),
        }
    }
}

/// Run a sensor check and return a `SensorReport` directly.
#[allow(clippy::too_many_arguments)]
pub fn run_sensor_check<R, H, C>(
    runner: &R,
    host_probe: &H,
    clock: &C,
    config: &ConfigFile,
    bench_name: &str,
    baseline: Option<&RunReceipt>,
    tool: ToolInfo,
    options: SensorCheckOptions,
) -> SensorReport
where
    R: ProcessRunner + Clone,
    H: HostProbe + Clone,
    C: Clock + Clone,
{
    let started_at = clock.now_rfc3339();
    let start_instant = std::time::Instant::now();

    // Validate bench name early
    if let Err(err) = validate_bench_name(bench_name) {
        let ended_at = clock.now_rfc3339();
        let duration_ms = start_instant.elapsed().as_millis() as u64;
        let builder = SensorReportBuilder::new(tool, started_at)
            .ended_at(ended_at, duration_ms)
            .baseline(baseline.is_some(), None);
        return builder.build_error(&err.to_string(), STAGE_CONFIG_PARSE, ERROR_KIND_PARSE);
    }

    // Validate config
    if let Err(msg) = config.validate() {
        let ended_at = clock.now_rfc3339();
        let duration_ms = start_instant.elapsed().as_millis() as u64;
        let builder = SensorReportBuilder::new(tool, started_at)
            .ended_at(ended_at, duration_ms)
            .baseline(baseline.is_some(), None);
        return builder.build_error(
            &format!("config validation: {}", msg),
            STAGE_CONFIG_PARSE,
            ERROR_KIND_PARSE,
        );
    }

    let baseline_available = baseline.is_some();

    let result = CheckUseCase::new(runner.clone(), host_probe.clone(), clock.clone()).execute(
        CheckRequest {
            config: config.clone(),
            bench_name: bench_name.to_string(),
            out_dir: std::path::PathBuf::from("."),
            baseline: baseline.cloned(),
            baseline_path: None,
            require_baseline: options.require_baseline,
            fail_on_warn: options.fail_on_warn,
            tool: tool.clone(),
            env: options.env.clone(),
            output_cap_bytes: options.output_cap_bytes,
            allow_nonzero: options.allow_nonzero,
            host_mismatch_policy: options.host_mismatch_policy,
            significance_alpha: None,
            significance_min_samples: 8,
            require_significance: false,
        },
    );

    let ended_at = clock.now_rfc3339();
    let duration_ms = start_instant.elapsed().as_millis() as u64;

    let baseline_reason = if !baseline_available {
        Some(BASELINE_REASON_NO_BASELINE.to_string())
    } else {
        None
    };

    match result {
        Ok(outcome) => {
            let mut builder = SensorReportBuilder::new(tool, started_at)
                .ended_at(ended_at, duration_ms)
                .baseline(baseline_available, baseline_reason);

            if let Some(limit) = options.max_findings {
                builder = builder.max_findings(limit);
            }

            builder.build(&outcome.report)
        }
        Err(err) => {
            let (stage, error_kind) = classify_error(&err);
            let builder = SensorReportBuilder::new(tool, started_at)
                .ended_at(ended_at, duration_ms)
                .baseline(baseline_available, baseline_reason);

            builder.build_error(&err.to_string(), stage, error_kind)
        }
    }
}

/// Classify an error into (stage, error_kind) for structured error reporting.
pub fn classify_error(err: &anyhow::Error) -> (&'static str, &'static str) {
    if err.downcast_ref::<ConfigValidationError>().is_some() {
        return (STAGE_CONFIG_PARSE, ERROR_KIND_PARSE);
    }

    if let Some(pe) = err.downcast_ref::<PerfgateError>() {
        return match pe {
            PerfgateError::Validation(_) => (STAGE_CONFIG_PARSE, ERROR_KIND_PARSE),
            PerfgateError::Config(_) => (STAGE_CONFIG_PARSE, ERROR_KIND_PARSE),
            PerfgateError::Adapter(ae) => match ae {
                AdapterError::Timeout => (STAGE_RUN_COMMAND, ERROR_KIND_EXEC),
                AdapterError::EmptyArgv => (STAGE_RUN_COMMAND, ERROR_KIND_EXEC),
                AdapterError::TimeoutUnsupported => (STAGE_RUN_COMMAND, ERROR_KIND_EXEC),
                AdapterError::RunCommand { .. } => (STAGE_RUN_COMMAND, ERROR_KIND_EXEC),
                AdapterError::Other(_) => (STAGE_RUN_COMMAND, ERROR_KIND_IO),
            },
            PerfgateError::Io(ie) => match ie {
                IoError::BaselineNotFound { .. } => (STAGE_BASELINE_RESOLVE, ERROR_KIND_IO),
                IoError::BaselineResolve(_) => (STAGE_BASELINE_RESOLVE, ERROR_KIND_IO),
                IoError::ArtifactWrite(_) => (STAGE_WRITE_ARTIFACTS, ERROR_KIND_IO),
                IoError::RunCommand { .. } => (STAGE_RUN_COMMAND, ERROR_KIND_EXEC),
                IoError::Other(_) => (STAGE_RUN_COMMAND, ERROR_KIND_IO),
            },
            PerfgateError::Stats(_) => (STAGE_RUN_COMMAND, ERROR_KIND_PARSE),
            PerfgateError::Paired(_) => (STAGE_RUN_COMMAND, ERROR_KIND_PARSE),
            PerfgateError::Auth(_) => (STAGE_BASELINE_RESOLVE, ERROR_KIND_IO),
        };
    }

    if err.downcast_ref::<perfgate_domain::DomainError>().is_some() {
        return (STAGE_RUN_COMMAND, ERROR_KIND_EXEC);
    }

    let msg_lower = err.to_string().to_lowercase();

    if msg_lower.contains("config") || msg_lower.contains("toml") || msg_lower.contains("json") {
        (STAGE_CONFIG_PARSE, ERROR_KIND_PARSE)
    } else if msg_lower.contains("baseline") {
        (STAGE_BASELINE_RESOLVE, ERROR_KIND_IO)
    } else if msg_lower.contains("failed to run")
        || msg_lower.contains("spawn")
        || msg_lower.contains("exec")
    {
        (STAGE_RUN_COMMAND, ERROR_KIND_EXEC)
    } else if msg_lower.contains("write") || msg_lower.contains("permission") {
        (STAGE_WRITE_ARTIFACTS, ERROR_KIND_IO)
    } else {
        (STAGE_RUN_COMMAND, ERROR_KIND_IO)
    }
}
