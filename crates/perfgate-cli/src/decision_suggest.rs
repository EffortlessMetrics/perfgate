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
use perfgate::domain::is_improvement;
use perfgate_types::{CompareReceipt, ConfigFile};

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
    has_regression: bool,
    has_improvement: bool,
    high_noise: bool,
    has_probe_config: bool,
    probe_receipts_found: usize,
    decision_index_exists: bool,
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
    let mut has_regression = false;
    let mut has_improvement = false;
    let mut high_noise = false;

    for (bench_name, path) in decision_compare_paths(config, out_dir) {
        if !path.exists() {
            compare_missing += 1;
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
            .any(|delta| delta.cv.is_some_and(|cv| cv > 0.10));
    }

    let probe_paths = configured_decision_probe_paths(config);
    let has_probe_config = !probe_paths.is_empty();
    let probe_receipts_found = probe_paths.iter().filter(|path| path.exists()).count();

    Ok(DecisionReadinessEvidence {
        compare_found,
        compare_missing,
        has_regression,
        has_improvement,
        high_noise,
        has_probe_config,
        probe_receipts_found,
        decision_index_exists: out_dir.join("decision.index.json").exists(),
    })
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
