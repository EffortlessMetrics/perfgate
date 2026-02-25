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
use perfgate_adapters::AdapterError;
use perfgate_adapters::{HostProbe, ProcessRunner};
use perfgate_types::{
    BASELINE_REASON_NO_BASELINE, ConfigFile, ConfigValidationError, ERROR_KIND_EXEC, ERROR_KIND_IO,
    ERROR_KIND_PARSE, HostMismatchPolicy, MAX_FINDINGS_DEFAULT, PerfgateError, RunReceipt,
    STAGE_BASELINE_RESOLVE, STAGE_CONFIG_PARSE, STAGE_RUN_COMMAND, STAGE_WRITE_ARTIFACTS,
    SensorReport, ToolInfo, validate_bench_name,
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
///
/// This is the library convenience API for cockpit linking. It delegates to
/// `CheckUseCase::execute()`, wraps the result in a `SensorReportBuilder`,
/// and catches errors to produce an error report.
///
/// Returns `SensorReport` directly — no I/O, no file writing.
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

    // Validate bench name early — invalid names produce an error report.
    if let Err(err) = validate_bench_name(bench_name) {
        let ended_at = clock.now_rfc3339();
        let duration_ms = start_instant.elapsed().as_millis() as u64;
        let builder = SensorReportBuilder::new(tool, started_at)
            .ended_at(ended_at, duration_ms)
            .baseline(baseline.is_some(), None);
        return builder.build_error(&err.to_string(), STAGE_CONFIG_PARSE, ERROR_KIND_PARSE);
    }

    // Validate config (covers other bench names in the file) — defense-in-depth
    // so library callers can't bypass config-level validation.
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
            out_dir: std::path::PathBuf::from("."), // artifacts not written
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

            builder.build_error(&format!("{:#}", err), stage, error_kind)
        }
    }
}

