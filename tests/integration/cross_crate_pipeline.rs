//! Cross-crate integration tests for perfgate pipeline boundaries.
//!
//! These tests verify data flows correctly across crate boundaries:
//! - types → domain: RunReceipt construction through domain comparison
//! - domain → app: Comparison results through app layer and markdown rendering
//! - types serialization roundtrip: Full receipts through serialize/deserialize
//! - config → check: Config parsing through validation and budget resolution

use perfgate_app::{CompareRequest, CompareUseCase};
use perfgate_domain::{compare_runs, compute_stats, derive_report};
use perfgate_render::render_markdown;
use perfgate_types::{
    BenchMeta, Budget, COMPARE_SCHEMA_V1, CompareReceipt, CompareRef, ConfigFile, Direction,
    HostInfo, HostMismatchPolicy, Metric, MetricStatistic, MetricStatus, PAIRED_SCHEMA_V1,
    PairedBenchMeta, PairedDiffSummary, PairedRunReceipt, PairedSample, PairedSampleHalf,
    PairedStats, RUN_SCHEMA_V1, RunMeta, RunReceipt, Sample, ToolInfo, U64Summary, VerdictStatus,
};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn sample(wall_ms: u64) -> Sample {
    Sample {
        wall_ms,
        exit_code: 0,
        warmup: false,
        timed_out: false,
        cpu_ms: Some(wall_ms / 2),
        page_faults: None,
        ctx_switches: None,
        max_rss_kb: Some(1024),
        binary_bytes: None,
        stdout: None,
        stderr: None,
    }
}

fn run_receipt(name: &str, samples: Vec<Sample>) -> RunReceipt {
    let stats = compute_stats(&samples, None).unwrap();
    RunReceipt {
        schema: RUN_SCHEMA_V1.to_string(),
        tool: ToolInfo {
            name: "perfgate".to_string(),
            version: "0.3.0".to_string(),
        },
        run: RunMeta {
            id: format!("{name}-run"),
            started_at: "2024-06-01T00:00:00Z".to_string(),
            ended_at: "2024-06-01T00:01:00Z".to_string(),
            host: HostInfo {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                cpu_count: Some(8),
                memory_bytes: Some(16 * 1024 * 1024 * 1024),
                hostname_hash: None,
            },
        },
        bench: BenchMeta {
            name: name.to_string(),
            cwd: None,
            command: vec!["echo".to_string(), "hello".to_string()],
            repeat: samples.len() as u32,
            warmup: 0,
            work_units: None,
            timeout_ms: None,
        },
        samples,
        stats,
    }
}

fn default_budgets() -> BTreeMap<Metric, Budget> {
    let mut m = BTreeMap::new();
    m.insert(
        Metric::WallMs,
        Budget {
            threshold: 0.20,
            warn_threshold: 0.10,
            direction: Direction::Lower,
        },
    );
    m
}

// ===========================================================================
// 1. types → domain pipeline
// ===========================================================================

#[test]
fn types_to_domain_run_receipt_through_compare_runs() {
    let baseline = run_receipt(
        "cross-crate",
        vec![100, 102, 98, 101, 99]
            .into_iter()
            .map(sample)
            .collect(),
    );
    let current = run_receipt(
        "cross-crate",
        vec![130, 132, 128, 131, 129]
            .into_iter()
            .map(sample)
            .collect(),
    );

    let comparison = compare_runs(
        &baseline,
        &current,
        &default_budgets(),
        &BTreeMap::new(),
        None,
    )
    .unwrap();

    // ~30% regression should fail the 20% threshold
    assert_eq!(comparison.verdict.status, VerdictStatus::Fail);
    let delta = comparison.deltas.get(&Metric::WallMs).unwrap();
    assert_eq!(delta.status, MetricStatus::Fail);
    assert!((delta.baseline - 100.0).abs() < f64::EPSILON);
    assert!((delta.current - 130.0).abs() < f64::EPSILON);
}

