//! Application layer for perfgate.
//!
//! The app layer coordinates adapters and domain logic.
//! It does not parse CLI flags and it does not do filesystem I/O.

use anyhow::Context;
use perfgate_adapters::{CommandSpec, ProcessRunner, RunResult};
use perfgate_domain::{compare_stats, compute_stats, Comparison};
use perfgate_types::{
    BenchMeta, Budget, CompareReceipt, CompareRef, Direction, HostInfo, Metric, MetricStatus,
    RunMeta, RunReceipt, Sample, ToolInfo,
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

#[derive(Debug, Clone)]
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
}

#[derive(Debug, Clone)]
pub struct RunBenchOutcome {
    pub receipt: RunReceipt,

    /// True if any measured (non-warmup) sample timed out or returned nonzero.
    pub failed: bool,

    /// Human-readable reasons (for CI logs).
    pub reasons: Vec<String>,
}

pub struct RunBenchUseCase<R: ProcessRunner, C: Clock> {
    runner: R,
    clock: C,
    tool: ToolInfo,
}

impl<R: ProcessRunner, C: Clock> RunBenchUseCase<R, C> {
    pub fn new(runner: R, clock: C, tool: ToolInfo) -> Self {
        Self {
            runner,
            clock,
            tool,
        }
    }

    pub fn execute(&self, req: RunBenchRequest) -> anyhow::Result<RunBenchOutcome> {
        let run_id = uuid::Uuid::new_v4().to_string();
        let started_at = self.clock.now_rfc3339();

        let host = HostInfo {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
        };

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
                argv: req.command.clone(),
                cwd: req.cwd.clone(),
                env: req.env.clone(),
                timeout: req.timeout,
                output_cap_bytes: req.output_cap_bytes,
            };

