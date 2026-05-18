//! Structured-decision readiness suggestions.

use anyhow::Context;
use std::path::{Path, PathBuf};

use crate::check_guidance::shell_path;
use crate::doctor::plural;
use crate::storage::read_json;
use crate::{
    COMPARE_RECEIPT_FILE, DecisionSuggestArgs, is_regression, load_validated_config,
    resolve_configured_out_dir,
};
use perfgate::domain::{MetricMovement, is_improvement, movement_for_delta};
use perfgate_types::{CompareReceipt, ConfigFile, Delta, Direction, Metric, MetricStatus};

const DECISION_HIGH_NOISE_CV: f64 = 0.10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecisionReadiness {
    RunLocalGateFirst,
    SimpleGateEnough,
    PairedModeRecommended,
    StructuredDecisionCandidate,
    StructuredDecisionReady,
    ReadyToBundle,
}

impl DecisionReadiness {
    fn status(self) -> &'static str {
        match self {
            Self::RunLocalGateFirst => "run_local_gate_first",
            Self::SimpleGateEnough => "simple_gate_enough",
            Self::PairedModeRecommended => "paired_mode_recommended",
            Self::StructuredDecisionCandidate => "structured_decision_candidate",
            Self::StructuredDecisionReady => "structured_decision_ready",
            Self::ReadyToBundle => "ready_to_bundle",
        }
    }

    fn meaning(self) -> &'static str {
        match self {
            Self::RunLocalGateFirst => {
                "No compare receipts were found yet; run the local gate before making a decision."
            }
            Self::SimpleGateEnough => {
                "The current evidence does not require structured decision ceremony."
            }
            Self::PairedModeRecommended => {
                "The current evidence looks noisy; paired mode is a better next step than decision policy."
            }
            Self::StructuredDecisionCandidate => {
                "Structured decisions may help, but scenario/tradeoff/probe evidence is not ready yet."
            }
            Self::StructuredDecisionReady => {
                "Scenario and tradeoff policy are configured; decision evaluation is useful now."
            }
            Self::ReadyToBundle => {
                "A decision index already exists; the evidence can be exported as a portable bundle."
            }
        }
    }
}

#[derive(Debug)]
struct DecisionReadinessEvidence {
    compare_found: usize,
    compare_missing: usize,
    compare_artifacts: Vec<CompareEvidence>,
    missing_compare_artifacts: Vec<(String, PathBuf)>,
    has_regression: bool,
    has_improvement: bool,
    high_noise: bool,
    has_probe_config: bool,
    probe_receipts_found: usize,
    probe_artifacts: Vec<ProbeEvidence>,
    decision_index_exists: bool,
}

#[derive(Debug, Clone)]
struct CompareEvidence {
    bench: String,
    path: PathBuf,
    verdict: &'static str,
    metrics: Vec<MetricEvidence>,
}

#[derive(Debug, Clone)]
struct MetricEvidence {
    metric: Metric,
    movement: MetricMovement,
    direction: Direction,
    pct: f64,
    status: MetricStatus,
    cv: Option<f64>,
    noise_threshold: Option<f64>,
}

#[derive(Debug, Clone)]
struct ProbeEvidence {
    path: PathBuf,
    found: bool,
}

pub(crate) fn execute_decision_suggest(args: DecisionSuggestArgs) -> anyhow::Result<()> {
    let config = load_validated_config(&args.config)?;
    let out_dir = args
        .out_dir
        .clone()
        .unwrap_or_else(|| resolve_configured_out_dir(None, Some(&config)));
    let evidence = collect_decision_readiness_evidence(&config, &out_dir)?;
    let readiness = classify_decision_readiness(&config, &evidence);
    let gaps = decision_readiness_gaps(&config, &evidence);

    println!("perfgate decision suggest");
    println!();
    println!("Status: {}", readiness.status());
    println!("Meaning: {}", readiness.meaning());
    println!();
    println!("Evidence:");
    println!("  benches: {}", config.benches.len());
    println!("  compare receipts found: {}", evidence.compare_found);
    println!("  compare receipts missing: {}", evidence.compare_missing);
    println!("  scenarios: {}", config.scenarios.len());
    println!("  tradeoff rules: {}", config.tradeoffs.len());
    println!(
        "  probe evidence: {}",
        if evidence.has_probe_config {
            format!(
                "configured, {} receipt{}",
                evidence.probe_receipts_found,
                plural(evidence.probe_receipts_found)
            )
        } else {
            "not configured".to_string()
        }
    );
    println!(
        "  decision index: {}",
        if evidence.decision_index_exists {
            out_dir.join("decision.index.json").display().to_string()
        } else {
            "missing".to_string()
        }
    );
    println!();
    println!("Why:");
    for reason in decision_readiness_reasons(readiness, &config, &evidence, &out_dir) {
        println!("  - {reason}");
    }
    println!();
    println!("Artifacts:");
    print_decision_artifacts(&evidence, &out_dir);
    println!();
    println!("Structured decisions may help if:");
    println!("  - one benchmark regressed while another improved");
    println!("  - reviewers need to accept a bounded tradeoff");
    println!("  - probe or scenario evidence explains where work moved");
    if !gaps.is_empty() {
        println!();
        println!("Not ready yet:");
        for gap in &gaps {
            println!("  - {gap}");
        }
    }
    println!();
    println!("Next:");
    for command in decision_readiness_next_commands(readiness, &args.config, &out_dir) {
        println!("  {command}");
    }
    println!("Do not:");
    println!("  do not make structured decisions mandatory for simple local gates");

    Ok(())
}

