//! CheckUseCase - Config-driven one-command workflow.
//!
//! This module implements the `check` command which:
//! 1. Loads a config file
//! 2. Finds a bench by name
//! 3. Runs the bench
//! 4. Loads baseline (if exists)
//! 5. Compares results
//! 6. Generates all artifacts (run.json, compare.json, report.json, comment.md)

use crate::{
    Clock, CompareRequest, CompareUseCase, RunBenchRequest, RunBenchUseCase, format_metric,
    format_pct,
};
use anyhow::Context;
use perfgate_adapters::{HostProbe, ProcessRunner};
use perfgate_domain::SignificancePolicy;
use perfgate_types::{
    BenchConfigFile, Budget, CHECK_ID_BASELINE, CHECK_ID_BUDGET, CompareReceipt, CompareRef,
    ConfigFile, ConfigValidationError, FINDING_CODE_BASELINE_MISSING, FINDING_CODE_METRIC_FAIL,
    FINDING_CODE_METRIC_WARN, FindingData, HostMismatchPolicy, Metric, MetricStatistic,
    MetricStatus, PerfgateError, PerfgateReport, REPORT_SCHEMA_V1, ReportFinding, ReportSummary,
    RunReceipt, Severity, ToolInfo, VERDICT_REASON_NO_BASELINE, Verdict, VerdictCounts,
    VerdictStatus,
};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Request for the check use case.
#[derive(Debug, Clone)]
pub struct CheckRequest {
    /// The loaded configuration file.
    pub config: ConfigFile,

    /// Name of the bench to run.
    pub bench_name: String,

    /// Output directory for artifacts.
    pub out_dir: PathBuf,

    /// Optional baseline receipt (already loaded).
    pub baseline: Option<RunReceipt>,

    /// Path to the baseline file (for reference in compare receipt).
    pub baseline_path: Option<PathBuf>,

    /// If true, fail if baseline is missing.
    pub require_baseline: bool,

    /// If true, treat warn verdict as failure.
    pub fail_on_warn: bool,

    /// Tool info for receipts.
    pub tool: ToolInfo,

    /// Environment variables for the benchmark.
    pub env: Vec<(String, String)>,

    /// Max bytes captured from stdout/stderr per run.
    pub output_cap_bytes: usize,

    /// If true, do not treat nonzero exit codes as a tool error.
    pub allow_nonzero: bool,

    /// Policy for handling host mismatches when comparing against baseline.
    pub host_mismatch_policy: HostMismatchPolicy,

    /// Optional p-value threshold for significance analysis.
    pub significance_alpha: Option<f64>,

    /// Minimum samples per side before significance is computed.
    pub significance_min_samples: u32,

    /// Require significance to escalate warn/fail statuses.
    pub require_significance: bool,
}

/// Outcome of the check use case.
#[derive(Debug, Clone)]
pub struct CheckOutcome {
    /// The run receipt produced.
    pub run_receipt: RunReceipt,

    /// Path where run receipt was written.
    pub run_path: PathBuf,

    /// The compare receipt (None if no baseline).
    pub compare_receipt: Option<CompareReceipt>,

    /// Path where compare receipt was written (None if no baseline).
    pub compare_path: Option<PathBuf>,

    /// The report (always present for cockpit integration).
    pub report: PerfgateReport,

    /// Path where report was written.
    pub report_path: PathBuf,

    /// The markdown content.
    pub markdown: String,

    /// Path where markdown was written.
    pub markdown_path: PathBuf,

    /// Warnings generated during the check.
    pub warnings: Vec<String>,

    /// True if the check failed (based on verdict and flags).
    pub failed: bool,

    /// Exit code to use (0=pass, 2=fail, 3=warn with fail-on-warn).
    pub exit_code: i32,
}

/// Use case for running a config-driven check.
pub struct CheckUseCase<R: ProcessRunner + Clone, H: HostProbe + Clone, C: Clock + Clone> {
    runner: R,
    host_probe: H,
    clock: C,
}

impl<R: ProcessRunner + Clone, H: HostProbe + Clone, C: Clock + Clone> CheckUseCase<R, H, C> {
    pub fn new(runner: R, host_probe: H, clock: C) -> Self {
        Self {
            runner,
            host_probe,
            clock,
        }
    }

