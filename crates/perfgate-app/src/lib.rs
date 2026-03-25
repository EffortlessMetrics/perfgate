//! Application layer for perfgate.
//!
//! The app layer coordinates adapters and domain logic.
//! It does not parse CLI flags and it does not do filesystem I/O.

mod aggregate;
pub mod baseline_resolve;
pub mod bisect;
pub mod blame;
mod check;
pub mod comparison_logic;
mod explain;
mod paired;
mod promote;
mod report;
mod sensor_report;

pub use aggregate::{AggregateOutcome, AggregateRequest, AggregateUseCase};
pub use bisect::{BisectRequest, BisectUseCase};
pub use blame::{BlameOutcome, BlameRequest, BlameUseCase};
pub use check::{CheckOutcome, CheckRequest, CheckUseCase};
pub use explain::{ExplainOutcome, ExplainRequest, ExplainUseCase};
pub use paired::{PairedRunOutcome, PairedRunRequest, PairedRunUseCase};
pub use promote::{PromoteRequest, PromoteResult, PromoteUseCase};
pub use report::{ReportRequest, ReportResult, ReportUseCase};
pub use sensor_report::{
    BenchOutcome, SensorCheckOptions, SensorReportBuilder, classify_error,
    default_engine_capability, run_sensor_check, sensor_fingerprint,
};

// Re-export rendering functions from perfgate-render for backward compatibility
pub use perfgate_render::{
    direction_str, format_metric, format_metric_with_statistic, format_pct, format_value,
    github_annotations, markdown_template_context, metric_status_icon, metric_status_str,
    parse_reason_token, render_markdown, render_markdown_template, render_reason_line,
};

// Re-export export functionality from perfgate-export for backward compatibility
pub use perfgate_export::{CompareExportRow, ExportFormat, ExportUseCase, RunExportRow};

use perfgate_adapters::{CommandSpec, HostProbe, HostProbeOptions, ProcessRunner, RunResult};
use perfgate_domain::{
    Comparison, SignificancePolicy, compare_runs, compute_stats, detect_host_mismatch,
};
use perfgate_types::{
    BenchMeta, Budget, CompareReceipt, CompareRef, HostMismatchInfo, HostMismatchPolicy, Metric,
    MetricStatistic, RunMeta, RunReceipt, Sample, ToolInfo,
};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

pub trait Clock: Send + Sync {
    fn now_rfc3339(&self) -> String;
}

#[derive(Debug, Default, Clone)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_rfc3339(&self) -> String {
        use time::format_description::well_known::Rfc3339;
        time::OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
    }
}

#[derive(Debug, Clone, Default)]
pub struct RunBenchRequest {
    pub name: String,
    pub cwd: Option<PathBuf>,
    pub command: Vec<String>,
    pub repeat: u32,
    pub warmup: u32,
    pub work_units: Option<u64>,
    pub timeout: Option<Duration>,
    pub env: Vec<(String, String)>,
    pub output_cap_bytes: usize,

    /// If true, do not treat nonzero exit codes as a tool error.
    /// The receipt will still record exit codes.
    pub allow_nonzero: bool,

    /// If true, include a hashed hostname in the host fingerprint.
    /// This is opt-in for privacy reasons.
    pub include_hostname_hash: bool,
}

#[derive(Debug, Clone)]
pub struct RunBenchOutcome {
    pub receipt: RunReceipt,

    /// True if any measured (non-warmup) sample timed out or returned nonzero.
    pub failed: bool,

    /// Human-readable reasons (for CI logs).
    pub reasons: Vec<String>,
}

pub struct RunBenchUseCase<R: ProcessRunner, H: HostProbe, C: Clock> {
    runner: R,
    host_probe: H,
    clock: C,
    tool: ToolInfo,
}

impl<R: ProcessRunner, H: HostProbe, C: Clock> RunBenchUseCase<R, H, C> {
    pub fn new(runner: R, host_probe: H, clock: C, tool: ToolInfo) -> Self {
        Self {
            runner,
            host_probe,
            clock,
            tool,
        }
    }