fn collect_decision_readiness_evidence(
    config: &ConfigFile,
    out_dir: &Path,
) -> anyhow::Result<DecisionReadinessEvidence> {
    let mut compare_found = 0usize;
    let mut compare_missing = 0usize;
    let mut compare_artifacts = Vec::new();
    let mut missing_compare_artifacts = Vec::new();
    let mut has_regression = false;
    let mut has_improvement = false;
    let mut high_noise = false;

    for (bench_name, path) in decision_compare_paths(config, out_dir) {
        if !path.exists() {
            compare_missing += 1;
            missing_compare_artifacts.push((bench_name, path));
            continue;
        }
        compare_found += 1;
        let compare: CompareReceipt = read_json(&path).with_context(|| {
            format!("read compare receipt for {bench_name}: {}", path.display())
        })?;
        has_regression |= is_regression(compare.verdict.status);
        has_improvement |= compare
            .deltas
            .iter()
            .any(|(metric, delta)| is_improvement(*metric, delta));
        high_noise |= compare
            .deltas
            .values()
            .any(|delta| delta.cv.is_some_and(|cv| cv > DECISION_HIGH_NOISE_CV));
        compare_artifacts.push(compare_evidence(bench_name, path, &compare));
    }

    let probe_paths = configured_decision_probe_paths(config);
    let has_probe_config = !probe_paths.is_empty();
    let probe_receipts_found = probe_paths.iter().filter(|path| path.exists()).count();
    let probe_artifacts = probe_paths
        .iter()
        .map(|path| ProbeEvidence {
            path: path.clone(),
            found: path.exists(),
        })
        .collect();

    Ok(DecisionReadinessEvidence {
        compare_found,
        compare_missing,
        compare_artifacts,
        missing_compare_artifacts,
        has_regression,
        has_improvement,
        high_noise,
        has_probe_config,
        probe_receipts_found,
        probe_artifacts,
        decision_index_exists: out_dir.join("decision.index.json").exists(),
    })
}

fn compare_evidence(bench: String, path: PathBuf, compare: &CompareReceipt) -> CompareEvidence {
    CompareEvidence {
        bench,
        path,
        verdict: compare.verdict.status.as_str(),
        metrics: compare
            .deltas
            .iter()
            .map(|(metric, delta)| metric_evidence(*metric, delta))
            .collect(),
    }
}

fn metric_evidence(metric: Metric, delta: &Delta) -> MetricEvidence {
    MetricEvidence {
        metric,
        movement: movement_for_delta(metric, delta),
        direction: metric.default_direction(),
        pct: delta.pct,
        status: delta.status,
        cv: delta.cv,
        noise_threshold: delta.noise_threshold,
    }
}

fn decision_compare_paths(config: &ConfigFile, out_dir: &Path) -> Vec<(String, PathBuf)> {
    let single_compare = out_dir.join(COMPARE_RECEIPT_FILE);
    config
        .benches
        .iter()
        .map(|bench| {
            let per_bench = out_dir.join(&bench.name).join(COMPARE_RECEIPT_FILE);
            let path = if config.benches.len() == 1 && single_compare.exists() {
                single_compare.clone()
            } else {
                per_bench
            };
            (bench.name.clone(), path)
        })
        .collect()
}