#[test]
fn types_to_domain_all_optional_metrics_preserved() {
    let full_sample = |wall: u64| Sample {
        wall_ms: wall,
        exit_code: 0,
        warmup: false,
        timed_out: false,
        cpu_ms: Some(wall / 2),
        page_faults: Some(10),
        ctx_switches: Some(5),
        max_rss_kb: Some(2048),
        binary_bytes: Some(4096),
        stdout: None,
        stderr: None,
    };

    let baseline = run_receipt("full-metrics", vec![full_sample(100), full_sample(100)]);
    let current = run_receipt("full-metrics", vec![full_sample(150), full_sample(150)]);

    // Budget every available metric
    let mut budgets = BTreeMap::new();
    for metric in [
        Metric::WallMs,
        Metric::CpuMs,
        Metric::MaxRssKb,
        Metric::PageFaults,
        Metric::CtxSwitches,
        Metric::BinaryBytes,
    ] {
        budgets.insert(
            metric,
            Budget {
                threshold: 0.20,
                warn_threshold: 0.10,
                direction: Direction::Lower,
            },
        );
    }

    let comparison = compare_runs(&baseline, &current, &budgets, &BTreeMap::new(), None).unwrap();

    // Every metric should produce a delta (none lost at crate boundary)
    assert_eq!(comparison.deltas.len(), 6);
    for metric in [
        Metric::WallMs,
        Metric::CpuMs,
        Metric::MaxRssKb,
        Metric::PageFaults,
        Metric::CtxSwitches,
        Metric::BinaryBytes,
    ] {
        assert!(
            comparison.deltas.contains_key(&metric),
            "missing delta for {metric:?}"
        );
    }
}

#[test]
fn types_to_domain_metric_statistic_override_flows() {
    let baseline = run_receipt("stat-override", vec![sample(100), sample(200)]);
    let current = run_receipt("stat-override", vec![sample(100), sample(200)]);

    let mut metric_statistics = BTreeMap::new();
    metric_statistics.insert(Metric::WallMs, MetricStatistic::P95);

    let comparison = compare_runs(
        &baseline,
        &current,
        &default_budgets(),
        &metric_statistics,
        None,
    )
    .unwrap();

    let delta = comparison.deltas.get(&Metric::WallMs).unwrap();
    assert_eq!(delta.statistic, MetricStatistic::P95);
    assert_eq!(delta.status, MetricStatus::Pass);
}

// ===========================================================================
// 2. domain → app pipeline
// ===========================================================================

#[test]
fn domain_to_app_compare_use_case_then_markdown() {
    let baseline = run_receipt(
        "app-pipeline",
        vec![100, 100, 100].into_iter().map(sample).collect(),
    );
    let current = run_receipt(
        "app-pipeline",
        vec![115, 115, 115].into_iter().map(sample).collect(),
    );

    // App layer: CompareUseCase produces a CompareReceipt
    let result = CompareUseCase::execute(CompareRequest {
        baseline,
        current,
        budgets: default_budgets(),
        metric_statistics: BTreeMap::new(),
        significance: None,
        baseline_ref: CompareRef {
            path: Some("baseline.json".to_string()),
            run_id: None,
        },
        current_ref: CompareRef {
            path: Some("current.json".to_string()),
            run_id: None,
        },
        tool: ToolInfo {
            name: "perfgate".to_string(),
            version: "0.3.0".to_string(),
        },
        host_mismatch_policy: HostMismatchPolicy::Warn,
    })
    .unwrap();

    // Verify receipt was produced
    assert_eq!(result.receipt.schema, COMPARE_SCHEMA_V1);
    assert_eq!(result.receipt.verdict.status, VerdictStatus::Warn);

    // Render layer: markdown from the receipt
    let md = render_markdown(&result.receipt);
    assert!(md.contains("app-pipeline"), "bench name in markdown");
    assert!(md.contains("wall_ms"), "metric name in markdown");
    assert!(md.contains("⚠️"), "warn icon in markdown");
}

#[test]
fn domain_derive_report_produces_findings_for_failures() {
    let baseline = run_receipt(
        "report-test",
        vec![100, 100, 100].into_iter().map(sample).collect(),
    );
    let current = run_receipt(
        "report-test",
        vec![130, 130, 130].into_iter().map(sample).collect(),
    );

    let result = CompareUseCase::execute(CompareRequest {
        baseline,
        current,
        budgets: default_budgets(),
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
            version: "0.3.0".to_string(),
        },
        host_mismatch_policy: HostMismatchPolicy::Ignore,
    })
    .unwrap();

    // Domain: derive_report extracts findings from the CompareReceipt
    let report = derive_report(&result.receipt);
    assert_eq!(report.verdict, VerdictStatus::Fail);
    assert_eq!(report.findings.len(), 1);

    let finding = &report.findings[0];
    assert_eq!(finding.check_id, "perf.budget");
    assert_eq!(finding.code, "metric_fail");
    assert_eq!(finding.data.metric_name, "wall_ms");
    assert!((finding.data.regression_pct - 0.30).abs() < 1e-6);
}