            let run = self.runner.run(&spec).with_context(|| {
                format!(
                    "failed to run command (iteration {}): {:?}",
                    i + 1,
                    spec.argv
                )
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
        max_rss_kb: run.max_rss_kb,
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
    pub baseline_ref: CompareRef,
    pub current_ref: CompareRef,
    pub tool: ToolInfo,
}

pub struct CompareUseCase;

impl CompareUseCase {
    pub fn execute(req: CompareRequest) -> anyhow::Result<CompareReceipt> {
        let Comparison { deltas, verdict } =
            compare_stats(&req.baseline.stats, &req.current.stats, &req.budgets)?;

        Ok(CompareReceipt {
            schema: perfgate_types::COMPARE_SCHEMA_V1.to_string(),
            tool: req.tool,
            bench: req.current.bench,
            baseline_ref: req.baseline_ref,
            current_ref: req.current_ref,
            budgets: req.budgets,
            deltas,
            verdict,
        })
    }
}

// ----------------------------
// Rendering helpers
// ----------------------------

pub fn render_markdown(compare: &CompareReceipt) -> String {
    let mut out = String::new();

    let header = match compare.verdict.status {
        perfgate_types::VerdictStatus::Pass => "✅ perfgate: pass",
        perfgate_types::VerdictStatus::Warn => "⚠️ perfgate: warn",
        perfgate_types::VerdictStatus::Fail => "❌ perfgate: fail",
    };

    out.push_str(header);
    out.push_str("\n\n");

    out.push_str(&format!("**Bench:** `{}`\n\n", compare.bench.name));

    out.push_str("| metric | baseline (median) | current (median) | delta | budget | status |\n");
    out.push_str("|---|---:|---:|---:|---:|---|\n");

    for (metric, delta) in &compare.deltas {
        let budget = compare.budgets.get(metric);
        let (budget_str, direction_str) = if let Some(b) = budget {
            (
                format!("{:.1}%", b.threshold * 100.0),
                match b.direction {
                    Direction::Lower => "lower",
                    Direction::Higher => "higher",
                },
            )
        } else {
            ("".to_string(), "")
        };

        let status_icon = match delta.status {
            MetricStatus::Pass => "✅",
            MetricStatus::Warn => "⚠️",
            MetricStatus::Fail => "❌",
        };

        out.push_str(&format!(
            "| `{metric}` | {b} {u} | {c} {u} | {pct} | {budget} ({dir}) | {status} |\n",
            metric = format_metric(*metric),
            b = format_value(*metric, delta.baseline),
            c = format_value(*metric, delta.current),
            u = metric.display_unit(),
            pct = format_pct(delta.pct),
            budget = budget_str,
            dir = direction_str,
            status = status_icon,
        ));
    }

    if !compare.verdict.reasons.is_empty() {
        out.push_str("\n**Notes:**\n");
        for r in &compare.verdict.reasons {
            out.push_str(&format!("- {}\n", r));
        }
    }

    out
}

pub fn github_annotations(compare: &CompareReceipt) -> Vec<String> {
    let mut lines = Vec::new();

    for (metric, delta) in &compare.deltas {
        let prefix = match delta.status {
            MetricStatus::Fail => "::error",
            MetricStatus::Warn => "::warning",
            MetricStatus::Pass => continue,
        };

        let msg = format!(
            "perfgate {bench} {metric}: {pct} (baseline {b}{u}, current {c}{u})",
            bench = compare.bench.name,
            metric = format_metric(*metric),
            pct = format_pct(delta.pct),
            b = format_value(*metric, delta.baseline),
            c = format_value(*metric, delta.current),
            u = metric.display_unit(),
        );

        lines.push(format!("{prefix}::{msg}"));
    }

    lines
}

fn format_metric(metric: Metric) -> &'static str {
    match metric {
        Metric::WallMs => "wall_ms",
        Metric::MaxRssKb => "max_rss_kb",
        Metric::ThroughputPerS => "throughput_per_s",
    }
}

fn format_value(metric: Metric, v: f64) -> String {
    match metric {
        Metric::WallMs | Metric::MaxRssKb => format!("{:.0}", v),
        Metric::ThroughputPerS => format!("{:.3}", v),
    }
}

fn format_pct(pct: f64) -> String {
    let sign = if pct > 0.0 { "+" } else { "" };
    format!("{}{:.2}%", sign, pct * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{Delta, Verdict, VerdictCounts, VerdictStatus};

    #[test]
    fn markdown_renders_table() {
        let mut budgets = BTreeMap::new();
        budgets.insert(
            Metric::WallMs,
            Budget {
                threshold: 0.2,
                warn_threshold: 0.18,
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
                },
                reasons: vec![],
            },
        };

        let md = render_markdown(&compare);
        assert!(md.contains("| metric | baseline"));
        assert!(md.contains("wall_ms"));
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use perfgate_types::{Delta, Verdict, VerdictCounts, VerdictStatus};
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
        ]
    }

    // Strategy for VerdictCounts
    fn verdict_counts_strategy() -> impl Strategy<Value = VerdictCounts> {
        (0u32..10, 0u32..10, 0u32..10).prop_map(|(pass, warn, fail)| VerdictCounts {
            pass,
            warn,
            fail,
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
            Just(Metric::WallMs),
            Just(Metric::MaxRssKb),
            Just(Metric::ThroughputPerS),
        ]
    }

    // Strategy for BTreeMap<Metric, Budget>
    fn budgets_map_strategy() -> impl Strategy<Value = BTreeMap<Metric, Budget>> {
        proptest::collection::btree_map(metric_strategy(), budget_strategy(), 0..4)
    }

    // Strategy for BTreeMap<Metric, Delta>
    fn deltas_map_strategy() -> impl Strategy<Value = BTreeMap<Metric, Delta>> {
        proptest::collection::btree_map(metric_strategy(), delta_strategy(), 0..4)
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
                let metric_name = match metric {
                    Metric::WallMs => "wall_ms",
                    Metric::MaxRssKb => "max_rss_kb",
                    Metric::ThroughputPerS => "throughput_per_s",
                };
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
                if delta.status == MetricStatus::Pass {
                    continue; // Pass metrics don't produce annotations
                }

                let metric_name = match metric {
                    Metric::WallMs => "wall_ms",
                    Metric::MaxRssKb => "max_rss_kb",
                    Metric::ThroughputPerS => "throughput_per_s",
                };

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
                    MetricStatus::Pass => unreachable!(),
                }
            }
        }
    }
}