fn configured_decision_probe_paths(config: &ConfigFile) -> Vec<PathBuf> {
    config
        .scenarios
        .iter()
        .flat_map(|scenario| {
            [
                scenario.probe_baseline.as_deref(),
                scenario.probe_current.as_deref(),
                scenario.probe_compare.as_deref(),
            ]
        })
        .flatten()
        .map(PathBuf::from)
        .collect()
}

fn classify_decision_readiness(
    config: &ConfigFile,
    evidence: &DecisionReadinessEvidence,
) -> DecisionReadiness {
    if evidence.decision_index_exists {
        return DecisionReadiness::ReadyToBundle;
    }
    if evidence.compare_found == 0 {
        return DecisionReadiness::RunLocalGateFirst;
    }
    if evidence.high_noise {
        return DecisionReadiness::PairedModeRecommended;
    }
    if !config.scenarios.is_empty() && !config.tradeoffs.is_empty() {
        return DecisionReadiness::StructuredDecisionReady;
    }
    if evidence.has_regression || evidence.has_improvement {
        return DecisionReadiness::StructuredDecisionCandidate;
    }
    DecisionReadiness::SimpleGateEnough
}

fn decision_readiness_gaps(
    config: &ConfigFile,
    evidence: &DecisionReadinessEvidence,
) -> Vec<&'static str> {
    let mut gaps = Vec::new();
    if evidence.compare_found == 0 {
        gaps.push("no compare receipts found; run `perfgate check` first");
    }
    if config.scenarios.is_empty() {
        gaps.push("no scenario weights configured");
    }
    if config.tradeoffs.is_empty() {
        gaps.push("no tradeoff rules configured");
    }
    if !evidence.has_probe_config {
        gaps.push("no probe evidence configured");
    } else if evidence.probe_receipts_found == 0 {
        gaps.push("configured probe evidence was not found on disk");
    }
    gaps
}

fn decision_readiness_next_commands(
    readiness: DecisionReadiness,
    config_path: &Path,
    out_dir: &Path,
) -> Vec<String> {
    match readiness {
        DecisionReadiness::RunLocalGateFirst => vec![format!(
            "perfgate check --config {} --all --require-baseline",
            shell_path(config_path)
        )],
        DecisionReadiness::SimpleGateEnough => vec![format!(
            "perfgate check --config {} --all --require-baseline",
            shell_path(config_path)
        )],
        DecisionReadiness::PairedModeRecommended => vec![
            "perfgate paired --name <bench> --baseline-cmd \"<baseline-cmd>\" --current-cmd \"<current-cmd>\" --repeat 10 --out artifacts/perfgate/<bench>/paired.json".to_string(),
            format!("perfgate calibrate --config {} --bench <bench>", shell_path(config_path)),
        ],
        DecisionReadiness::StructuredDecisionCandidate => vec![
            "add scenario weights and tradeoff rules for the review question".to_string(),
            "add probe evidence only if reviewers need to know where work moved".to_string(),
        ],
        DecisionReadiness::StructuredDecisionReady => vec![
            format!("perfgate decision evaluate --config {}", shell_path(config_path)),
            format!(
                "perfgate decision bundle --index {}",
                out_dir.join("decision.index.json").display()
            ),
        ],
        DecisionReadiness::ReadyToBundle => vec![format!(
            "perfgate decision bundle --index {}",
            out_dir.join("decision.index.json").display()
        )],
    }
}