    pub fn execute(&self, req: RunBenchRequest) -> anyhow::Result<RunBenchOutcome> {
        let run_id = uuid::Uuid::new_v4().to_string();
        let started_at = self.clock.now_rfc3339();

        let host_options = HostProbeOptions {
            include_hostname_hash: req.include_hostname_hash,
        };
        let host = self.host_probe.probe(&host_options);

        let bench = BenchMeta {
            name: req.name.clone(),
            cwd: req.cwd.as_ref().map(|p| p.to_string_lossy().to_string()),
            command: req.command.clone(),
            repeat: req.repeat,
            warmup: req.warmup,
            work_units: req.work_units,
            timeout_ms: req.timeout.map(|d| d.as_millis() as u64),
        };

        let mut samples: Vec<Sample> = Vec::new();
        let mut reasons: Vec<String> = Vec::new();

        let total = req.warmup + req.repeat;

        for i in 0..total {
            let is_warmup = i < req.warmup;

            let spec = CommandSpec {
                name: req.name.clone(),
                argv: req.command.clone(),
                cwd: req.cwd.clone(),
                env: req.env.clone(),
                timeout: req.timeout,
                output_cap_bytes: req.output_cap_bytes,
            };

            let run = self.runner.run(&spec).map_err(|e| match e {
                perfgate_adapters::AdapterError::RunCommand { command, reason } => {
                    anyhow::anyhow!("failed to run iteration {}: {}: {}", i + 1, command, reason)
                }
                _ => anyhow::anyhow!("failed to run iteration {}: {}", i + 1, e),
            })?;

            let s = sample_from_run(run, is_warmup);
            if !is_warmup {
                if s.timed_out {
                    reasons.push(format!("iteration {} timed out", i + 1));
                }
                if s.exit_code != 0 {
                    reasons.push(format!("iteration {} exit code {}", i + 1, s.exit_code));
                }
            }

            samples.push(s);
        }

        let stats = compute_stats(&samples, req.work_units)?;

        let ended_at = self.clock.now_rfc3339();

        let receipt = RunReceipt {
            schema: perfgate_types::RUN_SCHEMA_V1.to_string(),
            tool: self.tool.clone(),
            run: RunMeta {
                id: run_id,
                started_at,
                ended_at,
                host,
            },
            bench,
            samples,
            stats,
        };

        let failed = !reasons.is_empty();

        if failed && !req.allow_nonzero {
            // It's still a successful run from a *tooling* perspective, but callers may want a hard failure.
            // We return the receipt either way; the CLI decides exit codes.
        }

        Ok(RunBenchOutcome {
            receipt,
            failed,
            reasons,
        })
    }
}

fn sample_from_run(run: RunResult, warmup: bool) -> Sample {
    Sample {
        wall_ms: run.wall_ms,
        exit_code: run.exit_code,
        warmup,
        timed_out: run.timed_out,
        cpu_ms: run.cpu_ms,
        page_faults: run.page_faults,
        ctx_switches: run.ctx_switches,
        max_rss_kb: run.max_rss_kb,
        io_read_bytes: run.io_read_bytes,
        io_write_bytes: run.io_write_bytes,
        network_packets: run.network_packets,
        energy_uj: run.energy_uj,
        binary_bytes: run.binary_bytes,
        stdout: if run.stdout.is_empty() {
            None
        } else {
            Some(String::from_utf8_lossy(&run.stdout).to_string())
        },
        stderr: if run.stderr.is_empty() {
            None
        } else {
            Some(String::from_utf8_lossy(&run.stderr).to_string())
        },
    }
}

#[derive(Debug, Clone)]
pub struct CompareRequest {
    pub baseline: RunReceipt,
    pub current: RunReceipt,
    pub budgets: BTreeMap<Metric, Budget>,
    pub metric_statistics: BTreeMap<Metric, MetricStatistic>,
    pub significance: Option<SignificancePolicy>,
    pub baseline_ref: CompareRef,
    pub current_ref: CompareRef,
    pub tool: ToolInfo,
    /// Policy for handling host mismatches.
    #[allow(dead_code)]
    pub host_mismatch_policy: HostMismatchPolicy,
}

/// Result from CompareUseCase including host mismatch information.
#[derive(Debug, Clone)]
pub struct CompareResult {
    pub receipt: CompareReceipt,
    /// Host mismatch info if detected (only populated when policy is not Ignore).
    pub host_mismatch: Option<HostMismatchInfo>,
}

pub struct CompareUseCase;