#[test]
fn domain_to_app_pass_produces_no_findings() {
    let baseline = run_receipt(
        "pass-findings",
        vec![100, 100, 100].into_iter().map(sample).collect(),
    );
    let current = run_receipt(
        "pass-findings",
        vec![101, 101, 101].into_iter().map(sample).collect(),
    );

    let result = CompareUseCase::execute(CompareRequest {
        baseline,
        current,
        budgets: default_budgets(),
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
            version: "0.3.0".to_string(),
        },
        host_mismatch_policy: HostMismatchPolicy::Ignore,
    })
    .unwrap();

    let report = derive_report(&result.receipt);
    assert_eq!(report.verdict, VerdictStatus::Pass);
    assert!(report.findings.is_empty());
}

// ===========================================================================
// 3. types serialization roundtrip
// ===========================================================================

#[test]
fn run_receipt_json_roundtrip() {
    let receipt = run_receipt(
        "serde-test",
        vec![100, 102, 98, 101, 99]
            .into_iter()
            .map(sample)
            .collect(),
    );

    let json = serde_json::to_string_pretty(&receipt).unwrap();
    let deserialized: RunReceipt = serde_json::from_str(&json).unwrap();

    assert_eq!(receipt.schema, deserialized.schema);
    assert_eq!(receipt.bench.name, deserialized.bench.name);
    assert_eq!(receipt.samples.len(), deserialized.samples.len());
    assert_eq!(
        receipt.stats.wall_ms.median,
        deserialized.stats.wall_ms.median
    );
    assert_eq!(receipt.run.host.os, deserialized.run.host.os);
    assert_eq!(receipt.run.host.cpu_count, deserialized.run.host.cpu_count);
    // Optional metric stats survive
    assert_eq!(receipt.stats.cpu_ms, deserialized.stats.cpu_ms);
    assert_eq!(receipt.stats.max_rss_kb, deserialized.stats.max_rss_kb);
}

#[test]
fn compare_receipt_json_roundtrip() {
    let baseline = run_receipt(
        "roundtrip",
        vec![100, 100, 100].into_iter().map(sample).collect(),
    );
    let current = run_receipt(
        "roundtrip",
        vec![115, 115, 115].into_iter().map(sample).collect(),
    );

    let result = CompareUseCase::execute(CompareRequest {
        baseline,
        current,
        budgets: default_budgets(),
        metric_statistics: BTreeMap::new(),
        significance: None,
        baseline_ref: CompareRef {
            path: Some("base.json".to_string()),
            run_id: None,
        },
        current_ref: CompareRef {
            path: Some("cur.json".to_string()),
            run_id: None,
        },
        tool: ToolInfo {
            name: "perfgate".to_string(),
            version: "0.3.0".to_string(),
        },
        host_mismatch_policy: HostMismatchPolicy::Ignore,
    })
    .unwrap();

    let json = serde_json::to_string_pretty(&result.receipt).unwrap();
    let deserialized: CompareReceipt = serde_json::from_str(&json).unwrap();

    assert_eq!(result.receipt.schema, deserialized.schema);
    assert_eq!(result.receipt.verdict.status, deserialized.verdict.status);
    assert_eq!(result.receipt.deltas.len(), deserialized.deltas.len());

    let orig_delta = result.receipt.deltas.get(&Metric::WallMs).unwrap();
    let deser_delta = deserialized.deltas.get(&Metric::WallMs).unwrap();
    assert_eq!(orig_delta.status, deser_delta.status);
    assert!((orig_delta.baseline - deser_delta.baseline).abs() < f64::EPSILON);
    assert!((orig_delta.current - deser_delta.current).abs() < f64::EPSILON);
    assert!((orig_delta.regression - deser_delta.regression).abs() < f64::EPSILON);
}