fn decision_readiness_reasons(
    readiness: DecisionReadiness,
    config: &ConfigFile,
    evidence: &DecisionReadinessEvidence,
    out_dir: &Path,
) -> Vec<String> {
    let mut reasons = Vec::new();
    match readiness {
        DecisionReadiness::RunLocalGateFirst => {
            reasons.push(
                "no compare receipts were found; local gate evidence must exist first".to_string(),
            );
        }
        DecisionReadiness::SimpleGateEnough => {
            reasons.push(
                "compare receipts exist and no noisy or tradeoff-shaped movement was found"
                    .to_string(),
            );
        }
        DecisionReadiness::PairedModeRecommended => {
            reasons.push(format!(
                "at least one metric has CV above {:.1}%; paired mode is safer than policy",
                DECISION_HIGH_NOISE_CV * 100.0
            ));
        }
        DecisionReadiness::StructuredDecisionCandidate => {
            reasons.push(
                "metric movement exists, but scenario/tradeoff/probe evidence is incomplete"
                    .to_string(),
            );
        }
        DecisionReadiness::StructuredDecisionReady => {
            reasons.push(format!(
                "{} scenario{} and {} tradeoff rule{} are configured",
                config.scenarios.len(),
                plural(config.scenarios.len()),
                config.tradeoffs.len(),
                plural(config.tradeoffs.len())
            ));
        }
        DecisionReadiness::ReadyToBundle => {
            reasons.push(format!(
                "decision index already exists at {}",
                out_dir.join("decision.index.json").display()
            ));
        }
    }

    for compare in &evidence.compare_artifacts {
        reasons.push(format!(
            "{} compare verdict is {} ({})",
            compare.bench,
            compare.verdict,
            compare.path.display()
        ));
        for metric in &compare.metrics {
            reasons.push(metric_reason(metric));
            if let Some(noise) = metric_noise_reason(metric) {
                reasons.push(noise);
            }
        }
    }

    if evidence.has_probe_config {
        reasons.push(format!(
            "probe evidence configured with {} receipt{} found",
            evidence.probe_receipts_found,
            plural(evidence.probe_receipts_found)
        ));
    }
    if !evidence.missing_compare_artifacts.is_empty() {
        for (bench, path) in &evidence.missing_compare_artifacts {
            reasons.push(format!(
                "{bench} compare receipt is missing at {}",
                path.display()
            ));
        }
    }
    if matches!(
        readiness,
        DecisionReadiness::StructuredDecisionReady | DecisionReadiness::ReadyToBundle
    ) {
        reasons.push(
            "optional ledger history can record the decision after local receipts are reviewed"
                .to_string(),
        );
    }

    reasons
}

fn metric_reason(metric: &MetricEvidence) -> String {
    format!(
        "{} {} by {} (direction {}, threshold status {})",
        metric.metric.as_str(),
        movement_label(metric.movement),
        format_percent(metric.pct.abs()),
        direction_label(metric.direction),
        metric.status.as_str()
    )
}

fn metric_noise_reason(metric: &MetricEvidence) -> Option<String> {
    let cv = metric.cv?;
    let threshold = metric.noise_threshold.unwrap_or(DECISION_HIGH_NOISE_CV);
    if cv > threshold {
        Some(format!(
            "{} noise is high: CV {} exceeds {}",
            metric.metric.as_str(),
            format_percent(cv),
            format_percent(threshold)
        ))
    } else {
        Some(format!(
            "{} noise is within guidance: CV {} at or below {}",
            metric.metric.as_str(),
            format_percent(cv),
            format_percent(threshold)
        ))
    }
}

fn print_decision_artifacts(evidence: &DecisionReadinessEvidence, out_dir: &Path) {
    for compare in &evidence.compare_artifacts {
        println!("  compare: {}", compare.path.display());
    }
    for (bench, path) in &evidence.missing_compare_artifacts {
        println!("  compare: {} (missing for {bench})", path.display());
    }
    for probe in &evidence.probe_artifacts {
        println!(
            "  probe: {}{}",
            probe.path.display(),
            if probe.found { "" } else { " (missing)" }
        );
    }
    println!(
        "  decision index: {}{}",
        out_dir.join("decision.index.json").display(),
        if evidence.decision_index_exists {
            ""
        } else {
            " (missing)"
        }
    );
}

fn movement_label(movement: MetricMovement) -> &'static str {
    match movement {
        MetricMovement::Improved => "improved",
        MetricMovement::Regressed => "regressed",
        MetricMovement::Unchanged => "stayed unchanged",
        MetricMovement::Unknown => "has unknown movement",
    }
}

fn direction_label(direction: Direction) -> &'static str {
    match direction {
        Direction::Lower => "lower_is_better",
        Direction::Higher => "higher_is_better",
    }
}

