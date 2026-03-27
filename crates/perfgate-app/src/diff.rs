//! DiffUseCase — git-aware zero-argument comparison.
//!
//! This module implements the `diff` workflow which:
//! 1. Auto-discovers `perfgate.toml` by walking up from cwd
//! 2. Determines which benchmarks to run (all, or filtered by name)
//! 3. Finds the baseline for each benchmark
//! 4. Runs each benchmark
//! 5. Compares against baseline
//! 6. Returns structured diff outcomes for terminal rendering

use crate::{Clock, CompareRequest, CompareUseCase, RunBenchRequest, RunBenchUseCase};
use anyhow::Context;
use perfgate_adapters::{HostProbe, ProcessRunner};
use perfgate_types::{
    BenchConfigFile, CompareReceipt, CompareRef, ConfigFile, HostMismatchPolicy, Metric,
    MetricStatistic, RunReceipt, ToolInfo, VerdictStatus,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Request for the diff use case.
#[derive(Debug, Clone)]
pub struct DiffRequest {
    /// Path to config file. If `None`, auto-discover by walking up from cwd.
    pub config_path: Option<PathBuf>,

    /// Filter to a single benchmark by name.
    pub bench_filter: Option<String>,

    /// Git ref to compare against (reserved for future use).
    pub against: Option<String>,

    /// If true, reduce repeat count for faster feedback.
    pub quick: bool,

    /// If true, produce JSON output instead of terminal rendering.
    pub json: bool,

    /// Tool info for receipts.
    pub tool: ToolInfo,
}

/// Outcome for a single benchmark diff.
#[derive(Debug, Clone)]
pub struct BenchDiffOutcome {
    /// Name of the benchmark.
    pub bench_name: String,

    /// The run receipt produced.
    pub run_receipt: RunReceipt,

    /// The compare receipt (None if no baseline was found).
    pub compare_receipt: Option<CompareReceipt>,

    /// Path to the baseline that was used (if any).
    pub baseline_path: Option<PathBuf>,

    /// True if no baseline was found.
    pub no_baseline: bool,
}

/// Overall outcome of the diff command.
#[derive(Debug, Clone)]
pub struct DiffOutcome {
    /// Path to the config file that was used.
    pub config_path: PathBuf,

    /// Per-benchmark outcomes.
    pub bench_outcomes: Vec<BenchDiffOutcome>,

    /// Overall exit code (0=pass, 2=fail).
    pub exit_code: i32,
}

impl DiffOutcome {
    /// Returns the worst verdict status across all benchmarks.
    pub fn worst_verdict(&self) -> VerdictStatus {
        let mut worst = VerdictStatus::Pass;
        for outcome in &self.bench_outcomes {
            if let Some(compare) = &outcome.compare_receipt {
                match compare.verdict.status {
                    VerdictStatus::Fail => return VerdictStatus::Fail,
                    VerdictStatus::Warn => worst = VerdictStatus::Warn,
                    VerdictStatus::Skip if worst == VerdictStatus::Pass => {
                        worst = VerdictStatus::Skip;
                    }
                    _ => {}
                }
            }
        }
        worst
    }
}

/// Walk up from `start` looking for `perfgate.toml` or `perfgate.json`.
pub fn discover_config(start: &Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        let toml_path = dir.join("perfgate.toml");
        if toml_path.is_file() {
            return Some(toml_path);
        }
        let json_path = dir.join("perfgate.json");
        if json_path.is_file() {
            return Some(json_path);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Use case for running a diff workflow.
pub struct DiffUseCase<R: ProcessRunner + Clone, H: HostProbe + Clone, C: Clock + Clone> {
    runner: R,
    host_probe: H,
    clock: C,
}

impl<R: ProcessRunner + Clone, H: HostProbe + Clone, C: Clock + Clone> DiffUseCase<R, H, C> {
    pub fn new(runner: R, host_probe: H, clock: C) -> Self {
        Self {
            runner,
            host_probe,
            clock,
        }
    }

    /// Execute the diff workflow.
    pub fn execute(&self, req: DiffRequest) -> anyhow::Result<DiffOutcome> {
        // 1. Discover config
        let config_path = match &req.config_path {
            Some(p) => p.clone(),
            None => {
                let cwd = std::env::current_dir().context("failed to get current directory")?;
                discover_config(&cwd).ok_or_else(|| {
                    anyhow::anyhow!(
                        "no perfgate.toml found (searched upward from {})",
                        cwd.display()
                    )
                })?
            }
        };

        // 2. Load config
        let config = perfgate_config::load_config_file(&config_path)
            .with_context(|| format!("failed to load config from {}", config_path.display()))?;

        config
            .validate()
            .map_err(|e| anyhow::anyhow!("config validation failed: {}", e))?;

        // 3. Determine which benchmarks to run
        let bench_names: Vec<String> = if let Some(filter) = &req.bench_filter {
            // Verify the bench exists
            if !config.benches.iter().any(|b| &b.name == filter) {
                let available: Vec<&str> = config.benches.iter().map(|b| b.name.as_str()).collect();
                anyhow::bail!(
                    "bench '{}' not found in config; available: {}",
                    filter,
                    available.join(", ")
                );
            }
            vec![filter.clone()]
        } else {
            if config.benches.is_empty() {
                anyhow::bail!("no benchmarks defined in {}", config_path.display());
            }
            config.benches.iter().map(|b| b.name.clone()).collect()
        };

        // 4. Run each benchmark and compare
        let mut bench_outcomes = Vec::new();
        let mut max_exit_code: i32 = 0;

        for bench_name in &bench_names {
            let bench_config = config
                .benches
                .iter()
                .find(|b| &b.name == bench_name)
                .unwrap();

            let outcome = self.run_single_bench(bench_config, &config, &req)?;

            // Update exit code
            if let Some(compare) = &outcome.compare_receipt {
                match compare.verdict.status {
                    VerdictStatus::Fail => {
                        if max_exit_code < 2 {
                            max_exit_code = 2;
                        }
                    }
                    VerdictStatus::Warn | VerdictStatus::Pass | VerdictStatus::Skip => {}
                }
            }

            bench_outcomes.push(outcome);
        }

        Ok(DiffOutcome {
            config_path,
            bench_outcomes,
            exit_code: max_exit_code,
        })
    }

    fn run_single_bench(
        &self,
        bench: &BenchConfigFile,
        config: &ConfigFile,
        req: &DiffRequest,
    ) -> anyhow::Result<BenchDiffOutcome> {
        let defaults = &config.defaults;

        // Build run request
        let mut repeat = bench.repeat.or(defaults.repeat).unwrap_or(5);
        let warmup = bench.warmup.or(defaults.warmup).unwrap_or(0);

        // In quick mode, reduce repeat count
        if req.quick {
            repeat = repeat.clamp(1, 2);
        }

        let timeout = bench
            .timeout
            .as_deref()
            .map(|s| {
                humantime::parse_duration(s)
                    .with_context(|| format!("invalid timeout '{}' for bench '{}'", s, bench.name))
            })
            .transpose()?;

        let cwd = bench.cwd.as_ref().map(PathBuf::from);

        let run_request = RunBenchRequest {
            name: bench.name.clone(),
            cwd,
            command: bench.command.clone(),
            repeat,
            warmup,
            work_units: bench.work,
            timeout,
            env: Vec::new(),
            output_cap_bytes: 8192,
            allow_nonzero: false,
            include_hostname_hash: false,
        };

        // Run the benchmark
        let run_usecase = RunBenchUseCase::new(
            self.runner.clone(),
            self.host_probe.clone(),
            self.clock.clone(),
            req.tool.clone(),
        );
        let run_outcome = run_usecase.execute(run_request)?;
        let run_receipt = run_outcome.receipt;

        // Resolve baseline
        let baseline_path =
            perfgate_app_baseline_resolve::resolve_baseline_path(&None, &bench.name, config);

        let baseline_receipt = if baseline_path.is_file() {
            let content = std::fs::read_to_string(&baseline_path)
                .with_context(|| format!("read baseline {}", baseline_path.display()))?;
            Some(
                serde_json::from_str::<RunReceipt>(&content)
                    .with_context(|| format!("parse baseline {}", baseline_path.display()))?,
            )
        } else {
            None
        };

        // Compare if baseline exists
        let compare_receipt = if let Some(baseline) = &baseline_receipt {
            let (budgets, metric_statistics) =
                build_diff_budgets(bench, config, baseline, &run_receipt)?;

            let compare_req = CompareRequest {
                baseline: baseline.clone(),
                current: run_receipt.clone(),
                budgets,
                metric_statistics,
                significance: None,
                baseline_ref: CompareRef {
                    path: Some(baseline_path.display().to_string()),
                    run_id: Some(baseline.run.id.clone()),
                },
                current_ref: CompareRef {
                    path: None,
                    run_id: Some(run_receipt.run.id.clone()),
                },
                tool: req.tool.clone(),
                host_mismatch_policy: HostMismatchPolicy::Warn,
            };

            Some(CompareUseCase::execute(compare_req)?.receipt)
        } else {
            None
        };

        Ok(BenchDiffOutcome {
            bench_name: bench.name.clone(),
            run_receipt,
            compare_receipt,
            baseline_path: Some(baseline_path),
            no_baseline: baseline_receipt.is_none(),
        })
    }
}

/// Build budgets for the diff comparison (simplified from CheckUseCase).
fn build_diff_budgets(
    bench: &BenchConfigFile,
    config: &ConfigFile,
    baseline: &RunReceipt,
    current: &RunReceipt,
) -> anyhow::Result<(
    BTreeMap<Metric, perfgate_types::Budget>,
    BTreeMap<Metric, MetricStatistic>,
)> {
    let defaults = &config.defaults;
    let global_threshold = defaults.threshold.unwrap_or(0.20);
    let global_warn_factor = defaults.warn_factor.unwrap_or(0.90);

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

        let noise_threshold = override_opt
            .as_ref()
            .and_then(|o| o.noise_threshold)
            .or(defaults.noise_threshold);

        let noise_policy = override_opt
            .as_ref()
            .and_then(|o| o.noise_policy)
            .or(defaults.noise_policy)
            .unwrap_or(perfgate_types::NoisePolicy::Warn);

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
            perfgate_types::Budget {
                threshold,
                warn_threshold,
                noise_threshold,
                noise_policy,
                direction,
            },
        );

        metric_statistics.insert(metric, statistic);
    }

    Ok((budgets, metric_statistics))
}

// Module alias to avoid name collision with the crate itself
mod perfgate_app_baseline_resolve {
    pub use crate::baseline_resolve::resolve_baseline_path;
}

/// Render a terminal-friendly colored diff output from a DiffOutcome.
pub fn render_terminal_diff(outcome: &DiffOutcome) -> String {
    use crate::{format_metric_with_statistic, format_pct, format_value};

    let mut out = String::new();

    for bench_outcome in &outcome.bench_outcomes {
        out.push_str(&format!("bench: {}\n", bench_outcome.bench_name));

        if bench_outcome.no_baseline {
            out.push_str("  (no baseline found, skipping comparison)\n\n");
            continue;
        }

        if let Some(compare) = &bench_outcome.compare_receipt {
            let verdict_label = match compare.verdict.status {
                VerdictStatus::Pass => "PASS",
                VerdictStatus::Warn => "WARN",
                VerdictStatus::Fail => "FAIL",
                VerdictStatus::Skip => "SKIP",
            };
            out.push_str(&format!("  verdict: {}\n", verdict_label));

            for (metric, delta) in &compare.deltas {
                let name = format_metric_with_statistic(*metric, delta.statistic);
                let baseline_str = format_value(*metric, delta.baseline);
                let current_str = format_value(*metric, delta.current);
                let pct_str = format_pct(delta.pct);
                let unit = metric.display_unit();

                let status_indicator = match delta.status {
                    perfgate_types::MetricStatus::Pass => " ",
                    perfgate_types::MetricStatus::Warn => "~",
                    perfgate_types::MetricStatus::Fail => "!",
                    perfgate_types::MetricStatus::Skip => "-",
                };

                out.push_str(&format!(
                    "  {status_indicator} {name}: {baseline_str} {unit} -> {current_str} {unit} ({pct_str})\n"
                ));
            }
        }

        out.push('\n');
    }

    out
}

/// Render the diff outcome as JSON.
pub fn render_json_diff(outcome: &DiffOutcome) -> anyhow::Result<String> {
    let mut entries = Vec::new();

    for bench_outcome in &outcome.bench_outcomes {
        let entry = serde_json::json!({
            "bench": bench_outcome.bench_name,
            "no_baseline": bench_outcome.no_baseline,
            "compare": bench_outcome.compare_receipt,
        });
        entries.push(entry);
    }

    let output = serde_json::json!({
        "config": outcome.config_path.display().to_string(),
        "exit_code": outcome.exit_code,
        "benchmarks": entries,
    });

    serde_json::to_string_pretty(&output).context("serialize diff output")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn discover_config_finds_toml_in_current_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("perfgate.toml");
        std::fs::write(&config_path, "[defaults]\n").unwrap();

        let found = discover_config(tmp.path());
        assert_eq!(found, Some(config_path));
    }

    #[test]
    fn discover_config_finds_toml_in_parent() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("perfgate.toml");
        std::fs::write(&config_path, "[defaults]\n").unwrap();

        let child = tmp.path().join("subdir");
        std::fs::create_dir_all(&child).unwrap();

        let found = discover_config(&child);
        assert_eq!(found, Some(config_path));
    }

    #[test]
    fn discover_config_prefers_toml_over_json() {
        let tmp = tempfile::tempdir().unwrap();
        let toml_path = tmp.path().join("perfgate.toml");
        let json_path = tmp.path().join("perfgate.json");
        std::fs::write(&toml_path, "[defaults]\n").unwrap();
        std::fs::write(&json_path, "{}").unwrap();

        let found = discover_config(tmp.path());
        assert_eq!(found, Some(toml_path));
    }

    #[test]
    fn discover_config_falls_back_to_json() {
        let tmp = tempfile::tempdir().unwrap();
        let json_path = tmp.path().join("perfgate.json");
        std::fs::write(&json_path, "{}").unwrap();

        let found = discover_config(tmp.path());
        assert_eq!(found, Some(json_path));
    }

    #[test]
    fn discover_config_returns_none_when_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let found = discover_config(tmp.path());
        assert!(found.is_none());
    }

    #[test]
    fn render_terminal_diff_no_baseline() {
        let outcome = DiffOutcome {
            config_path: PathBuf::from("perfgate.toml"),
            bench_outcomes: vec![BenchDiffOutcome {
                bench_name: "my-bench".to_string(),
                run_receipt: make_dummy_run_receipt(),
                compare_receipt: None,
                baseline_path: None,
                no_baseline: true,
            }],
            exit_code: 0,
        };

        let rendered = render_terminal_diff(&outcome);
        assert!(rendered.contains("my-bench"));
        assert!(rendered.contains("no baseline found"));
    }

    #[test]
    fn render_terminal_diff_with_comparison() {
        use perfgate_types::*;

        let mut deltas = BTreeMap::new();
        deltas.insert(
            Metric::WallMs,
            Delta {
                baseline: 100.0,
                current: 110.0,
                ratio: 1.10,
                pct: 0.10,
                regression: 0.10,
                statistic: MetricStatistic::Median,
                significance: None,
                cv: None,
                noise_threshold: None,
                status: MetricStatus::Pass,
            },
        );

        let mut budgets = BTreeMap::new();
        budgets.insert(Metric::WallMs, Budget::new(0.2, 0.18, Direction::Lower));

        let compare = CompareReceipt {
            schema: COMPARE_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".into(),
                version: "0.1.0".into(),
            },
            bench: BenchMeta {
                name: "my-bench".into(),
                cwd: None,
                command: vec!["echo".into()],
                repeat: 2,
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

        let outcome = DiffOutcome {
            config_path: PathBuf::from("perfgate.toml"),
            bench_outcomes: vec![BenchDiffOutcome {
                bench_name: "my-bench".to_string(),
                run_receipt: make_dummy_run_receipt(),
                compare_receipt: Some(compare),
                baseline_path: Some(PathBuf::from("baselines/my-bench.json")),
                no_baseline: false,
            }],
            exit_code: 0,
        };

        let rendered = render_terminal_diff(&outcome);
        assert!(rendered.contains("my-bench"));
        assert!(rendered.contains("PASS"));
        assert!(rendered.contains("wall_ms"));
        assert!(rendered.contains("+10.00%"));
    }

    #[test]
    fn render_json_diff_produces_valid_json() {
        let outcome = DiffOutcome {
            config_path: PathBuf::from("perfgate.toml"),
            bench_outcomes: vec![BenchDiffOutcome {
                bench_name: "test".to_string(),
                run_receipt: make_dummy_run_receipt(),
                compare_receipt: None,
                baseline_path: None,
                no_baseline: true,
            }],
            exit_code: 0,
        };

        let json = render_json_diff(&outcome).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["exit_code"], 0);
        assert_eq!(parsed["benchmarks"][0]["bench"], "test");
        assert_eq!(parsed["benchmarks"][0]["no_baseline"], true);
    }

    #[test]
    fn worst_verdict_returns_fail_when_any_fail() {
        use perfgate_types::*;

        let pass_compare = make_compare_with_verdict(VerdictStatus::Pass);
        let fail_compare = make_compare_with_verdict(VerdictStatus::Fail);

        let outcome = DiffOutcome {
            config_path: PathBuf::from("perfgate.toml"),
            bench_outcomes: vec![
                BenchDiffOutcome {
                    bench_name: "a".to_string(),
                    run_receipt: make_dummy_run_receipt(),
                    compare_receipt: Some(pass_compare),
                    baseline_path: None,
                    no_baseline: false,
                },
                BenchDiffOutcome {
                    bench_name: "b".to_string(),
                    run_receipt: make_dummy_run_receipt(),
                    compare_receipt: Some(fail_compare),
                    baseline_path: None,
                    no_baseline: false,
                },
            ],
            exit_code: 2,
        };

        assert_eq!(outcome.worst_verdict(), VerdictStatus::Fail);
    }

    #[test]
    fn worst_verdict_returns_pass_when_no_comparisons() {
        let outcome = DiffOutcome {
            config_path: PathBuf::from("perfgate.toml"),
            bench_outcomes: vec![BenchDiffOutcome {
                bench_name: "a".to_string(),
                run_receipt: make_dummy_run_receipt(),
                compare_receipt: None,
                baseline_path: None,
                no_baseline: true,
            }],
            exit_code: 0,
        };

        assert_eq!(outcome.worst_verdict(), VerdictStatus::Pass);
    }

    fn make_dummy_run_receipt() -> RunReceipt {
        use perfgate_types::*;

        RunReceipt {
            schema: RUN_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".into(),
                version: "0.1.0".into(),
            },
            bench: BenchMeta {
                name: "test".into(),
                cwd: None,
                command: vec!["echo".into()],
                repeat: 2,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            run: RunMeta {
                id: "test-id".into(),
                started_at: "2024-01-01T00:00:00Z".into(),
                ended_at: "2024-01-01T00:00:01Z".into(),
                host: HostInfo {
                    os: "linux".into(),
                    arch: "x86_64".into(),
                    cpu_count: None,
                    memory_bytes: None,
                    hostname_hash: None,
                },
            },
            samples: vec![],
            stats: Stats {
                wall_ms: U64Summary::new(100, 90, 110),
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                binary_bytes: None,
                throughput_per_s: None,
                io_read_bytes: None,
                io_write_bytes: None,
                energy_uj: None,
                network_packets: None,
            },
        }
    }

    fn make_compare_with_verdict(status: VerdictStatus) -> CompareReceipt {
        use perfgate_types::*;

        CompareReceipt {
            schema: COMPARE_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".into(),
                version: "0.1.0".into(),
            },
            bench: BenchMeta {
                name: "test".into(),
                cwd: None,
                command: vec!["echo".into()],
                repeat: 2,
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
            budgets: BTreeMap::new(),
            deltas: BTreeMap::new(),
            verdict: Verdict {
                status,
                counts: VerdictCounts {
                    pass: 0,
                    warn: 0,
                    fail: 0,
                    skip: 0,
                },
                reasons: vec![],
            },
        }
    }
}