#[test]
fn paired_receipt_json_roundtrip() {
    let receipt = PairedRunReceipt {
        schema: PAIRED_SCHEMA_V1.to_string(),
        tool: ToolInfo {
            name: "perfgate".to_string(),
            version: "0.3.0".to_string(),
        },
        run: RunMeta {
            id: "paired-run".to_string(),
            started_at: "2024-06-01T00:00:00Z".to_string(),
            ended_at: "2024-06-01T00:01:00Z".to_string(),
            host: HostInfo {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                cpu_count: Some(8),
                memory_bytes: Some(16 * 1024 * 1024 * 1024),
                hostname_hash: Some("abc123".to_string()),
            },
        },
        bench: PairedBenchMeta {
            name: "paired-bench".to_string(),
            cwd: None,
            baseline_command: vec!["echo".to_string(), "baseline".to_string()],
            current_command: vec!["echo".to_string(), "current".to_string()],
            repeat: 3,
            warmup: 0,
            work_units: None,
            timeout_ms: None,
        },
        samples: vec![
            PairedSample {
                pair_index: 0,
                warmup: false,
                baseline: PairedSampleHalf {
                    wall_ms: 100,
                    exit_code: 0,
                    timed_out: false,
                    max_rss_kb: Some(1024),
                    stdout: None,
                    stderr: None,
                },
                current: PairedSampleHalf {
                    wall_ms: 90,
                    exit_code: 0,
                    timed_out: false,
                    max_rss_kb: Some(1024),
                    stdout: None,
                    stderr: None,
                },
                wall_diff_ms: -10,
                rss_diff_kb: Some(0),
            },
            PairedSample {
                pair_index: 1,
                warmup: false,
                baseline: PairedSampleHalf {
                    wall_ms: 102,
                    exit_code: 0,
                    timed_out: false,
                    max_rss_kb: Some(1028),
                    stdout: None,
                    stderr: None,
                },
                current: PairedSampleHalf {
                    wall_ms: 95,
                    exit_code: 0,
                    timed_out: false,
                    max_rss_kb: Some(1020),
                    stdout: None,
                    stderr: None,
                },
                wall_diff_ms: -7,
                rss_diff_kb: Some(-8),
            },
        ],
        stats: PairedStats {
            baseline_wall_ms: U64Summary {
                median: 101,
                min: 100,
                max: 102,
            },
            current_wall_ms: U64Summary {
                median: 92,
                min: 90,
                max: 95,
            },
            wall_diff_ms: PairedDiffSummary {
                mean: -8.5,
                median: -8.5,
                std_dev: 2.12,
                min: -10.0,
                max: -7.0,
                count: 2,
            },
            baseline_max_rss_kb: Some(U64Summary {
                median: 1026,
                min: 1024,
                max: 1028,
            }),
            current_max_rss_kb: Some(U64Summary {
                median: 1022,
                min: 1020,
                max: 1024,
            }),
            rss_diff_kb: Some(PairedDiffSummary {
                mean: -4.0,
                median: -4.0,
                std_dev: 5.66,
                min: -8.0,
                max: 0.0,
                count: 2,
            }),
            baseline_throughput_per_s: None,
            current_throughput_per_s: None,
            throughput_diff_per_s: None,
        },
    };

    let json = serde_json::to_string_pretty(&receipt).unwrap();
    let deserialized: PairedRunReceipt = serde_json::from_str(&json).unwrap();

    assert_eq!(receipt.schema, deserialized.schema);
    assert_eq!(receipt.bench.name, deserialized.bench.name);
    assert_eq!(receipt.samples.len(), deserialized.samples.len());
    assert_eq!(
        receipt.stats.wall_diff_ms.mean,
        deserialized.stats.wall_diff_ms.mean
    );
    assert_eq!(
        receipt.stats.baseline_max_rss_kb,
        deserialized.stats.baseline_max_rss_kb
    );
    assert_eq!(receipt.stats.rss_diff_kb, deserialized.stats.rss_diff_kb);
    assert_eq!(
        receipt.run.host.hostname_hash,
        deserialized.run.host.hostname_hash
    );
}

#[test]
fn config_file_toml_roundtrip() {
    let toml_input = r#"
[defaults]
repeat = 5
warmup = 1
threshold = 0.15
warn_factor = 0.5

[[bench]]
name = "my-bench"
command = ["echo", "hello"]
repeat = 10
metrics = ["wall_ms", "max_rss_kb"]

[bench.budgets.wall_ms]
threshold = 0.10
direction = "lower"

[bench.budgets.max_rss_kb]
threshold = 0.30
"#;

    let config: ConfigFile = toml::from_str(toml_input).unwrap();

    // Re-serialize and deserialize
    let toml_output = toml::to_string_pretty(&config).unwrap();
    let roundtripped: ConfigFile = toml::from_str(&toml_output).unwrap();

    assert_eq!(config.defaults.repeat, roundtripped.defaults.repeat);
    assert_eq!(config.defaults.threshold, roundtripped.defaults.threshold);
    assert_eq!(
        config.defaults.warn_factor,
        roundtripped.defaults.warn_factor
    );
    assert_eq!(config.benches.len(), roundtripped.benches.len());
    assert_eq!(config.benches[0].name, roundtripped.benches[0].name);
    assert_eq!(config.benches[0].repeat, roundtripped.benches[0].repeat);

    let orig_budgets = config.benches[0].budgets.as_ref().unwrap();
    let rt_budgets = roundtripped.benches[0].budgets.as_ref().unwrap();
    assert_eq!(orig_budgets.len(), rt_budgets.len());
    assert_eq!(
        orig_budgets.get(&Metric::WallMs).unwrap().threshold,
        rt_budgets.get(&Metric::WallMs).unwrap().threshold
    );
}