    /// Execute the check workflow.
    pub fn execute(&self, req: CheckRequest) -> anyhow::Result<CheckOutcome> {
        let mut warnings = Vec::new();

        // 1. Find the bench config by name
        let bench_config = req
            .config
            .benches
            .iter()
            .find(|b| b.name == req.bench_name)
            .ok_or_else(|| {
                ConfigValidationError::BenchName(format!(
                    "bench '{}' not found in config",
                    req.bench_name
                ))
            })?;

        // 2. Build run request from config
        let run_request = self.build_run_request(bench_config, &req)?;

        // 3. Run the benchmark
        let run_usecase = RunBenchUseCase::new(
            self.runner.clone(),
            self.host_probe.clone(),
            self.clock.clone(),
            req.tool.clone(),
        );
        let run_outcome = run_usecase.execute(run_request)?;
        let run_receipt = run_outcome.receipt;

        // 4. Write run receipt
        let run_path = req.out_dir.join("run.json");

        // 5. Handle baseline
        let report_path = req.out_dir.join("report.json");
        let (compare_receipt, compare_path, report) = if let Some(baseline) = &req.baseline {
            // Build budgets from config
            let (budgets, metric_statistics) =
                self.build_budgets(bench_config, &req.config, baseline, &run_receipt)?;

            // Compare
            let compare_req = CompareRequest {
                baseline: baseline.clone(),
                current: run_receipt.clone(),
                budgets,
                metric_statistics,
                significance: req
                    .significance_alpha
                    .map(|alpha| {
                        SignificancePolicy::new(
                            alpha,
                            req.significance_min_samples as usize,
                            req.require_significance,
                        )
                    })
                    .transpose()?,
                baseline_ref: CompareRef {
                    path: req.baseline_path.as_ref().map(|p| p.display().to_string()),
                    run_id: Some(baseline.run.id.clone()),
                },
                current_ref: CompareRef {
                    path: Some(run_path.display().to_string()),
                    run_id: Some(run_receipt.run.id.clone()),
                },
                tool: req.tool.clone(),
                host_mismatch_policy: req.host_mismatch_policy,
            };

            let compare_result = CompareUseCase::execute(compare_req)?;

            // Add host mismatch warnings if detected (for Warn policy)
            if let Some(mismatch) = &compare_result.host_mismatch {
                for reason in &mismatch.reasons {
                    warnings.push(format!("host mismatch: {}", reason));
                }
            }

            // Build report
            let report = build_report(&compare_result.receipt);

            let compare_path = req.out_dir.join("compare.json");

            (Some(compare_result.receipt), Some(compare_path), report)
        } else {
            // No baseline
            if req.require_baseline {
                use perfgate_error::IoError;
                return Err(PerfgateError::Io(IoError::BaselineNotFound {
                    path: format!("bench '{}'", req.bench_name),
                })
                .into());
            }
            warnings.push(format!(
                "no baseline found for bench '{}', skipping comparison",
                req.bench_name
            ));

            // Build a no-baseline report for cockpit integration
            let report = build_no_baseline_report(&run_receipt);

            (None, None, report)
        };

        // 6. Generate markdown
        let markdown = if let Some(compare) = &compare_receipt {
            crate::render_markdown(compare)
        } else {
            render_no_baseline_markdown(&run_receipt, &warnings)
        };

        let markdown_path = req.out_dir.join("comment.md");

        // 7. Determine exit code
        let (failed, exit_code) = if let Some(compare) = &compare_receipt {
            match compare.verdict.status {
                VerdictStatus::Pass => (false, 0),
                VerdictStatus::Warn => {
                    if req.fail_on_warn {
                        (true, 3)
                    } else {
                        (false, 0)
                    }
                }
                VerdictStatus::Fail => (true, 2),
            }
        } else {
            // No baseline - pass by default (unless require_baseline was set, which already bailed)
            (false, 0)
        };

        Ok(CheckOutcome {
            run_receipt,
            run_path,
            compare_receipt,
            compare_path,
            report,
            report_path,
            markdown,
            markdown_path,
            warnings,
            failed,
            exit_code,
        })
    }

    fn build_run_request(
        &self,
        bench: &BenchConfigFile,
        req: &CheckRequest,
    ) -> anyhow::Result<RunBenchRequest> {
        let defaults = &req.config.defaults;

        // Resolve repeat: bench > defaults > 5
        let repeat = bench.repeat.or(defaults.repeat).unwrap_or(5);

        // Resolve warmup: bench > defaults > 0
        let warmup = bench.warmup.or(defaults.warmup).unwrap_or(0);

        // Parse timeout if present
        let timeout = bench
            .timeout
            .as_deref()
            .map(|s| {
                humantime::parse_duration(s)
                    .with_context(|| format!("invalid timeout '{}' for bench '{}'", s, bench.name))
            })
            .transpose()?;

        // Resolve cwd
        let cwd = bench.cwd.as_ref().map(PathBuf::from);

        Ok(RunBenchRequest {
            name: bench.name.clone(),
            cwd,
            command: bench.command.clone(),
            repeat,
            warmup,
            work_units: bench.work,
            timeout,
            env: req.env.clone(),
            output_cap_bytes: req.output_cap_bytes,
            allow_nonzero: req.allow_nonzero,
            include_hostname_hash: false,
        })
    }

