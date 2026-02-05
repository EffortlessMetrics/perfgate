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
    format_metric, format_pct, Clock, CompareRequest, CompareUseCase, RunBenchRequest,
    RunBenchUseCase,
};
use anyhow::{bail, Context};
use perfgate_adapters::{HostProbe, ProcessRunner};
use perfgate_types::{
    BenchConfigFile, Budget, CompareReceipt, CompareRef, ConfigFile, FindingData, Metric,
    MetricStatus, PerfgateReport, ReportFinding, ReportSummary, RunReceipt, Severity, ToolInfo,
    Verdict, VerdictCounts, VerdictStatus, CHECK_ID_BASELINE, CHECK_ID_BUDGET,
    FINDING_CODE_BASELINE_MISSING, FINDING_CODE_METRIC_FAIL, FINDING_CODE_METRIC_WARN,
    REPORT_SCHEMA_V1, VERDICT_REASON_NO_BASELINE,
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
            .with_context(|| format!("bench '{}' not found in config", req.bench_name))?;

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
            let budgets = self.build_budgets(bench_config, &req.config, baseline, &run_receipt)?;

            // Compare
            let compare_req = CompareRequest {
                baseline: baseline.clone(),
                current: run_receipt.clone(),
                budgets,
                baseline_ref: CompareRef {
                    path: req.baseline_path.as_ref().map(|p| p.display().to_string()),
                    run_id: Some(baseline.run.id.clone()),
                },
                current_ref: CompareRef {
                    path: Some(run_path.display().to_string()),
                    run_id: Some(run_receipt.run.id.clone()),
                },
                tool: req.tool.clone(),
            };

            let compare = CompareUseCase::execute(compare_req)?;

            // Build report
            let report = build_report(&compare);

            let compare_path = req.out_dir.join("compare.json");

            (Some(compare), Some(compare_path), report)
        } else {
            // No baseline
            if req.require_baseline {
                bail!(
                    "baseline required but not found for bench '{}'",
                    req.bench_name
                );
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
        let markdown = if let Some(ref compare) = compare_receipt {
            crate::render_markdown(compare)
        } else {
            render_no_baseline_markdown(&run_receipt, &warnings)
        };

        let markdown_path = req.out_dir.join("comment.md");

        // 7. Determine exit code
        let (failed, exit_code) = if let Some(ref compare) = compare_receipt {
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
    ) -> anyhow::Result<BTreeMap<Metric, Budget>> {
        let defaults = &config.defaults;

        // Global defaults
        let global_threshold = defaults.threshold.unwrap_or(0.20);
        let global_warn_factor = defaults.warn_factor.unwrap_or(0.90);

        // Determine candidate metrics: those present in both baseline+current
        let mut candidates = Vec::new();
        candidates.push(Metric::WallMs);
        if baseline.stats.max_rss_kb.is_some() && current.stats.max_rss_kb.is_some() {
            candidates.push(Metric::MaxRssKb);
        }
        if baseline.stats.throughput_per_s.is_some() && current.stats.throughput_per_s.is_some() {
            candidates.push(Metric::ThroughputPerS);
        }

        let mut budgets = BTreeMap::new();

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

            budgets.insert(
                metric,
                Budget {
                    threshold,
                    warn_threshold,
                    direction,
                },
            );
        }

        Ok(budgets)
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

    if let Some(ref rss) = run.stats.max_rss_kb {
        out.push_str(&format!("| `max_rss_kb` | {} KB |\n", rss.median));
    }

    if let Some(ref throughput) = run.stats.throughput_per_s {
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
    use perfgate_types::{
        BenchMeta, CompareReceipt, Delta, Direction, HostInfo, RunMeta, Sample, Stats, U64Summary,
        Verdict, VerdictCounts, COMPARE_SCHEMA_V1,
    };

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
                max_rss_kb: Some(1024),
                stdout: None,
                stderr: None,
            }],
            stats: Stats {
                wall_ms: U64Summary {
                    median: wall_ms_median,
                    min: wall_ms_median.saturating_sub(10),
                    max: wall_ms_median.saturating_add(10),
                },
                max_rss_kb: Some(U64Summary {
                    median: 1024,
                    min: 1000,
                    max: 1100,
                }),
                throughput_per_s: None,
            },
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
}