#[test]
fn full_receipt_pipeline_roundtrip() {
    // Build RunReceipt → serialize → deserialize → feed to domain → serialize compare
    let baseline_samples: Vec<Sample> = vec![100, 102, 98].into_iter().map(sample).collect();
    let baseline = run_receipt("roundtrip-pipeline", baseline_samples);

    let json = serde_json::to_string(&baseline).unwrap();
    let deserialized_baseline: RunReceipt = serde_json::from_str(&json).unwrap();

    // Use deserialized receipt in domain comparison
    let current = run_receipt(
        "roundtrip-pipeline",
        vec![120, 122, 118].into_iter().map(sample).collect(),
    );

    let comparison = compare_runs(
        &deserialized_baseline,
        &current,
        &default_budgets(),
        &BTreeMap::new(),
        None,
    )
    .unwrap();

    assert_eq!(comparison.verdict.status, VerdictStatus::Warn);

    // The delta should use the deserialized baseline's median correctly
    let delta = comparison.deltas.get(&Metric::WallMs).unwrap();
    assert!((delta.baseline - 100.0).abs() < f64::EPSILON);
}

// ===========================================================================
// 4. config → check pipeline
// ===========================================================================

#[test]
fn config_parse_validate_resolve_budgets() {
    let toml_str = r#"
[defaults]
threshold = 0.15
warn_factor = 0.5

[[bench]]
name = "my-bench"
command = ["echo", "hello"]
repeat = 5
metrics = ["wall_ms", "max_rss_kb"]

[bench.budgets.wall_ms]
threshold = 0.10
direction = "lower"
"#;

    // Parse config from TOML
    let config: ConfigFile = toml::from_str(toml_str).unwrap();

    // Validate config
    assert!(config.validate().is_ok());

    // Resolve budgets: merge defaults with per-bench overrides
    let bench = &config.benches[0];
    let global_threshold = config.defaults.threshold.unwrap_or(0.20);
    let global_warn_factor = config.defaults.warn_factor.unwrap_or(0.5);

    let mut budgets = BTreeMap::new();
    if let Some(ref overrides) = bench.budgets {
        for (metric, ovr) in overrides {
            let threshold = ovr.threshold.unwrap_or(global_threshold);
            let warn_factor = ovr.warn_factor.unwrap_or(global_warn_factor);
            let direction = ovr.direction.unwrap_or(Direction::Lower);
            budgets.insert(
                *metric,
                Budget {
                    threshold,
                    warn_threshold: threshold * warn_factor,
                    direction,
                },
            );
        }
    }

    // wall_ms: bench override threshold=0.10, warn = 0.10 * 0.5 = 0.05
    let wall_budget = budgets.get(&Metric::WallMs).unwrap();
    assert!((wall_budget.threshold - 0.10).abs() < f64::EPSILON);
    assert!((wall_budget.warn_threshold - 0.05).abs() < f64::EPSILON);
    assert_eq!(wall_budget.direction, Direction::Lower);

    // Use resolved budgets with domain comparison
    let baseline = run_receipt(
        "my-bench",
        vec![100, 100, 100].into_iter().map(sample).collect(),
    );
    let current = run_receipt(
        "my-bench",
        vec![108, 108, 108].into_iter().map(sample).collect(),
    );

    let comparison = compare_runs(&baseline, &current, &budgets, &BTreeMap::new(), None).unwrap();

    // 8% regression: above 5% warn but below 10% fail → warn
    assert_eq!(comparison.verdict.status, VerdictStatus::Warn);
}