    fn build_budgets(
        &self,
        bench: &BenchConfigFile,
        config: &ConfigFile,
        baseline: &RunReceipt,
        current: &RunReceipt,
    ) -> anyhow::Result<(BTreeMap<Metric, Budget>, BTreeMap<Metric, MetricStatistic>)> {
        let defaults = &config.defaults;

        // Global defaults
        let global_threshold = defaults.threshold.unwrap_or(0.20);
        let global_warn_factor = defaults.warn_factor.unwrap_or(0.90);

        // Determine candidate metrics: those present in both baseline+current
        let mut candidates = Vec::new();
        candidates.push(Metric::WallMs);
        if baseline.stats.cpu_ms.is_some() && current.stats.cpu_ms.is_some() {
            candidates.push(Metric::CpuMs);
        }
        if baseline.stats.page_faults.is_some() && current.stats.page_faults.is_some() {
            candidates.push(Metric::PageFaults);
        }
        if baseline.stats.ctx_switches.is_some() && current.stats.ctx_switches.is_some() {
            candidates.push(Metric::CtxSwitches);
        }
        if baseline.stats.max_rss_kb.is_some() && current.stats.max_rss_kb.is_some() {
            candidates.push(Metric::MaxRssKb);
        }
        if baseline.stats.binary_bytes.is_some() && current.stats.binary_bytes.is_some() {
            candidates.push(Metric::BinaryBytes);
        }
        if baseline.stats.throughput_per_s.is_some() && current.stats.throughput_per_s.is_some() {
            candidates.push(Metric::ThroughputPerS);
        }

        let mut budgets = BTreeMap::new();
        let mut metric_statistics = BTreeMap::new();

        for metric in candidates {
            // Check for per-bench budget override
            let override_opt = bench.budgets.as_ref().and_then(|b| b.get(&metric).cloned());

            let threshold = override_opt
                .as_ref()
                .and_then(|o| o.threshold)
                .unwrap_or(global_threshold);

            let warn_factor = override_opt
                .as_ref()
                .and_then(|o| o.warn_factor)
                .unwrap_or(global_warn_factor);

            let warn_threshold = threshold * warn_factor;

            let direction = override_opt
                .as_ref()
                .and_then(|o| o.direction)
                .unwrap_or_else(|| metric.default_direction());

            let statistic = override_opt
                .as_ref()
                .and_then(|o| o.statistic)
                .unwrap_or(MetricStatistic::Median);

            budgets.insert(
                metric,
                Budget {
                    threshold,
                    warn_threshold,
                    direction,
                },
            );
            metric_statistics.insert(metric, statistic);
        }

        Ok((budgets, metric_statistics))
    }
}

/// Build a PerfgateReport from a CompareReceipt.
fn build_report(compare: &CompareReceipt) -> PerfgateReport {
    let mut findings = Vec::new();

    for (metric, delta) in &compare.deltas {
        let severity = match delta.status {
            MetricStatus::Pass => continue,
            MetricStatus::Warn => Severity::Warn,
            MetricStatus::Fail => Severity::Fail,
        };

        let budget = compare.budgets.get(metric);
        let (threshold, direction) = budget
            .map(|b| (b.threshold, b.direction))
            .unwrap_or((0.20, metric.default_direction()));

        let code = match delta.status {
            MetricStatus::Warn => FINDING_CODE_METRIC_WARN.to_string(),
            MetricStatus::Fail => FINDING_CODE_METRIC_FAIL.to_string(),
            MetricStatus::Pass => unreachable!(),
        };

        let metric_name = format_metric(*metric).to_string();
        let message = format!(
            "{} regression: {} (threshold: {:.1}%)",
            metric_name,
            format_pct(delta.pct),
            threshold * 100.0
        );

        findings.push(ReportFinding {
            check_id: CHECK_ID_BUDGET.to_string(),
            code,
            severity,
            message,
            data: Some(FindingData {
                metric_name,
                baseline: delta.baseline,
                current: delta.current,
                regression_pct: delta.pct * 100.0,
                threshold,
                direction,
            }),
        });
    }

    let summary = ReportSummary {
        pass_count: compare.verdict.counts.pass,
        warn_count: compare.verdict.counts.warn,
        fail_count: compare.verdict.counts.fail,
        total_count: compare.verdict.counts.pass
            + compare.verdict.counts.warn
            + compare.verdict.counts.fail,
    };

    PerfgateReport {
        report_type: REPORT_SCHEMA_V1.to_string(),
        verdict: compare.verdict.clone(),
        compare: Some(compare.clone()),
        findings,
        summary,
    }
}

/// Build a PerfgateReport for the case when there is no baseline.
///
/// Returns a report with Warn status (not Pass) to indicate that while
/// the check is non-blocking by default, no actual performance evaluation
/// occurred. The cockpit can highlight this as "baseline missing" rather
/// than incorrectly showing "pass".
fn build_no_baseline_report(run: &RunReceipt) -> PerfgateReport {
    // Warn verdict: the sensor ran but no comparison was possible
    let verdict = Verdict {
        status: VerdictStatus::Warn,
        counts: VerdictCounts {
            pass: 0,
            warn: 1,
            fail: 0,
        },
        reasons: vec![VERDICT_REASON_NO_BASELINE.to_string()],
    };

    // Single finding for the baseline-missing condition
    let finding = ReportFinding {
        check_id: CHECK_ID_BASELINE.to_string(),
        code: FINDING_CODE_BASELINE_MISSING.to_string(),
        severity: Severity::Warn,
        message: format!(
            "No baseline found for bench '{}'; comparison skipped",
            run.bench.name
        ),
        data: None, // No metric data for structural findings
    };

    PerfgateReport {
        report_type: REPORT_SCHEMA_V1.to_string(),
        verdict,
        compare: None, // No synthetic compare receipt
        findings: vec![finding],
        summary: ReportSummary {
            pass_count: 0,
            warn_count: 1,
            fail_count: 0,
            total_count: 1,
        },
    }
}