impl CompareUseCase {
    pub fn execute(req: CompareRequest) -> anyhow::Result<CompareResult> {
        // Check for host mismatch
        let host_mismatch = if req.host_mismatch_policy != HostMismatchPolicy::Ignore {
            detect_host_mismatch(&req.baseline.run.host, &req.current.run.host)
        } else {
            None
        };

        // If policy is Error and there's a mismatch, fail immediately
        if req.host_mismatch_policy == HostMismatchPolicy::Error
            && let Some(mismatch) = &host_mismatch
        {
            anyhow::bail!(
                "host mismatch detected (--host-mismatch=error): {}",
                mismatch.reasons.join("; ")
            );
        }

        let Comparison { deltas, verdict } = compare_runs(
            &req.baseline,
            &req.current,
            &req.budgets,
            &req.metric_statistics,
            req.significance,
        )?;

        let receipt = CompareReceipt {
            schema: perfgate_types::COMPARE_SCHEMA_V1.to_string(),
            tool: req.tool,
            bench: req.current.bench,
            baseline_ref: req.baseline_ref,
            current_ref: req.current_ref,
            budgets: req.budgets,
            deltas,
            verdict,
        };

        Ok(CompareResult {
            receipt,
            host_mismatch,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{
        Delta, Direction, HostInfo, MetricStatistic, MetricStatus, RUN_SCHEMA_V1, RunMeta,
        RunReceipt, Stats, U64Summary, Verdict, VerdictCounts, VerdictStatus,
    };
    use std::collections::BTreeMap;

    fn make_compare_receipt(status: MetricStatus) -> CompareReceipt {
        let mut budgets = BTreeMap::new();
        budgets.insert(
            Metric::WallMs,
            Budget {
                threshold: 0.2,
                warn_threshold: 0.1,
                noise_threshold: None,
                noise_policy: perfgate_types::NoisePolicy::Ignore,
                direction: Direction::Lower,
            },
        );

        let mut deltas = BTreeMap::new();
        deltas.insert(
            Metric::WallMs,
            Delta {
                baseline: 100.0,
                current: 115.0,
                ratio: 1.15,
                pct: 0.15,
                regression: 0.15,
                cv: None,
                noise_threshold: None,
                statistic: MetricStatistic::Median,
                significance: None,
                status,
            },
        );

        CompareReceipt {
            schema: perfgate_types::COMPARE_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".into(),
                version: "0.1.0".into(),
            },
            bench: BenchMeta {
                name: "bench".into(),
                cwd: None,
                command: vec!["true".into()],
                repeat: 1,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            baseline_ref: CompareRef {
                path: None,
                run_id: None,
            },
            current_ref: CompareRef {
                path: None,
                run_id: None,
            },
            budgets,
            deltas,
            verdict: Verdict {
                status: VerdictStatus::Warn,
                counts: VerdictCounts {
                    pass: if status == MetricStatus::Pass { 1 } else { 0 },
                    warn: if status == MetricStatus::Warn { 1 } else { 0 },
                    fail: if status == MetricStatus::Fail { 1 } else { 0 },
                    skip: if status == MetricStatus::Skip { 1 } else { 0 },
                },
                reasons: vec!["wall_ms_warn".to_string()],
            },
        }
    }

    fn make_run_receipt_with_host(host: HostInfo, wall_ms: u64) -> RunReceipt {
        RunReceipt {
            schema: RUN_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
            run: RunMeta {
                id: "run-id".to_string(),
                started_at: "2024-01-01T00:00:00Z".to_string(),
                ended_at: "2024-01-01T00:00:01Z".to_string(),
                host,
            },
            bench: BenchMeta {
                name: "bench".to_string(),
                cwd: None,
                command: vec!["true".to_string()],
                repeat: 1,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            samples: Vec::new(),
            stats: Stats {
                wall_ms: U64Summary::new(wall_ms, wall_ms, wall_ms),
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                throughput_per_s: None,
            },
        }
    }

    #[test]
    fn markdown_renders_table() {
        let mut budgets = BTreeMap::new();
        budgets.insert(
            Metric::WallMs,
            Budget {
                threshold: 0.2,
                warn_threshold: 0.18,
                noise_threshold: None,
                noise_policy: perfgate_types::NoisePolicy::Ignore,
                direction: Direction::Lower,
            },
        );

        let mut deltas = BTreeMap::new();
        deltas.insert(
            Metric::WallMs,
            Delta {
                baseline: 1000.0,
                current: 1100.0,
                ratio: 1.1,
                pct: 0.1,
                regression: 0.1,
                cv: None,
                noise_threshold: None,
                statistic: MetricStatistic::Median,
                significance: None,
                status: MetricStatus::Pass,
            },
        );

        let compare = CompareReceipt {
            schema: perfgate_types::COMPARE_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".into(),
                version: "0.1.0".into(),
            },
            bench: BenchMeta {
                name: "demo".into(),
                cwd: None,
                command: vec!["true".into()],
                repeat: 1,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            baseline_ref: CompareRef {
                path: None,
                run_id: None,
            },
            current_ref: CompareRef {
                path: None,
                run_id: None,
            },
            budgets,
            deltas,
            verdict: Verdict {
                status: VerdictStatus::Pass,
                counts: VerdictCounts {
                    pass: 1,
                    warn: 0,
                    fail: 0,
                    skip: 0,
                },
                reasons: vec![],
            },
        };

        let md = render_markdown(&compare);
        assert!(md.contains("| metric | baseline"));
        assert!(md.contains("wall_ms"));
    }

    #[test]
    fn markdown_template_renders_context_rows() {
        let compare = make_compare_receipt(MetricStatus::Warn);
        let template = "{{header}}\nbench={{bench.name}}\n{{#each rows}}metric={{metric}} status={{status}}\n{{/each}}";

        let rendered = render_markdown_template(&compare, template).expect("render template");
        assert!(rendered.contains("bench=bench"));
        assert!(rendered.contains("metric=wall_ms"));
        assert!(rendered.contains("status=warn"));
    }

    #[test]
    fn markdown_template_strict_mode_rejects_unknown_fields() {
        let compare = make_compare_receipt(MetricStatus::Warn);
        let err = render_markdown_template(&compare, "{{does_not_exist}}").unwrap_err();
        assert!(
            err.to_string().contains("render markdown template"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn parse_reason_token_handles_valid_and_invalid() {
        let parsed = parse_reason_token("wall_ms_warn");
        assert!(parsed.is_some());
        let (metric, status) = parsed.unwrap();
        assert_eq!(metric, Metric::WallMs);
        assert_eq!(status, MetricStatus::Warn);

        assert!(parse_reason_token("wall_ms_pass").is_none());
        assert!(parse_reason_token("unknown_warn").is_none());
    }

    #[test]
    fn render_reason_line_formats_thresholds() {
        let compare = make_compare_receipt(MetricStatus::Warn);
        let line = render_reason_line(&compare, "wall_ms_warn");
        assert!(line.contains("warn >="));
        assert!(line.contains("fail >"));
        assert!(line.contains("+15.00%"));
    }

    #[test]
    fn render_reason_line_falls_back_when_missing_budget() {
        let mut compare = make_compare_receipt(MetricStatus::Warn);
        compare.budgets.clear();
        let line = render_reason_line(&compare, "wall_ms_warn");
        assert_eq!(line, "- wall_ms_warn\n");
    }

    #[test]
    fn format_value_and_pct_render_expected_strings() {
        assert_eq!(format_value(Metric::ThroughputPerS, 1.23456), "1.235");
        assert_eq!(format_value(Metric::WallMs, 123.0), "123");
        assert_eq!(format_pct(0.1), "+10.00%");
        assert_eq!(format_pct(-0.1), "-10.00%");
        assert_eq!(format_pct(0.0), "0.00%");
    }

    #[test]
    fn github_annotations_only_warn_and_fail() {
        let mut compare = make_compare_receipt(MetricStatus::Warn);
        compare.deltas.insert(
            Metric::MaxRssKb,
            Delta {
                baseline: 100.0,
                current: 150.0,
                ratio: 1.5,
                pct: 0.5,
                regression: 0.5,
                cv: None,
                noise_threshold: None,
                statistic: MetricStatistic::Median,
                significance: None,
                status: MetricStatus::Fail,
            },
        );
        compare.deltas.insert(
            Metric::ThroughputPerS,
            Delta {
                baseline: 100.0,
                current: 90.0,
                ratio: 0.9,
                pct: -0.1,
                regression: 0.0,
                cv: None,
                noise_threshold: None,
                statistic: MetricStatistic::Median,
                significance: None,
                status: MetricStatus::Pass,
            },
        );

        let lines = github_annotations(&compare);
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().any(|l| l.starts_with("::warning::")));
        assert!(lines.iter().any(|l| l.starts_with("::error::")));
        assert!(lines.iter().all(|l| !l.contains("throughput_per_s")));
    }

    #[test]
    fn sample_from_run_sets_optional_stdout_stderr() {
        let run = RunResult {
            wall_ms: 100,
            exit_code: 0,
            timed_out: false,
            cpu_ms: None,
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: None,
            io_read_bytes: None,
            io_write_bytes: None,
            network_packets: None,
            energy_uj: None,
            binary_bytes: None,
            stdout: b"ok".to_vec(),
            stderr: vec![],
        };

        let sample = sample_from_run(run, false);
        assert_eq!(sample.stdout.as_deref(), Some("ok"));
        assert!(sample.stderr.is_none());
    }

    #[test]
    fn compare_use_case_host_mismatch_policies() {
        let baseline = make_run_receipt_with_host(
            HostInfo {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                cpu_count: None,
                memory_bytes: None,
                hostname_hash: None,
            },
            100,
        );
        let current = make_run_receipt_with_host(
            HostInfo {
                os: "windows".to_string(),
                arch: "x86_64".to_string(),
                cpu_count: None,
                memory_bytes: None,
                hostname_hash: None,
            },
            100,
        );

        let mut budgets = BTreeMap::new();
        budgets.insert(
            Metric::WallMs,
            Budget {
                threshold: 0.2,
                warn_threshold: 0.1,
                noise_threshold: None,
                noise_policy: perfgate_types::NoisePolicy::Ignore,
                direction: Direction::Lower,
            },
        );

        let err = CompareUseCase::execute(CompareRequest {
            baseline: baseline.clone(),
            current: current.clone(),
            budgets: budgets.clone(),
            metric_statistics: BTreeMap::new(),
            significance: None,
            baseline_ref: CompareRef {
                path: None,
                run_id: None,
            },
            current_ref: CompareRef {
                path: None,
                run_id: None,
            },
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
            host_mismatch_policy: HostMismatchPolicy::Error,
        })
        .unwrap_err();
        assert!(err.to_string().contains("host mismatch"));

        let matching = CompareUseCase::execute(CompareRequest {
            baseline: baseline.clone(),
            current: baseline.clone(),
            budgets: budgets.clone(),
            metric_statistics: BTreeMap::new(),
            significance: None,
            baseline_ref: CompareRef {
                path: None,
                run_id: None,
            },
            current_ref: CompareRef {
                path: None,
                run_id: None,
            },
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
            host_mismatch_policy: HostMismatchPolicy::Error,
        })
        .expect("matching hosts should not error");
        assert!(matching.host_mismatch.is_none());

        let ignore = CompareUseCase::execute(CompareRequest {
            baseline,
            current,
            budgets,
            metric_statistics: BTreeMap::new(),
            significance: None,
            baseline_ref: CompareRef {
                path: None,
                run_id: None,
            },
            current_ref: CompareRef {
                path: None,
                run_id: None,
            },
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
            host_mismatch_policy: HostMismatchPolicy::Ignore,
        })
        .expect("ignore mismatch should succeed");

        assert!(ignore.host_mismatch.is_none());
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use perfgate_types::{
        Delta, Direction, MetricStatistic, MetricStatus, Verdict, VerdictCounts, VerdictStatus,
    };
    use proptest::prelude::*;

    // --- Strategies for generating CompareReceipt ---

    // Strategy for generating valid non-empty strings (for names, IDs, etc.)
    fn non_empty_string() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9_-]{1,20}".prop_map(|s| s)
    }

    // Strategy for ToolInfo
    fn tool_info_strategy() -> impl Strategy<Value = ToolInfo> {
        (non_empty_string(), non_empty_string())
            .prop_map(|(name, version)| ToolInfo { name, version })
    }

    // Strategy for BenchMeta
    fn bench_meta_strategy() -> impl Strategy<Value = BenchMeta> {
        (
            non_empty_string(),
            proptest::option::of(non_empty_string()),
            proptest::collection::vec(non_empty_string(), 1..5),
            1u32..100,
            0u32..10,
            proptest::option::of(1u64..10000),
            proptest::option::of(100u64..60000),
        )
            .prop_map(
                |(name, cwd, command, repeat, warmup, work_units, timeout_ms)| BenchMeta {
                    name,
                    cwd,
                    command,
                    repeat,
                    warmup,
                    work_units,
                    timeout_ms,
                },
            )
    }

    // Strategy for CompareRef
    fn compare_ref_strategy() -> impl Strategy<Value = CompareRef> {
        (
            proptest::option::of(non_empty_string()),
            proptest::option::of(non_empty_string()),
        )
            .prop_map(|(path, run_id)| CompareRef { path, run_id })
    }

    // Strategy for Direction
    fn direction_strategy() -> impl Strategy<Value = Direction> {
        prop_oneof![Just(Direction::Lower), Just(Direction::Higher),]
    }

    // Strategy for Budget - using finite positive floats for thresholds
    fn budget_strategy() -> impl Strategy<Value = Budget> {
        (0.01f64..1.0, 0.01f64..1.0, direction_strategy()).prop_map(
            |(threshold, warn_factor, direction)| {
                // warn_threshold should be <= threshold
                let warn_threshold = threshold * warn_factor;
                Budget {
                    noise_threshold: None,
                    noise_policy: perfgate_types::NoisePolicy::Ignore,
                    threshold,
                    warn_threshold,
                    direction,
                }
            },
        )
    }

    // Strategy for MetricStatus
    fn metric_status_strategy() -> impl Strategy<Value = MetricStatus> {
        prop_oneof![
            Just(MetricStatus::Pass),
            Just(MetricStatus::Warn),
            Just(MetricStatus::Fail),
            Just(MetricStatus::Skip),
        ]
    }

    // Strategy for Delta - using finite positive floats
    fn delta_strategy() -> impl Strategy<Value = Delta> {
        (
            0.1f64..10000.0, // baseline (positive, non-zero)
            0.1f64..10000.0, // current (positive, non-zero)
            metric_status_strategy(),
        )
            .prop_map(|(baseline, current, status)| {
                let ratio = current / baseline;
                let pct = (current - baseline) / baseline;
                let regression = if pct > 0.0 { pct } else { 0.0 };
                Delta {
                    baseline,
                    current,
                    ratio,
                    pct,
                    regression,
                    cv: None,
                    noise_threshold: None,
                    statistic: MetricStatistic::Median,
                    significance: None,
                    status,
                }
            })
    }

    // Strategy for VerdictStatus
    fn verdict_status_strategy() -> impl Strategy<Value = VerdictStatus> {
        prop_oneof![
            Just(VerdictStatus::Pass),
            Just(VerdictStatus::Warn),
            Just(VerdictStatus::Fail),
            Just(VerdictStatus::Skip),
        ]
    }

    // Strategy for VerdictCounts
    fn verdict_counts_strategy() -> impl Strategy<Value = VerdictCounts> {
        (0u32..10, 0u32..10, 0u32..10, 0u32..10).prop_map(|(pass, warn, fail, skip)| {
            VerdictCounts {
                pass,
                warn,
                fail,
                skip,
            }
        })
    }

    // Strategy for Verdict with reasons
    fn verdict_strategy() -> impl Strategy<Value = Verdict> {
        (
            verdict_status_strategy(),
            verdict_counts_strategy(),
            proptest::collection::vec("[a-zA-Z0-9 ]{1,50}", 0..5),
        )
            .prop_map(|(status, counts, reasons)| Verdict {
                status,
                counts,
                reasons,
            })
    }

    // Strategy for Metric
    fn metric_strategy() -> impl Strategy<Value = Metric> {
        prop_oneof![
            Just(Metric::BinaryBytes),
            Just(Metric::CpuMs),
            Just(Metric::CtxSwitches),
            Just(Metric::IoReadBytes),
            Just(Metric::IoWriteBytes),
            Just(Metric::WallMs),
            Just(Metric::MaxRssKb),
            Just(Metric::NetworkPackets),
            Just(Metric::PageFaults),
            Just(Metric::ThroughputPerS),
        ]
    }

    // Strategy for BTreeMap<Metric, Budget>
    fn budgets_map_strategy() -> impl Strategy<Value = BTreeMap<Metric, Budget>> {
        proptest::collection::btree_map(metric_strategy(), budget_strategy(), 0..8)
    }

    // Strategy for BTreeMap<Metric, Delta>
    fn deltas_map_strategy() -> impl Strategy<Value = BTreeMap<Metric, Delta>> {
        proptest::collection::btree_map(metric_strategy(), delta_strategy(), 0..8)
    }

    // Strategy for CompareReceipt
    fn compare_receipt_strategy() -> impl Strategy<Value = CompareReceipt> {
        (
            tool_info_strategy(),
            bench_meta_strategy(),
            compare_ref_strategy(),
            compare_ref_strategy(),
            budgets_map_strategy(),
            deltas_map_strategy(),
            verdict_strategy(),
        )
            .prop_map(
                |(tool, bench, baseline_ref, current_ref, budgets, deltas, verdict)| {
                    CompareReceipt {
                        schema: perfgate_types::COMPARE_SCHEMA_V1.to_string(),
                        tool,
                        bench,
                        baseline_ref,
                        current_ref,
                        budgets,
                        deltas,
                        verdict,
                    }
                },
            )
    }

    // **Property 6: Markdown Rendering Completeness**
    //
    // For any valid CompareReceipt, the rendered Markdown SHALL contain:
    // - A header with the correct verdict emoji (✅ for Pass, ⚠️ for Warn, ❌ for Fail)
    // - The benchmark name
    // - A table row for each metric in deltas
    // - All verdict reasons (if any)
    //
    // **Validates: Requirements 7.2, 7.3, 7.4, 7.5**
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn markdown_rendering_completeness(receipt in compare_receipt_strategy()) {
            let md = render_markdown(&receipt);

            // Verify header contains correct verdict emoji (Requirement 7.2)
            let expected_emoji = match receipt.verdict.status {
                VerdictStatus::Pass => "✅",
                VerdictStatus::Warn => "⚠️",
                VerdictStatus::Fail => "❌",
                VerdictStatus::Skip => "⏭️",
            };
            prop_assert!(
                md.contains(expected_emoji),
                "Markdown should contain verdict emoji '{}' for status {:?}. Got:\n{}",
                expected_emoji,
                receipt.verdict.status,
                md
            );

            // Verify header contains "perfgate" and verdict status word
            let expected_status_word = match receipt.verdict.status {
                VerdictStatus::Pass => "pass",
                VerdictStatus::Warn => "warn",
                VerdictStatus::Fail => "fail",
                VerdictStatus::Skip => "skip",
            };
            prop_assert!(
                md.contains(expected_status_word),
                "Markdown should contain status word '{}'. Got:\n{}",
                expected_status_word,
                md
            );

            // Verify benchmark name is present (Requirement 7.3)
            prop_assert!(
                md.contains(&receipt.bench.name),
                "Markdown should contain benchmark name '{}'. Got:\n{}",
                receipt.bench.name,
                md
            );

            // Verify table header is present (Requirement 7.4)
            prop_assert!(
                md.contains("| metric |"),
                "Markdown should contain table header. Got:\n{}",
                md
            );

            // Verify a table row exists for each metric in deltas (Requirement 7.4)
            for metric in receipt.deltas.keys() {
                let metric_name = metric.as_str();
                prop_assert!(
                    md.contains(metric_name),
                    "Markdown should contain metric '{}'. Got:\n{}",
                    metric_name,
                    md
                );
            }

            // Verify all verdict reasons are present (Requirement 7.5)
            for reason in &receipt.verdict.reasons {
                prop_assert!(
                    md.contains(reason),
                    "Markdown should contain verdict reason '{}'. Got:\n{}",
                    reason,
                    md
                );
            }

            // If there are reasons, verify the Notes section exists
            if !receipt.verdict.reasons.is_empty() {
                prop_assert!(
                    md.contains("**Notes:**"),
                    "Markdown should contain Notes section when there are reasons. Got:\n{}",
                    md
                );
            }
        }
    }

    // **Property 7: GitHub Annotation Generation**
    //
    // For any valid CompareReceipt:
    // - Metrics with Fail status SHALL produce exactly one `::error::` annotation
    // - Metrics with Warn status SHALL produce exactly one `::warning::` annotation
    // - Metrics with Pass status SHALL produce no annotations
    // - Each annotation SHALL contain the bench name, metric name, and delta percentage
    //
    // **Validates: Requirements 8.2, 8.3, 8.4, 8.5**
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn github_annotation_generation(receipt in compare_receipt_strategy()) {
            let annotations = github_annotations(&receipt);

            // Count expected annotations by status
            let expected_fail_count = receipt.deltas.values()
                .filter(|d| d.status == MetricStatus::Fail)
                .count();
            let expected_warn_count = receipt.deltas.values()
                .filter(|d| d.status == MetricStatus::Warn)
                .count();
            let expected_pass_count = receipt.deltas.values()
                .filter(|d| d.status == MetricStatus::Pass)
                .count();

            // Count actual annotations by type
            let actual_error_count = annotations.iter()
                .filter(|a| a.starts_with("::error::"))
                .count();
            let actual_warning_count = annotations.iter()
                .filter(|a| a.starts_with("::warning::"))
                .count();

            // Requirement 8.2: Fail status produces exactly one ::error:: annotation
            prop_assert_eq!(
                actual_error_count,
                expected_fail_count,
                "Expected {} ::error:: annotations for {} Fail metrics, got {}. Annotations: {:?}",
                expected_fail_count,
                expected_fail_count,
                actual_error_count,
                annotations
            );

            // Requirement 8.3: Warn status produces exactly one ::warning:: annotation
            prop_assert_eq!(
                actual_warning_count,
                expected_warn_count,
                "Expected {} ::warning:: annotations for {} Warn metrics, got {}. Annotations: {:?}",
                expected_warn_count,
                expected_warn_count,
                actual_warning_count,
                annotations
            );

            // Requirement 8.4: Pass status produces no annotations
            // Total annotations should equal fail + warn count (no pass annotations)
            let total_annotations = annotations.len();
            let expected_total = expected_fail_count + expected_warn_count;
            prop_assert_eq!(
                total_annotations,
                expected_total,
                "Expected {} total annotations (fail: {}, warn: {}, pass: {} should produce none), got {}. Annotations: {:?}",
                expected_total,
                expected_fail_count,
                expected_warn_count,
                expected_pass_count,
                total_annotations,
                annotations
            );

            // Requirement 8.5: Each annotation contains bench name, metric name, and delta percentage
            for (metric, delta) in &receipt.deltas {
                if delta.status == MetricStatus::Pass || delta.status == MetricStatus::Skip {
                    continue; // Pass metrics don't produce annotations
                }

                let metric_name = metric.as_str();

                // Find the annotation for this metric
                let matching_annotation = annotations.iter().find(|a| a.contains(metric_name));

                prop_assert!(
                    matching_annotation.is_some(),
                    "Expected annotation for metric '{}' with status {:?}. Annotations: {:?}",
                    metric_name,
                    delta.status,
                    annotations
                );

                let annotation = matching_annotation.unwrap();

                // Verify annotation contains bench name
                prop_assert!(
                    annotation.contains(&receipt.bench.name),
                    "Annotation should contain bench name '{}'. Got: {}",
                    receipt.bench.name,
                    annotation
                );

                // Verify annotation contains metric name
                prop_assert!(
                    annotation.contains(metric_name),
                    "Annotation should contain metric name '{}'. Got: {}",
                    metric_name,
                    annotation
                );

                // Verify annotation contains delta percentage (formatted as +X.XX% or -X.XX%)
                // The format_pct function produces strings like "+10.00%" or "-5.50%"
                let pct_str = format_pct(delta.pct);
                prop_assert!(
                    annotation.contains(&pct_str),
                    "Annotation should contain delta percentage '{}'. Got: {}",
                    pct_str,
                    annotation
                );

                // Verify correct annotation type based on status
                match delta.status {
                    MetricStatus::Fail => {
                        prop_assert!(
                            annotation.starts_with("::error::"),
                            "Fail metric should produce ::error:: annotation. Got: {}",
                            annotation
                        );
                    }
                    MetricStatus::Warn => {
                        prop_assert!(
                            annotation.starts_with("::warning::"),
                            "Warn metric should produce ::warning:: annotation. Got: {}",
                            annotation
                        );
                    }
                    MetricStatus::Pass | MetricStatus::Skip => unreachable!(),
                }
            }
        }
    }
}