#[test]
fn config_defaults_apply_when_no_bench_override() {
    let toml_str = r#"
[defaults]
threshold = 0.20
warn_factor = 0.5

[[bench]]
name = "simple-bench"
command = ["echo"]
"#;

    let config: ConfigFile = toml::from_str(toml_str).unwrap();
    assert!(config.validate().is_ok());

    let global_threshold = config.defaults.threshold.unwrap();
    let global_warn_factor = config.defaults.warn_factor.unwrap();

    // No per-bench budgets: use defaults for wall_ms
    let mut budgets = BTreeMap::new();
    budgets.insert(
        Metric::WallMs,
        Budget {
            threshold: global_threshold,
            warn_threshold: global_threshold * global_warn_factor,
            direction: Direction::Lower,
        },
    );

    assert!((budgets[&Metric::WallMs].threshold - 0.20).abs() < f64::EPSILON);
    assert!((budgets[&Metric::WallMs].warn_threshold - 0.10).abs() < f64::EPSILON);

    // Verify these budgets work through domain
    let baseline = run_receipt(
        "simple-bench",
        vec![100, 100, 100].into_iter().map(sample).collect(),
    );
    let current = run_receipt(
        "simple-bench",
        vec![115, 115, 115].into_iter().map(sample).collect(),
    );

    let comparison = compare_runs(&baseline, &current, &budgets, &BTreeMap::new(), None).unwrap();

    // 15% regression: above 10% warn but below 20% fail → warn
    assert_eq!(comparison.verdict.status, VerdictStatus::Warn);
}

#[test]
fn config_invalid_bench_name_stops_pipeline() {
    let toml_str = r#"
[[bench]]
name = "../evil-bench"
command = ["echo"]
"#;

    let config: ConfigFile = toml::from_str(toml_str).unwrap();
    assert!(config.validate().is_err());
}

#[test]
fn config_multiple_benches_each_with_budgets_to_domain() {
    let toml_str = r#"
[defaults]
threshold = 0.20
warn_factor = 0.5

[[bench]]
name = "bench-a"
command = ["echo", "a"]

[bench.budgets.wall_ms]
threshold = 0.05

[[bench]]
name = "bench-b"
command = ["echo", "b"]

[bench.budgets.wall_ms]
threshold = 0.30
"#;

    let config: ConfigFile = toml::from_str(toml_str).unwrap();
    assert!(config.validate().is_ok());
    assert_eq!(config.benches.len(), 2);

    // Resolve budgets for bench-a (tight threshold)
    let bench_a = &config.benches[0];
    let ovr_a = bench_a
        .budgets
        .as_ref()
        .unwrap()
        .get(&Metric::WallMs)
        .unwrap();
    let threshold_a = ovr_a.threshold.unwrap();
    assert!((threshold_a - 0.05).abs() < f64::EPSILON);

    // Resolve budgets for bench-b (loose threshold)
    let bench_b = &config.benches[1];
    let ovr_b = bench_b
        .budgets
        .as_ref()
        .unwrap()
        .get(&Metric::WallMs)
        .unwrap();
    let threshold_b = ovr_b.threshold.unwrap();
    assert!((threshold_b - 0.30).abs() < f64::EPSILON);

    // Same 10% regression: bench-a fails, bench-b passes
    let baseline = run_receipt(
        "bench-a",
        vec![100, 100, 100].into_iter().map(sample).collect(),
    );
    let current = run_receipt(
        "bench-a",
        vec![110, 110, 110].into_iter().map(sample).collect(),
    );

    let global_wf = config.defaults.warn_factor.unwrap_or(0.5);

    let mut budgets_a = BTreeMap::new();
    budgets_a.insert(
        Metric::WallMs,
        Budget {
            threshold: threshold_a,
            warn_threshold: threshold_a * global_wf,
            direction: Direction::Lower,
        },
    );

    let cmp_a = compare_runs(&baseline, &current, &budgets_a, &BTreeMap::new(), None).unwrap();
    assert_eq!(cmp_a.verdict.status, VerdictStatus::Fail);

    let mut budgets_b = BTreeMap::new();
    budgets_b.insert(
        Metric::WallMs,
        Budget {
            threshold: threshold_b,
            warn_threshold: threshold_b * global_wf,
            direction: Direction::Lower,
        },
    );

    let cmp_b = compare_runs(&baseline, &current, &budgets_b, &BTreeMap::new(), None).unwrap();
    assert_eq!(cmp_b.verdict.status, VerdictStatus::Pass);
}