/// Render markdown for the case when there is no baseline.
fn render_no_baseline_markdown(run: &RunReceipt, warnings: &[String]) -> String {
    let mut out = String::new();

    out.push_str("## perfgate: no baseline\n\n");
    out.push_str(&format!("**Bench:** `{}`\n\n", run.bench.name));
    out.push_str("No baseline found for comparison. This run will establish a new baseline.\n\n");

    out.push_str("### Current Results\n\n");
    out.push_str("| metric | value |\n");
    out.push_str("|---|---:|\n");
    out.push_str(&format!(
        "| `wall_ms` | {} ms |\n",
        run.stats.wall_ms.median
    ));

    if let Some(cpu) = &run.stats.cpu_ms {
        out.push_str(&format!("| `cpu_ms` | {} ms |\n", cpu.median));
    }

    if let Some(page_faults) = &run.stats.page_faults {
        out.push_str(&format!(
            "| `page_faults` | {} count |\n",
            page_faults.median
        ));
    }

    if let Some(ctx_switches) = &run.stats.ctx_switches {
        out.push_str(&format!(
            "| `ctx_switches` | {} count |\n",
            ctx_switches.median
        ));
    }

    if let Some(rss) = &run.stats.max_rss_kb {
        out.push_str(&format!("| `max_rss_kb` | {} KB |\n", rss.median));
    }

    if let Some(binary_bytes) = &run.stats.binary_bytes {
        out.push_str(&format!(
            "| `binary_bytes` | {} bytes |\n",
            binary_bytes.median
        ));
    }

    if let Some(throughput) = &run.stats.throughput_per_s {
        out.push_str(&format!(
            "| `throughput_per_s` | {:.3} /s |\n",
            throughput.median
        ));
    }

    if !warnings.is_empty() {
        out.push_str("\n**Warnings:**\n");
        for w in warnings {
            out.push_str(&format!("- {}\n", w));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_adapters::{AdapterError, CommandSpec, HostProbeOptions, RunResult};
    use perfgate_types::{
        BaselineServerConfig, BenchConfigFile, BenchMeta, BudgetOverride, COMPARE_SCHEMA_V1,
        CompareReceipt, DefaultsConfig, Delta, Direction, HostInfo, Metric, RunMeta, Sample, Stats,
        U64Summary, Verdict, VerdictCounts,
    };
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    fn make_run_receipt(wall_ms_median: u64) -> RunReceipt {
        RunReceipt {
            schema: perfgate_types::RUN_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
            run: RunMeta {
                id: "test-run".to_string(),
                started_at: "2024-01-01T00:00:00Z".to_string(),
                ended_at: "2024-01-01T00:01:00Z".to_string(),
                host: HostInfo {
                    os: "linux".to_string(),
                    arch: "x86_64".to_string(),
                    cpu_count: None,
                    memory_bytes: None,
                    hostname_hash: None,
                },
            },
            bench: BenchMeta {
                name: "test-bench".to_string(),
                cwd: None,
                command: vec!["echo".to_string(), "hello".to_string()],
                repeat: 5,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            samples: vec![Sample {
                wall_ms: wall_ms_median,
                exit_code: 0,
                warmup: false,
                timed_out: false,
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: Some(1024),
                binary_bytes: None,
                stdout: None,
                stderr: None,
            }],
            stats: Stats {
                wall_ms: U64Summary {
                    median: wall_ms_median,
                    min: wall_ms_median.saturating_sub(10),
                    max: wall_ms_median.saturating_add(10),
                },
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: Some(U64Summary {
                    median: 1024,
                    min: 1000,
                    max: 1100,
                }),
                binary_bytes: None,
                throughput_per_s: None,
            },
        }
    }

    #[derive(Clone)]
    struct TestRunner {
        runs: Arc<Mutex<Vec<RunResult>>>,
    }

    impl TestRunner {
        fn new(runs: Vec<RunResult>) -> Self {
            Self {
                runs: Arc::new(Mutex::new(runs)),
            }
        }
    }

    impl ProcessRunner for TestRunner {
        fn run(&self, _spec: &CommandSpec) -> Result<RunResult, AdapterError> {
            let mut runs = self.runs.lock().expect("lock runs");
            if runs.is_empty() {
                return Err(AdapterError::Other("no more queued runs".to_string()));
            }
            Ok(runs.remove(0))
        }
    }

    #[derive(Clone)]
    struct TestHostProbe {
        host: HostInfo,
    }

    impl TestHostProbe {
        fn new(host: HostInfo) -> Self {
            Self { host }
        }
    }

    impl HostProbe for TestHostProbe {
        fn probe(&self, _options: &HostProbeOptions) -> HostInfo {
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

    fn run_result(wall_ms: u64, exit_code: i32, timed_out: bool) -> RunResult {
        RunResult {
            wall_ms,
            exit_code,
            timed_out,
            cpu_ms: None,
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: None,
            binary_bytes: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }

    fn make_baseline_receipt(wall_ms: u64, host: HostInfo, max_rss_kb: Option<u64>) -> RunReceipt {
        RunReceipt {
            schema: perfgate_types::RUN_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
            run: RunMeta {
                id: "baseline-id".to_string(),
                started_at: "2024-01-01T00:00:00Z".to_string(),
                ended_at: "2024-01-01T00:00:01Z".to_string(),
                host,
            },
            bench: BenchMeta {
                name: "bench".to_string(),
                cwd: None,
                command: vec!["echo".to_string(), "hello".to_string()],
                repeat: 1,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            samples: Vec::new(),
            stats: Stats {
                wall_ms: U64Summary {
                    median: wall_ms,
                    min: wall_ms,
                    max: wall_ms,
                },
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: max_rss_kb.map(|v| U64Summary {
                    median: v,
                    min: v,
                    max: v,
                }),
                binary_bytes: None,
                throughput_per_s: None,
            },
        }
    }

    fn make_check_request(
        config: ConfigFile,
        baseline: Option<RunReceipt>,
        host_mismatch_policy: HostMismatchPolicy,
        fail_on_warn: bool,
    ) -> CheckRequest {
        CheckRequest {
            config,
            bench_name: "bench".to_string(),
            out_dir: PathBuf::from("out"),
            baseline,
            baseline_path: None,
            require_baseline: false,
            fail_on_warn,
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
            env: vec![],
            output_cap_bytes: 1024,
            allow_nonzero: false,
            host_mismatch_policy,
            significance_alpha: None,
            significance_min_samples: 8,
            require_significance: false,
        }
    }

    #[test]
    fn test_build_report_from_compare() {
        let mut budgets = BTreeMap::new();
        budgets.insert(
            Metric::WallMs,
            Budget {
                threshold: 0.20,
                warn_threshold: 0.18,
                direction: Direction::Lower,
            },
        );

        let mut deltas = BTreeMap::new();
        deltas.insert(
            Metric::WallMs,
            Delta {
                baseline: 1000.0,
                current: 1250.0,
                ratio: 1.25,
                pct: 0.25,
                regression: 0.25,
                statistic: MetricStatistic::Median,
                significance: None,
                status: MetricStatus::Fail,
            },
        );

        let compare = CompareReceipt {
            schema: COMPARE_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
            bench: BenchMeta {
                name: "test-bench".to_string(),
                cwd: None,
                command: vec!["echo".to_string()],
                repeat: 5,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            baseline_ref: CompareRef {
                path: Some("baseline.json".to_string()),
                run_id: Some("baseline-id".to_string()),
            },
            current_ref: CompareRef {
                path: Some("current.json".to_string()),
                run_id: Some("current-id".to_string()),
            },
            budgets,
            deltas,
            verdict: Verdict {
                status: VerdictStatus::Fail,
                counts: VerdictCounts {
                    pass: 0,
                    warn: 0,
                    fail: 1,
                },
                reasons: vec!["wall_ms_fail".to_string()],
            },
        };

        let report = build_report(&compare);

        assert_eq!(report.report_type, REPORT_SCHEMA_V1);
        assert_eq!(report.verdict.status, VerdictStatus::Fail);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].severity, Severity::Fail);
        assert_eq!(report.findings[0].check_id, "perf.budget");
        assert_eq!(report.summary.fail_count, 1);
        assert_eq!(report.summary.total_count, 1);
    }

    #[test]
    fn test_render_no_baseline_markdown() {
        let run = make_run_receipt(1000);
        let warnings = vec!["no baseline found".to_string()];

        let md = render_no_baseline_markdown(&run, &warnings);

        assert!(md.contains("perfgate: no baseline"));
        assert!(md.contains("test-bench"));
        assert!(md.contains("wall_ms"));
        assert!(md.contains("no baseline found"));
    }

    #[test]
    fn test_build_no_baseline_report() {
        let run = make_run_receipt(1000);

        let report = build_no_baseline_report(&run);

        // Verify report structure
        assert_eq!(report.report_type, REPORT_SCHEMA_V1);

        // Verify verdict is Warn (not Pass) - baseline missing is not "green"
        assert_eq!(report.verdict.status, VerdictStatus::Warn);
        assert_eq!(report.verdict.counts.pass, 0);
        assert_eq!(report.verdict.counts.warn, 1);
        assert_eq!(report.verdict.counts.fail, 0);
        assert_eq!(report.verdict.reasons.len(), 1);
        assert_eq!(report.verdict.reasons[0], "no_baseline");

        // Verify single finding for baseline-missing condition
        assert_eq!(report.findings.len(), 1);
        let finding = &report.findings[0];
        assert_eq!(finding.check_id, "perf.baseline");
        assert_eq!(finding.code, "missing");
        assert_eq!(finding.severity, Severity::Warn);
        assert!(finding.message.contains("No baseline found"));
        assert!(finding.message.contains("test-bench"));
        assert!(finding.data.is_none()); // No metric data for structural findings

        // Verify summary reflects the warning
        assert_eq!(report.summary.pass_count, 0);
        assert_eq!(report.summary.warn_count, 1);
        assert_eq!(report.summary.fail_count, 0);
        assert_eq!(report.summary.total_count, 1);

        // Verify no compare receipt (no synthetic comparison)
        assert!(report.compare.is_none());
    }

    #[test]
    fn build_run_request_resolves_defaults_and_timeout() {
        let bench = BenchConfigFile {
            name: "bench".to_string(),
            cwd: Some("some/dir".to_string()),
            work: Some(42),
            timeout: Some("2s".to_string()),
            command: vec!["echo".to_string(), "ok".to_string()],
            repeat: None,
            warmup: None,
            metrics: None,
            budgets: None,
        };

        let config = ConfigFile {
            defaults: DefaultsConfig {
                repeat: Some(7),
                warmup: Some(2),
                threshold: None,
                warn_factor: None,
                out_dir: None,
                baseline_dir: None,
                baseline_pattern: None,
                markdown_template: None,
            },
            baseline_server: BaselineServerConfig::default(),
            benches: vec![bench.clone()],
        };

        let req = CheckRequest {
            config: config.clone(),
            bench_name: "bench".to_string(),
            out_dir: PathBuf::from("out"),
            baseline: None,
            baseline_path: None,
            require_baseline: false,
            fail_on_warn: false,
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
            env: vec![("K".to_string(), "V".to_string())],
            output_cap_bytes: 512,
            allow_nonzero: true,
            host_mismatch_policy: HostMismatchPolicy::Warn,
            significance_alpha: None,
            significance_min_samples: 8,
            require_significance: false,
        };

        let usecase = CheckUseCase::new(
            TestRunner::new(Vec::new()),
            TestHostProbe::new(HostInfo {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                cpu_count: None,
                memory_bytes: None,
                hostname_hash: None,
            }),
            TestClock::new("2024-01-01T00:00:00Z"),
        );

        let run_req = usecase
            .build_run_request(&bench, &req)
            .expect("build run request");
        assert_eq!(run_req.repeat, 7);
        assert_eq!(run_req.warmup, 2);
        assert_eq!(run_req.work_units, Some(42));
        assert_eq!(run_req.timeout, Some(Duration::from_secs(2)));
        assert_eq!(run_req.output_cap_bytes, 512);
        assert_eq!(run_req.env.len(), 1);
    }

    #[test]
    fn build_run_request_rejects_invalid_timeout() {
        let bench = BenchConfigFile {
            name: "bench".to_string(),
            cwd: None,
            work: None,
            timeout: Some("not-a-duration".to_string()),
            command: vec!["echo".to_string()],
            repeat: None,
            warmup: None,
            metrics: None,
            budgets: None,
        };
        let config = ConfigFile::default();
        let req = make_check_request(config, None, HostMismatchPolicy::Warn, false);

        let usecase = CheckUseCase::new(
            TestRunner::new(Vec::new()),
            TestHostProbe::new(HostInfo {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                cpu_count: None,
                memory_bytes: None,
                hostname_hash: None,
            }),
            TestClock::new("2024-01-01T00:00:00Z"),
        );

        let err = usecase.build_run_request(&bench, &req).unwrap_err();
        assert!(
            err.to_string().contains("invalid timeout"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn build_budgets_applies_overrides_and_defaults() {
        let mut overrides = BTreeMap::new();
        overrides.insert(
            Metric::WallMs,
            BudgetOverride {
                threshold: Some(0.3),
                direction: Some(Direction::Higher),
                warn_factor: Some(0.8),
                statistic: Some(MetricStatistic::P95),
            },
        );

        let bench = BenchConfigFile {
            name: "bench".to_string(),
            cwd: None,
            work: None,
            timeout: None,
            command: vec!["echo".to_string()],
            repeat: None,
            warmup: None,
            metrics: None,
            budgets: Some(overrides),
        };

        let config = ConfigFile {
            defaults: DefaultsConfig {
                repeat: None,
                warmup: None,
                threshold: Some(0.2),
                warn_factor: Some(0.5),
                out_dir: None,
                baseline_dir: None,
                baseline_pattern: None,
                markdown_template: None,
            },
            baseline_server: BaselineServerConfig::default(),
            benches: vec![bench.clone()],
        };

        let baseline = make_baseline_receipt(
            100,
            HostInfo {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                cpu_count: None,
                memory_bytes: None,
                hostname_hash: None,
            },
            Some(1024),
        );
        let current = make_baseline_receipt(
            110,
            HostInfo {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                cpu_count: None,
                memory_bytes: None,
                hostname_hash: None,
            },
            Some(2048),
        );

        let usecase = CheckUseCase::new(
            TestRunner::new(Vec::new()),
            TestHostProbe::new(HostInfo {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                cpu_count: None,
                memory_bytes: None,
                hostname_hash: None,
            }),
            TestClock::new("2024-01-01T00:00:00Z"),
        );

        let (budgets, statistics) = usecase
            .build_budgets(&bench, &config, &baseline, &current)
            .expect("build budgets");

        let wall = budgets.get(&Metric::WallMs).expect("wall budget");
        assert!((wall.threshold - 0.3).abs() < f64::EPSILON);
        assert!((wall.warn_threshold - 0.24).abs() < f64::EPSILON);
        assert_eq!(wall.direction, Direction::Higher);

        let max_rss = budgets.get(&Metric::MaxRssKb).expect("max_rss budget");
        assert!((max_rss.threshold - 0.2).abs() < f64::EPSILON);
        assert!((max_rss.warn_threshold - 0.1).abs() < f64::EPSILON);
        assert_eq!(max_rss.direction, Direction::Lower);

        assert_eq!(statistics.get(&Metric::WallMs), Some(&MetricStatistic::P95));
        assert_eq!(
            statistics.get(&Metric::MaxRssKb),
            Some(&MetricStatistic::Median)
        );
    }

    #[test]
    fn execute_no_baseline_builds_warn_report() {
        let bench = BenchConfigFile {
            name: "bench".to_string(),
            cwd: None,
            work: None,
            timeout: None,
            command: vec!["echo".to_string(), "ok".to_string()],
            repeat: Some(1),
            warmup: Some(0),
            metrics: None,
            budgets: None,
        };
        let config = ConfigFile {
            defaults: DefaultsConfig::default(),
            baseline_server: BaselineServerConfig::default(),
            benches: vec![bench],
        };

        let runner = TestRunner::new(vec![run_result(100, 0, false)]);
        let host_probe = TestHostProbe::new(HostInfo {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            cpu_count: None,
            memory_bytes: None,
            hostname_hash: None,
        });
        let clock = TestClock::new("2024-01-01T00:00:00Z");
        let usecase = CheckUseCase::new(runner, host_probe, clock);

        let outcome = usecase
            .execute(make_check_request(
                config,
                None,
                HostMismatchPolicy::Warn,
                false,
            ))
            .expect("check should succeed");

        assert!(outcome.compare_receipt.is_none());
        assert_eq!(outcome.report.verdict.status, VerdictStatus::Warn);
        assert!(
            outcome
                .warnings
                .iter()
                .any(|w| w.contains("no baseline found")),
            "expected no-baseline warning"
        );
        assert!(!outcome.failed);
        assert_eq!(outcome.exit_code, 0);
    }

    #[test]
    fn execute_with_baseline_emits_host_mismatch_warning() {
        let bench = BenchConfigFile {
            name: "bench".to_string(),
            cwd: None,
            work: None,
            timeout: None,
            command: vec!["echo".to_string(), "ok".to_string()],
            repeat: Some(1),
            warmup: Some(0),
            metrics: None,
            budgets: None,
        };
        let config = ConfigFile {
            defaults: DefaultsConfig::default(),
            baseline_server: BaselineServerConfig::default(),
            benches: vec![bench],
        };

        let baseline = make_baseline_receipt(
            100,
            HostInfo {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                cpu_count: Some(4),
                memory_bytes: None,
                hostname_hash: None,
            },
            None,
        );

        let runner = TestRunner::new(vec![run_result(100, 0, false)]);
        let host_probe = TestHostProbe::new(HostInfo {
            os: "windows".to_string(),
            arch: "x86_64".to_string(),
            cpu_count: Some(4),
            memory_bytes: None,
            hostname_hash: None,
        });
        let clock = TestClock::new("2024-01-01T00:00:00Z");
        let usecase = CheckUseCase::new(runner, host_probe, clock);

        let outcome = usecase
            .execute(make_check_request(
                config,
                Some(baseline),
                HostMismatchPolicy::Warn,
                false,
            ))
            .expect("check should succeed");

        assert!(outcome.compare_receipt.is_some());
        assert!(
            outcome.warnings.iter().any(|w| w.contains("host mismatch")),
            "expected host mismatch warning"
        );
    }

    #[test]
    fn execute_fail_on_warn_sets_exit_code_3() {
        let bench = BenchConfigFile {
            name: "bench".to_string(),
            cwd: None,
            work: None,
            timeout: None,
            command: vec!["echo".to_string(), "ok".to_string()],
            repeat: Some(1),
            warmup: Some(0),
            metrics: None,
            budgets: None,
        };
        let config = ConfigFile {
            defaults: DefaultsConfig {
                repeat: None,
                warmup: None,
                threshold: Some(0.2),
                warn_factor: Some(0.5),
                out_dir: None,
                baseline_dir: None,
                baseline_pattern: None,
                markdown_template: None,
            },
            baseline_server: BaselineServerConfig::default(),
            benches: vec![bench],
        };

        let baseline = make_baseline_receipt(
            100,
            HostInfo {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                cpu_count: None,
                memory_bytes: None,
                hostname_hash: None,
            },
            None,
        );

        let runner = TestRunner::new(vec![run_result(115, 0, false)]);
        let host_probe = TestHostProbe::new(HostInfo {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            cpu_count: None,
            memory_bytes: None,
            hostname_hash: None,
        });
        let clock = TestClock::new("2024-01-01T00:00:00Z");
        let usecase = CheckUseCase::new(runner, host_probe, clock);

        let outcome = usecase
            .execute(make_check_request(
                config,
                Some(baseline),
                HostMismatchPolicy::Warn,
                true,
            ))
            .expect("check should succeed");

        assert!(outcome.failed);
        assert_eq!(outcome.exit_code, 3);
    }

    #[test]
    fn execute_require_baseline_without_baseline_returns_error() {
        let bench = BenchConfigFile {
            name: "bench".to_string(),
            cwd: None,
            work: None,
            timeout: None,
            command: vec!["echo".to_string(), "ok".to_string()],
            repeat: Some(1),
            warmup: Some(0),
            metrics: None,
            budgets: None,
        };
        let config = ConfigFile {
            defaults: DefaultsConfig::default(),
            baseline_server: BaselineServerConfig::default(),
            benches: vec![bench],
        };

        let runner = TestRunner::new(vec![run_result(100, 0, false)]);
        let host_probe = TestHostProbe::new(HostInfo {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            cpu_count: None,
            memory_bytes: None,
            hostname_hash: None,
        });
        let clock = TestClock::new("2024-01-01T00:00:00Z");
        let usecase = CheckUseCase::new(runner, host_probe, clock);

        let mut req = make_check_request(config, None, HostMismatchPolicy::Warn, false);
        req.require_baseline = true;

        let err = usecase.execute(req).unwrap_err();
        assert!(
            err.to_string().contains("baseline not found"),
            "expected baseline not found error, got: {}",
            err
        );
    }

    #[test]
    fn execute_bench_not_found_returns_error() {
        let config = ConfigFile {
            defaults: DefaultsConfig::default(),
            baseline_server: BaselineServerConfig::default(),
            benches: vec![],
        };

        let runner = TestRunner::new(vec![]);
        let host_probe = TestHostProbe::new(HostInfo {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            cpu_count: None,
            memory_bytes: None,
            hostname_hash: None,
        });
        let clock = TestClock::new("2024-01-01T00:00:00Z");
        let usecase = CheckUseCase::new(runner, host_probe, clock);

        let req = make_check_request(config, None, HostMismatchPolicy::Warn, false);
        let err = usecase.execute(req).unwrap_err();
        assert!(
            err.to_string().contains("not found"),
            "expected bench not found error, got: {}",
            err
        );
    }

    #[test]
    fn execute_with_baseline_pass_produces_exit_0() {
        let bench = BenchConfigFile {
            name: "bench".to_string(),
            cwd: None,
            work: None,
            timeout: None,
            command: vec!["echo".to_string(), "ok".to_string()],
            repeat: Some(1),
            warmup: Some(0),
            metrics: None,
            budgets: None,
        };
        let config = ConfigFile {
            defaults: DefaultsConfig {
                repeat: None,
                warmup: None,
                threshold: Some(0.5),
                warn_factor: Some(0.9),
                out_dir: None,
                baseline_dir: None,
                baseline_pattern: None,
                markdown_template: None,
            },
            baseline_server: BaselineServerConfig::default(),
            benches: vec![bench],
        };

        let baseline = make_baseline_receipt(
            100,
            HostInfo {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                cpu_count: None,
                memory_bytes: None,
                hostname_hash: None,
            },
            None,
        );

        // Current is same as baseline → pass
        let runner = TestRunner::new(vec![run_result(100, 0, false)]);
        let host_probe = TestHostProbe::new(HostInfo {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            cpu_count: None,
            memory_bytes: None,
            hostname_hash: None,
        });
        let clock = TestClock::new("2024-01-01T00:00:00Z");
        let usecase = CheckUseCase::new(runner, host_probe, clock);

        let outcome = usecase
            .execute(make_check_request(
                config,
                Some(baseline),
                HostMismatchPolicy::Warn,
                false,
            ))
            .expect("check should succeed");

        assert!(outcome.compare_receipt.is_some());
        assert!(!outcome.failed);
        assert_eq!(outcome.exit_code, 0);
        assert_eq!(
            outcome.compare_receipt.as_ref().unwrap().verdict.status,
            VerdictStatus::Pass
        );
    }
}