fn format_percent(value: f64) -> String {
    format!("{:.1}%", value * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{
        BenchConfigFile, BenchMeta, Budget, COMPARE_SCHEMA_V1, CompareReceipt, CompareRef,
        ConfigFile, Delta, Metric, MetricStatistic, MetricStatus, ScenarioConfigFile, ToolInfo,
        TradeoffDowngrade, TradeoffRule, Verdict, VerdictCounts, VerdictStatus,
    };
    use std::collections::BTreeMap;
    use std::fs;
    use tempfile::tempdir;

    fn config_with_benches(bench_names: &[&str]) -> ConfigFile {
        ConfigFile {
            benches: bench_names
                .iter()
                .map(|name| BenchConfigFile {
                    name: (*name).to_string(),
                    cwd: None,
                    work: None,
                    timeout: None,
                    command: vec!["true".into()],
                    repeat: None,
                    warmup: None,
                    metrics: None,
                    budgets: None,
                    scaling: None,
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn decision_compare_paths_uses_single_receipt_for_single_bench() {
        let config = config_with_benches(&["parser"]);
        let temp = tempdir().expect("temp dir");
        let out_dir = temp.path().join("artifacts");
        fs::create_dir_all(&out_dir).expect("create out dir");

        let expected = out_dir.join(COMPARE_RECEIPT_FILE);
        fs::write(&expected, "{}").expect("write root compare");

        let paths = decision_compare_paths(&config, &out_dir);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].1, expected);
    }

    #[test]
    fn decision_compare_paths_uses_per_bench_path_when_more_than_one_bench() {
        let config = config_with_benches(&["alpha", "beta"]);
        let temp = tempdir().expect("temp dir");
        let out_dir = temp.path().join("artifacts");
        fs::create_dir_all(&out_dir).expect("create out dir");

        fs::write(out_dir.join("compare.json"), "{}").expect("write shared compare");

        let paths = decision_compare_paths(&config, &out_dir);
        assert_eq!(
            paths,
            vec![
                (
                    "alpha".to_string(),
                    out_dir.join("alpha").join(COMPARE_RECEIPT_FILE)
                ),
                (
                    "beta".to_string(),
                    out_dir.join("beta").join(COMPARE_RECEIPT_FILE)
                )
            ]
        );
    }

    #[test]
    fn configured_decision_probe_paths_collects_all_scenarios() {
        let config = ConfigFile {
            scenarios: vec![
                ScenarioConfigFile {
                    name: "release".to_string(),
                    weight: 1.0,
                    bench: "alpha".to_string(),
                    description: None,
                    compare: None,
                    probe_compare: Some("scenario/compare.json".to_string()),
                    probe_baseline: Some("scenario/baseline.json".to_string()),
                    probe_current: Some("scenario/current.json".to_string()),
                },
                ScenarioConfigFile {
                    name: "startup".to_string(),
                    weight: 2.0,
                    bench: "beta".to_string(),
                    description: None,
                    compare: None,
                    probe_compare: Some("startup/compare.json".to_string()),
                    probe_baseline: Some("startup/baseline.json".to_string()),
                    probe_current: None,
                },
            ],
            ..Default::default()
        };

        let probe_paths = configured_decision_probe_paths(&config);
        let expected: Vec<_> = vec![
            "scenario/baseline.json",
            "scenario/current.json",
            "scenario/compare.json",
            "startup/baseline.json",
            "startup/compare.json",
        ]
        .into_iter()
        .map(std::path::PathBuf::from)
        .collect();

        assert_eq!(probe_paths, expected);
    }

    #[test]
    fn collect_decision_readiness_evidence_sees_regression_improvement_and_noise() {
        let config = config_with_benches(&["parser"]);
        let temp = tempdir().expect("temp dir");
        let out_dir = temp.path().join("artifacts");
        fs::create_dir_all(&out_dir).expect("create out dir");
        let compare_path = out_dir.join(COMPARE_RECEIPT_FILE);

        let mut deltas = BTreeMap::new();
        deltas.insert(
            Metric::WallMs,
            Delta {
                baseline: 100.0,
                current: 140.0,
                ratio: 1.4,
                pct: 0.4,
                regression: 0.4,
                cv: Some(0.11),
                noise_threshold: None,
                statistic: MetricStatistic::Median,
                significance: None,
                status: MetricStatus::Fail,
            },
        );
        deltas.insert(
            Metric::ThroughputPerS,
            Delta {
                baseline: 200.0,
                current: 220.0,
                ratio: 1.1,
                pct: 0.1,
                regression: 0.0,
                cv: None,
                noise_threshold: None,
                statistic: MetricStatistic::Median,
                significance: None,
                status: MetricStatus::Pass,
            },
        );

        let receipt = CompareReceipt {
            schema: COMPARE_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".into(),
                version: "0.19.0".into(),
            },
            bench: BenchMeta {
                name: "parser".into(),
                cwd: None,
                command: vec!["echo".into()],
                repeat: 1,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            baseline_ref: CompareRef {
                path: Some("baseline.json".into()),
                run_id: Some("base".into()),
            },
            current_ref: CompareRef {
                path: Some("current.json".into()),
                run_id: Some("cur".into()),
            },
            budgets: BTreeMap::<Metric, Budget>::new(),
            deltas,
            verdict: Verdict {
                status: VerdictStatus::Warn,
                counts: VerdictCounts {
                    pass: 1,
                    warn: 1,
                    fail: 0,
                    skip: 0,
                },
                reasons: Vec::new(),
            },
        };
        fs::write(
            &compare_path,
            serde_json::to_string_pretty(&receipt).expect("serialize receipt"),
        )
        .expect("write compare");

        let evidence =
            collect_decision_readiness_evidence(&config, &out_dir).expect("collect evidence");

        assert_eq!(evidence.compare_found, 1);
        assert_eq!(evidence.compare_missing, 0);
        assert!(evidence.has_regression);
        assert!(evidence.has_improvement);
        assert!(evidence.high_noise);
        assert!(!evidence.has_probe_config);
        assert_eq!(evidence.probe_receipts_found, 0);
    }

    #[test]
    fn decision_readiness_gaps_reported_in_expected_order() {
        let mut config = ConfigFile::default();
        let evidence = DecisionReadinessEvidence {
            compare_found: 0,
            compare_missing: 1,
            compare_artifacts: Vec::new(),
            missing_compare_artifacts: vec![("bench".into(), PathBuf::from("compare.json"))],
            has_regression: false,
            has_improvement: false,
            high_noise: false,
            has_probe_config: false,
            probe_receipts_found: 0,
            probe_artifacts: Vec::new(),
            decision_index_exists: false,
        };
        assert_eq!(
            decision_readiness_gaps(&config, &evidence),
            vec![
                "no compare receipts found; run `perfgate check` first",
                "no scenario weights configured",
                "no tradeoff rules configured",
                "no probe evidence configured",
            ]
        );

        config.scenarios.push(ScenarioConfigFile {
            name: "release".into(),
            weight: 1.0,
            bench: "bench".into(),
            description: None,
            compare: None,
            probe_compare: None,
            probe_baseline: None,
            probe_current: None,
        });
        config.tradeoffs.push(TradeoffRule {
            name: "balance".into(),
            if_failed: Metric::WallMs,
            require: vec![],
            allow: vec![],
            downgrade_to: TradeoffDowngrade::Warn,
        });

        let configured_only = DecisionReadinessEvidence {
            compare_found: 1,
            compare_missing: 0,
            compare_artifacts: Vec::new(),
            missing_compare_artifacts: Vec::new(),
            has_regression: false,
            has_improvement: false,
            high_noise: false,
            has_probe_config: true,
            probe_receipts_found: 0,
            probe_artifacts: vec![ProbeEvidence {
                path: PathBuf::from("probe-compare.json"),
                found: false,
            }],
            decision_index_exists: false,
        };
        assert_eq!(
            decision_readiness_gaps(&config, &configured_only),
            vec!["configured probe evidence was not found on disk"]
        );
    }

    #[test]
    fn classify_decision_readiness_prioritizes_overall_ordering() {
        let mut config = config_with_benches(&["parser"]);
        let evidence = DecisionReadinessEvidence {
            compare_found: 1,
            compare_missing: 0,
            compare_artifacts: Vec::new(),
            missing_compare_artifacts: Vec::new(),
            has_regression: true,
            has_improvement: true,
            high_noise: true,
            has_probe_config: false,
            probe_receipts_found: 0,
            probe_artifacts: Vec::new(),
            decision_index_exists: false,
        };
        config.scenarios.push(ScenarioConfigFile {
            name: "release".into(),
            weight: 1.0,
            bench: "parser".into(),
            description: None,
            compare: None,
            probe_compare: None,
            probe_baseline: None,
            probe_current: None,
        });
        config.tradeoffs.push(TradeoffRule {
            name: "balance".into(),
            if_failed: Metric::WallMs,
            require: vec![],
            allow: vec![],
            downgrade_to: TradeoffDowngrade::Warn,
        });

        assert_eq!(
            classify_decision_readiness(&config, &evidence),
            DecisionReadiness::PairedModeRecommended
        );

        let mut bundled = evidence;
        bundled.decision_index_exists = true;
        assert_eq!(
            classify_decision_readiness(&config, &bundled),
            DecisionReadiness::ReadyToBundle
        );
        bundled.decision_index_exists = false;
        let mut still_no_index = bundled;
        still_no_index.has_regression = false;
        still_no_index.has_improvement = false;
        still_no_index.high_noise = false;
        assert_eq!(
            classify_decision_readiness(&config, &still_no_index),
            DecisionReadiness::StructuredDecisionReady
        );
    }
}