/// Classify an error into (stage, error_kind) for structured error reporting.
pub fn classify_error(err: &anyhow::Error) -> (&'static str, &'static str) {
    // Structural classification — preferred over string matching.
    if err.downcast_ref::<ConfigValidationError>().is_some() {
        return (STAGE_CONFIG_PARSE, ERROR_KIND_PARSE);
    }

    if let Some(pe) = err.downcast_ref::<PerfgateError>() {
        return match pe {
            PerfgateError::BaselineResolve(_) => (STAGE_BASELINE_RESOLVE, ERROR_KIND_IO),
            PerfgateError::ArtifactWrite(_) => (STAGE_WRITE_ARTIFACTS, ERROR_KIND_IO),
            PerfgateError::RunCommand(_) => (STAGE_RUN_COMMAND, ERROR_KIND_EXEC),
        };
    }

    // Walk the error chain for AdapterError.
    if let Some(ae) = err.downcast_ref::<AdapterError>() {
        return match ae {
            AdapterError::EmptyArgv | AdapterError::Timeout | AdapterError::TimeoutUnsupported => {
                (STAGE_RUN_COMMAND, ERROR_KIND_EXEC)
            }
            AdapterError::Other(_) => (STAGE_RUN_COMMAND, ERROR_KIND_IO),
        };
    }

    // Walk the chain for DomainError.
    if err.downcast_ref::<perfgate_domain::DomainError>().is_some() {
        return (STAGE_RUN_COMMAND, ERROR_KIND_EXEC);
    }

    // Fallback: string heuristics for errors not yet converted to typed errors.
    let msg = format!("{:#}", err);
    let msg_lower = msg.to_lowercase();

    if msg_lower.contains("bench name")
        || msg_lower.contains("config validation")
        || (msg_lower.contains("parse")
            && (msg_lower.contains("toml")
                || msg_lower.contains("json config")
                || msg_lower.contains("config")))
    {
        (STAGE_CONFIG_PARSE, ERROR_KIND_PARSE)
    } else if msg_lower.contains("baseline") || msg_lower.contains("not found") {
        (STAGE_BASELINE_RESOLVE, ERROR_KIND_IO)
    } else if msg_lower.contains("failed to run")
        || msg_lower.contains("spawn")
        || msg_lower.contains("exec")
    {
        (STAGE_RUN_COMMAND, ERROR_KIND_EXEC)
    } else if msg_lower.contains("write")
        || msg_lower.contains("create dir")
        || msg_lower.contains("rename")
    {
        (STAGE_WRITE_ARTIFACTS, ERROR_KIND_IO)
    } else if err.downcast_ref::<std::io::Error>().is_some() {
        (STAGE_RUN_COMMAND, ERROR_KIND_IO)
    } else {
        (STAGE_RUN_COMMAND, ERROR_KIND_EXEC)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_adapters::FakeProcessRunner;
    use perfgate_types::{
        ERROR_KIND_EXEC, ERROR_KIND_PARSE, PerfgateError, REPORT_SCHEMA_V1, ReportSummary,
        SensorVerdictStatus, Verdict, VerdictCounts, VerdictStatus,
    };

    fn make_tool_info() -> ToolInfo {
        ToolInfo {
            name: "perfgate".to_string(),
            version: "0.1.0".to_string(),
        }
    }

    fn make_pass_report() -> perfgate_types::PerfgateReport {
        perfgate_types::PerfgateReport {
            report_type: REPORT_SCHEMA_V1.to_string(),
            verdict: Verdict {
                status: VerdictStatus::Pass,
                counts: VerdictCounts {
                    pass: 2,
                    warn: 0,
                    fail: 0,
                },
                reasons: vec![],
            },
            compare: None,
            findings: vec![],
            summary: ReportSummary {
                pass_count: 2,
                warn_count: 0,
                fail_count: 0,
                total_count: 2,
            },
        }
    }

    #[test]
    fn test_classify_error_config_parse() {
        let err = anyhow::anyhow!("parse TOML config perfgate.toml: expected `=`");
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_CONFIG_PARSE);
        assert_eq!(kind, ERROR_KIND_PARSE);
    }

    #[test]
    fn test_classify_error_baseline_resolve() {
        let err = anyhow::anyhow!("baseline file not found");
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_BASELINE_RESOLVE);
        assert_eq!(kind, ERROR_KIND_IO);
    }

    #[test]
    fn test_classify_error_default_exec() {
        let err = anyhow::anyhow!("something unexpected happened");
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_RUN_COMMAND);
        assert_eq!(kind, ERROR_KIND_EXEC);
    }

    #[test]
    fn test_classify_error_spawn_failure() {
        let err = anyhow::anyhow!("failed to run command: spawn error");
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_RUN_COMMAND);
        assert_eq!(kind, ERROR_KIND_EXEC);
    }

    #[test]
    fn test_classify_error_exec_failure() {
        let err = anyhow::anyhow!("exec failed for process");
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_RUN_COMMAND);
        assert_eq!(kind, ERROR_KIND_EXEC);
    }

    #[test]
    fn test_classify_error_write_artifacts() {
        let err = anyhow::anyhow!("write output file failed");
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_WRITE_ARTIFACTS);
        assert_eq!(kind, ERROR_KIND_IO);
    }

    #[test]
    fn test_classify_error_create_dir() {
        let err = anyhow::anyhow!("create dir /tmp/out: permission denied");
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_WRITE_ARTIFACTS);
        assert_eq!(kind, ERROR_KIND_IO);
    }

    #[test]
    fn test_classify_error_rename() {
        let err = anyhow::anyhow!("rename temp file: cross-device link");
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_WRITE_ARTIFACTS);
        assert_eq!(kind, ERROR_KIND_IO);
    }

    #[test]
    fn test_classify_error_io_downcast() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file gone");
        let err: anyhow::Error = io_err.into();
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_RUN_COMMAND);
        assert_eq!(kind, ERROR_KIND_IO);
    }

    #[test]
    fn test_classify_error_json_config() {
        let err = anyhow::anyhow!("parse JSON config perfgate.json: unexpected token");
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_CONFIG_PARSE);
        assert_eq!(kind, ERROR_KIND_PARSE);
    }

    #[test]
    fn test_classify_error_generic_config_parse() {
        let err = anyhow::anyhow!("parse config perfgate");
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_CONFIG_PARSE);
        assert_eq!(kind, ERROR_KIND_PARSE);
    }

    #[test]
    fn test_classify_error_bench_name_validation() {
        let err = anyhow::anyhow!(
            "bench name \"../evil\" contains a \"..\" path segment (path traversal is forbidden)"
        );
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_CONFIG_PARSE);
        assert_eq!(kind, ERROR_KIND_PARSE);
    }

    #[test]
    fn test_classify_error_config_validation() {
        let err =
            anyhow::anyhow!("config validation: bench name \"Bad\" contains invalid characters");
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_CONFIG_PARSE);
        assert_eq!(kind, ERROR_KIND_PARSE);
    }

    #[test]
    fn test_classify_error_config_validation_without_bench_name() {
        let err = anyhow::anyhow!("config validation: some future validation error");
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_CONFIG_PARSE);
        assert_eq!(kind, ERROR_KIND_PARSE);
    }

    #[test]
    fn test_classify_error_config_validation_typed_config_file() {
        let err: anyhow::Error = ConfigValidationError::ConfigFile(
            "bench name \"Bad\" contains invalid characters".to_string(),
        )
        .into();
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_CONFIG_PARSE);
        assert_eq!(kind, ERROR_KIND_PARSE);
    }

    #[test]
    fn test_classify_error_config_validation_typed_bench_name() {
        let err: anyhow::Error =
            ConfigValidationError::BenchName("bench name must not be empty".to_string()).into();
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_CONFIG_PARSE);
        assert_eq!(kind, ERROR_KIND_PARSE);
    }

    // --- Typed PerfgateError downcast tests ---

    #[test]
    fn test_classify_error_typed_baseline_resolve() {
        let err: anyhow::Error =
            PerfgateError::BaselineResolve("file not found".to_string()).into();
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_BASELINE_RESOLVE);
        assert_eq!(kind, ERROR_KIND_IO);
    }

    #[test]
    fn test_classify_error_typed_artifact_write() {
        let err: anyhow::Error =
            PerfgateError::ArtifactWrite("permission denied".to_string()).into();
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_WRITE_ARTIFACTS);
        assert_eq!(kind, ERROR_KIND_IO);
    }

    #[test]
    fn test_classify_error_typed_run_command() {
        let err: anyhow::Error = PerfgateError::RunCommand("spawn failed".to_string()).into();
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_RUN_COMMAND);
        assert_eq!(kind, ERROR_KIND_EXEC);
    }

    // --- AdapterError downcast tests ---

    #[test]
    fn test_classify_error_adapter_empty_argv() {
        let err: anyhow::Error = AdapterError::EmptyArgv.into();
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_RUN_COMMAND);
        assert_eq!(kind, ERROR_KIND_EXEC);
    }

    #[test]
    fn test_classify_error_adapter_timeout() {
        let err: anyhow::Error = AdapterError::Timeout.into();
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_RUN_COMMAND);
        assert_eq!(kind, ERROR_KIND_EXEC);
    }

    #[test]
    fn test_classify_error_adapter_timeout_unsupported() {
        let err: anyhow::Error = AdapterError::TimeoutUnsupported.into();
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_RUN_COMMAND);
        assert_eq!(kind, ERROR_KIND_EXEC);
    }

    #[test]
    fn test_classify_error_adapter_other() {
        let inner = anyhow::anyhow!("some IO problem");
        let err: anyhow::Error = AdapterError::Other(inner).into();
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_RUN_COMMAND);
        assert_eq!(kind, ERROR_KIND_IO);
    }

    // --- DomainError downcast test ---

    #[test]
    fn test_classify_error_domain_no_samples() {
        let err: anyhow::Error = perfgate_domain::DomainError::NoSamples.into();
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_RUN_COMMAND);
        assert_eq!(kind, ERROR_KIND_EXEC);
    }

    // --- Bench-not-found misclassification bug regression test ---

    #[test]
    fn test_classify_error_bench_not_found_typed() {
        let err: anyhow::Error =
            ConfigValidationError::BenchName("bench 'xyz' not found in config".to_string()).into();
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_CONFIG_PARSE);
        assert_eq!(kind, ERROR_KIND_PARSE);
    }

    // --- run_sensor_check integration test ---

    #[derive(Clone)]
    struct TestHostProbe {
        host: perfgate_types::HostInfo,
    }

    impl TestHostProbe {
        fn new(host: perfgate_types::HostInfo) -> Self {
            Self { host }
        }
    }

    impl HostProbe for TestHostProbe {
        fn probe(
            &self,
            _options: &perfgate_adapters::HostProbeOptions,
        ) -> perfgate_types::HostInfo {
            self.host.clone()
        }
    }

    #[derive(Clone)]
    struct TestClock {
        now: String,
    }

    impl TestClock {
        fn new(now: &str) -> Self {
            Self {
                now: now.to_string(),
            }
        }
    }

    impl Clock for TestClock {
        fn now_rfc3339(&self) -> String {
            self.now.clone()
        }
    }

    #[test]
    fn test_run_sensor_check_deterministic() {
        use perfgate_adapters::RunResult;

        let runner = FakeProcessRunner::new();
        runner.set_fallback(RunResult {
            wall_ms: 100,
            exit_code: 0,
            timed_out: false,
            cpu_ms: Some(50),
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: Some(2048),
            binary_bytes: None,
            stdout: b"ok".to_vec(),
            stderr: b"".to_vec(),
        });

        let host_probe = TestHostProbe::new(perfgate_types::HostInfo {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            cpu_count: Some(4),
            memory_bytes: Some(8 * 1024 * 1024 * 1024),
            hostname_hash: None,
        });
        let clock = TestClock::new("2024-01-01T00:00:00Z");

        let config = ConfigFile {
            defaults: perfgate_types::DefaultsConfig::default(),
            benches: vec![perfgate_types::BenchConfigFile {
                name: "test-bench".to_string(),
                cwd: None,
                work: None,
                timeout: None,
                command: vec!["true".to_string()],
                repeat: None,
                warmup: None,
                metrics: None,
                budgets: None,
            }],
        };

        let baseline = perfgate_types::RunReceipt {
            schema: perfgate_types::RUN_SCHEMA_V1.to_string(),
            tool: make_tool_info(),
            run: perfgate_types::RunMeta {
                id: "baseline".to_string(),
                started_at: "2024-01-01T00:00:00Z".to_string(),
                ended_at: "2024-01-01T00:00:01Z".to_string(),
                host: host_probe.probe(&perfgate_adapters::HostProbeOptions::default()),
            },
            bench: perfgate_types::BenchMeta {
                name: "test-bench".to_string(),
                cwd: None,
                command: vec!["true".to_string()],
                repeat: 1,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            samples: vec![],
            stats: perfgate_types::Stats {
                wall_ms: perfgate_types::U64Summary {
                    median: 50,
                    min: 50,
                    max: 50,
                },
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                binary_bytes: None,
                throughput_per_s: None,
            },
        };

        let report = run_sensor_check(
            &runner,
            &host_probe,
            &clock,
            &config,
            "test-bench",
            Some(&baseline),
            make_tool_info(),
            SensorCheckOptions::default(),
        );

        assert_eq!(report.verdict.status, SensorVerdictStatus::Fail);
        assert_eq!(report.verdict.counts.error, 1);
        assert!(report.findings[0].message.contains("wall_ms regression"));
    }

    // Tests for re-exported types from perfgate-sensor

    #[test]
    fn test_reexported_sensor_report_builder() {
        let report = make_pass_report();
        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
                .ended_at("2024-01-01T00:01:00Z".to_string(), 60000)
                .baseline(true, None);

        let sensor_report = builder.build(&report);

        assert_eq!(sensor_report.verdict.status, SensorVerdictStatus::Pass);
    }

    #[test]
    fn test_reexported_sensor_fingerprint() {
        let fp = sensor_fingerprint(&["perfgate", "perf.budget", "metric_fail"]);
        assert_eq!(fp.len(), 64);
    }

    #[test]
    fn test_reexported_default_engine_capability() {
        let cap = default_engine_capability();
        if cfg!(unix) {
            assert_eq!(cap.status, perfgate_types::CapabilityStatus::Available);
        } else {
            assert_eq!(cap.status, perfgate_types::CapabilityStatus::Unavailable);
        }
    }

    #[test]
    fn test_reexported_bench_outcome() {
        let outcome = BenchOutcome::Success {
            bench_name: "test".to_string(),
            report: make_pass_report(),
            has_compare: false,
            baseline_available: false,
            markdown: "## test".to_string(),
            extras_prefix: "extras".to_string(),
        };
        assert_eq!(outcome.bench_name(), "test");
    }
}
