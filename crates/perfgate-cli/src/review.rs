//! First-use performance review surfaces.

use anyhow::Context;
use clap::{Args, Subcommand};
use perfgate_types::config::load_config_file;
use perfgate_types::error::ConfigValidationError;
use perfgate_types::{CompareReceipt, ConfigFile};
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::baseline_doctor::{
    BaselineDoctorRow, BaselineMaturity, configured_benches, inspect_baseline,
};
use crate::benchmark_passport::BenchmarkPassport;
use crate::doctor::{SignalDoctorRow, SignalRecommendation, inspect_signal, plural};
use crate::imported_evidence::ImportedEvidenceSummary;
use crate::{check_command, paired_command, read_json, resolve_configured_out_dir};

#[derive(Debug, Subcommand)]
pub enum ReviewAction {
    /// Explain first-use performance posture without changing config or baselines.
    Explain(ReviewExplainArgs),
}

#[derive(Debug, Args)]
pub struct ReviewExplainArgs {
    /// Path to the config file (TOML or JSON).
    #[arg(long, default_value = "perfgate.toml")]
    pub config: PathBuf,

    /// Output directory containing recent artifacts. Defaults to [defaults].out_dir or artifacts/perfgate.
    #[arg(long, value_name = "DIR")]
    pub out_dir: Option<PathBuf>,

    /// Benchmark to explain.
    #[arg(long)]
    pub bench: String,

    /// Emit machine-readable JSON instead of human-readable text.
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Debug, Serialize)]
struct ReviewExplainOutput {
    config: String,
    bench: String,
    gate_verdict: String,
    baseline: BaselineSummary,
    signal: SignalSummary,
    policy: PolicySummary,
    benchmark_passport: BenchmarkPassport,
    evidence_source: EvidenceSourceSummary,
    artifacts: Vec<ArtifactSummary>,
    next_commands: Vec<String>,
    human_review_required: Vec<String>,
    agent_guardrails: AgentGuardrails,
    non_inferences: Vec<String>,
}

#[derive(Debug, Serialize)]
struct BaselineSummary {
    maturity: String,
    path: String,
    samples: Option<usize>,
    cv: Option<f64>,
    host: Option<String>,
    age_days: Option<i64>,
    recommendation: String,
}

#[derive(Debug, Serialize)]
struct SignalSummary {
    recommendation: String,
    meaning: String,
    samples: usize,
    cv: Option<f64>,
    host_stability: String,
    baseline_age_days: Option<i64>,
    recent_drift: String,
    calibration_status: String,
    host_compatibility: String,
    proof_freshness: String,
}

#[derive(Debug, Serialize)]
struct PolicySummary {
    current_posture: String,
    recommended_posture: String,
    decision_readiness: String,
    reasons: Vec<String>,
    missing_or_review_required: Vec<String>,
}

#[derive(Debug, Serialize)]
struct EvidenceSourceSummary {
    kind: String,
    source_path: Option<String>,
    sample_model: String,
    host_context: String,
    noise_support: String,
    metric_mappings: Vec<String>,
    limitations: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ArtifactSummary {
    label: String,
    path: String,
    exists: bool,
}

#[derive(Debug, Serialize)]
struct AgentGuardrails {
    allowed: Vec<String>,
    forbidden_by_default: Vec<String>,
    human_review_required: Vec<String>,
}

struct ReviewEvidence {
    config: ConfigFile,
    baseline: BaselineDoctorRow,
    signal: SignalDoctorRow,
    compare: Option<CompareReceipt>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReviewPolicyPosture {
    Smoke,
    Advisory,
    GateCandidate,
    Quarantined,
}

impl ReviewPolicyPosture {
    fn as_str(self) -> &'static str {
        match self {
            Self::Smoke => "smoke",
            Self::Advisory => "advisory",
            Self::GateCandidate => "gate_candidate",
            Self::Quarantined => "quarantined",
        }
    }
}

pub fn execute_review_action(action: ReviewAction) -> anyhow::Result<()> {
    match action {
        ReviewAction::Explain(args) => execute_review_explain(args),
    }
}

fn execute_review_explain(args: ReviewExplainArgs) -> anyhow::Result<()> {
    let evidence = load_review_evidence(&args.config, args.out_dir.as_ref(), &args.bench)?;
    let out_dir = resolve_configured_out_dir(args.out_dir.as_ref(), Some(&evidence.config));
    let output = build_review_explain_output(&args.config, &out_dir, &evidence);

    if args.json {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print!("{}", render_review_explain(&output));
    }

    Ok(())
}

fn load_review_evidence(
    config_path: &Path,
    out_dir: Option<&PathBuf>,
    bench_name: &str,
) -> anyhow::Result<ReviewEvidence> {
    let config = load_config_file(config_path)?;
    config
        .validate()
        .map_err(ConfigValidationError::ConfigFile)?;
    let benches = configured_benches(&config, Some(bench_name))?;
    let Some(bench) = benches.first() else {
        anyhow::bail!("benchmark '{bench_name}' was not returned after config validation");
    };
    let resolved_out_dir = resolve_configured_out_dir(out_dir, Some(&config));
    let baseline = inspect_baseline(&config, bench)?;
    let signal = inspect_signal(&config, &resolved_out_dir, bench)?;
    let compare = read_compare_if_present(&signal)
        .with_context(|| format!("failed to read compare receipt for {bench}"))?;

    Ok(ReviewEvidence {
        config,
        baseline,
        signal,
        compare,
    })
}

fn build_review_explain_output(
    config_path: &Path,
    out_dir: &Path,
    evidence: &ReviewEvidence,
) -> ReviewExplainOutput {
    let baseline = &evidence.baseline;
    let signal = &evidence.signal;
    let current_posture = current_posture(baseline);
    let recommended_posture = recommended_posture(baseline, signal);
    let evidence_source = summarize_evidence_source(baseline, signal);
    let non_inferences = non_inferences(&evidence_source);
    let human_review_required =
        human_review_required(baseline, signal, recommended_posture, &evidence_source);
    let next_commands = next_commands(config_path, baseline, signal, recommended_posture);
    let next_safe_action = next_commands
        .first()
        .cloned()
        .unwrap_or_else(|| check_command(config_path, Some(&baseline.bench), false));
    let proof_freshness = proof_freshness(baseline, signal);
    let benchmark_passport = BenchmarkPassport::from_rows(
        baseline,
        signal,
        recommended_posture.as_str(),
        proof_freshness.clone(),
        non_inferences.clone(),
        next_safe_action,
    );

    ReviewExplainOutput {
        config: config_path.display().to_string(),
        bench: baseline.bench.clone(),
        gate_verdict: gate_verdict(signal, evidence.compare.as_ref()),
        baseline: BaselineSummary {
            maturity: baseline.maturity.as_str().to_string(),
            path: baseline.path.clone(),
            samples: baseline.samples,
            cv: baseline.cv,
            host: baseline.host.clone(),
            age_days: baseline.age_days,
            recommendation: baseline.maturity.recommendation().to_string(),
        },
        signal: SignalSummary {
            recommendation: signal.recommendation.as_str().to_string(),
            meaning: signal.recommendation.meaning().to_string(),
            samples: signal.samples,
            cv: signal.cv,
            host_stability: signal.host_stability.clone(),
            baseline_age_days: signal.baseline_age_days,
            recent_drift: signal.recent_drift.clone(),
            calibration_status: calibration_status(signal).to_string(),
            host_compatibility: host_compatibility(signal),
            proof_freshness,
        },
        policy: PolicySummary {
            current_posture: current_posture.as_str().to_string(),
            recommended_posture: recommended_posture.as_str().to_string(),
            decision_readiness: decision_readiness(&evidence.config, &baseline.bench).to_string(),
            reasons: policy_reasons(baseline, signal, recommended_posture, &evidence_source),
            missing_or_review_required: policy_missing_requirements(
                baseline,
                signal,
                recommended_posture,
                &evidence_source,
            ),
        },
        benchmark_passport,
        evidence_source,
        artifacts: artifact_summaries(signal, out_dir, &baseline.bench),
        next_commands,
        human_review_required: human_review_required.clone(),
        agent_guardrails: AgentGuardrails {
            allowed: vec![
                "inspect run, compare, report, comment, and repair artifacts".to_string(),
                "rerun the named benchmark or suggest a targeted code optimization".to_string(),
                "propose a reviewable config patch without writing it".to_string(),
            ],
            forbidden_by_default: vec![
                "promote baselines".to_string(),
                "loosen thresholds".to_string(),
                "make the benchmark required_gate".to_string(),
                "accept tradeoffs".to_string(),
                "require server ledger mode for local correctness".to_string(),
            ],
            human_review_required,
        },
        non_inferences,
    }
}

fn render_review_explain(output: &ReviewExplainOutput) -> String {
    let mut out = String::new();
    out.push_str("perfgate review explain\n");
    out.push_str(&format!("Config: {}\n", output.config));
    out.push_str(&format!("Bench: {}\n", output.bench));
    out.push_str(&format!("Gate verdict: {}\n", output.gate_verdict));
    out.push_str(&format!(
        "Baseline maturity: {}\n",
        output.baseline.maturity
    ));
    out.push_str(&format!(
        "Signal confidence: {} - {}\n",
        output.signal.recommendation, output.signal.meaning
    ));
    out.push_str(&format!(
        "Policy posture: current={}, recommended={}\n",
        output.policy.current_posture, output.policy.recommended_posture
    ));
    out.push_str(&format!(
        "Decision readiness: {}\n",
        output.policy.decision_readiness
    ));
    out.push_str(&format!(
        "Proof freshness: {}\n",
        output.signal.proof_freshness
    ));
    out.push_str(&format!(
        "Evidence source: {}\n",
        output.evidence_source.kind
    ));
    out.push_str(&format!(
        "Sample model: {}\n",
        output.evidence_source.sample_model
    ));
    out.push_str(&format!(
        "Host context: {}\n",
        output.evidence_source.host_context
    ));
    out.push_str(&format!(
        "Noise support: {}\n",
        output.evidence_source.noise_support
    ));
    output.benchmark_passport.render_terminal(&mut out);

    out.push_str("\nWhat this means:\n");
    for reason in &output.policy.reasons {
        out.push_str(&format!("  - {reason}\n"));
    }

    out.push_str("\nMissing or review-required:\n");
    for item in &output.policy.missing_or_review_required {
        out.push_str(&format!("  - {item}\n"));
    }

    out.push_str("\nArtifacts:\n");
    for artifact in &output.artifacts {
        let status = if artifact.exists { "" } else { " (missing)" };
        out.push_str(&format!(
            "  - {}: {}{}\n",
            artifact.label, artifact.path, status
        ));
    }

    out.push_str("\nNext safe commands:\n");
    for command in &output.next_commands {
        out.push_str(&format!("  {command}\n"));
    }

    out.push_str("\nAgent guardrails:\n");
    out.push_str("  Allowed:\n");
    for item in &output.agent_guardrails.allowed {
        out.push_str(&format!("    - {item}\n"));
    }
    out.push_str("  Human review required:\n");
    for item in &output.agent_guardrails.human_review_required {
        out.push_str(&format!("    - {item}\n"));
    }
    out.push_str("  Forbidden by default:\n");
    for item in &output.agent_guardrails.forbidden_by_default {
        out.push_str(&format!("    - {item}\n"));
    }

    out.push_str("\nDo not infer:\n");
    for item in &output.non_inferences {
        out.push_str(&format!("  - {item}\n"));
    }
    out.push_str(
        "\nAdvisory only: no config, baseline, threshold, policy, or server setting was changed.\n",
    );

    out
}

fn read_compare_if_present(signal: &SignalDoctorRow) -> anyhow::Result<Option<CompareReceipt>> {
    if signal.compare_found {
        Ok(Some(read_json(&signal.compare_path)?))
    } else {
        Ok(None)
    }
}

fn current_posture(baseline: &BaselineDoctorRow) -> ReviewPolicyPosture {
    match baseline.maturity {
        BaselineMaturity::Missing => ReviewPolicyPosture::Smoke,
        _ => ReviewPolicyPosture::Advisory,
    }
}

fn recommended_posture(
    baseline: &BaselineDoctorRow,
    signal: &SignalDoctorRow,
) -> ReviewPolicyPosture {
    match baseline.maturity {
        BaselineMaturity::Missing => return ReviewPolicyPosture::Advisory,
        BaselineMaturity::HostMismatched | BaselineMaturity::Stale => {
            return ReviewPolicyPosture::Quarantined;
        }
        BaselineMaturity::HighNoise
        | BaselineMaturity::New
        | BaselineMaturity::Immature
        | BaselineMaturity::Remote => return ReviewPolicyPosture::Advisory,
        BaselineMaturity::Mature => {}
    }

    match signal.recommendation {
        SignalRecommendation::SafeToGate => ReviewPolicyPosture::GateCandidate,
        SignalRecommendation::CheckHostMismatch | SignalRecommendation::RefreshBaseline => {
            ReviewPolicyPosture::Quarantined
        }
        SignalRecommendation::UsePairedMode
        | SignalRecommendation::AdvisoryOnly
        | SignalRecommendation::IncreaseSamples
        | SignalRecommendation::NoDecisionYet => ReviewPolicyPosture::Advisory,
    }
}

fn gate_verdict(signal: &SignalDoctorRow, compare: Option<&CompareReceipt>) -> String {
    if let Some(compare) = compare {
        return compare.verdict.status.as_str().to_string();
    }
    if !signal.baseline_found && !signal.baseline_remote {
        return "setup_incomplete_missing_baseline".to_string();
    }
    "no_compare_receipt_yet".to_string()
}

fn host_compatibility(signal: &SignalDoctorRow) -> String {
    if matches!(
        signal.recommendation,
        SignalRecommendation::CheckHostMismatch
    ) {
        format!("host_mismatch ({})", signal.host_stability)
    } else {
        format!("compatible_or_not_checked ({})", signal.host_stability)
    }
}

fn calibration_status(signal: &SignalDoctorRow) -> &'static str {
    if signal.cv.is_some_and(|cv| cv > 0.10) {
        "paired mode or calibration review required before promotion"
    } else if signal.samples >= 7 && signal.cv.is_some() {
        "review recommended before required_gate"
    } else {
        "insufficient evidence for reviewed calibration"
    }
}

fn proof_freshness(baseline: &BaselineDoctorRow, signal: &SignalDoctorRow) -> String {
    match baseline.maturity {
        BaselineMaturity::Missing => "unproven (baseline missing)".to_string(),
        BaselineMaturity::Stale => "stale (baseline older than maturity window)".to_string(),
        BaselineMaturity::Remote => {
            "unproven locally (remote baseline history not probed)".to_string()
        }
        _ if signal.run_found && signal.compare_found => {
            "current (local run and compare receipts present)".to_string()
        }
        _ if baseline.age_days.is_some() => format!(
            "recent baseline receipt ({} day{} old), compare receipt {}",
            baseline.age_days.unwrap_or_default(),
            plural(baseline.age_days.unwrap_or_default() as usize),
            if signal.compare_found {
                "present"
            } else {
                "missing"
            }
        ),
        _ => "unproven (receipt freshness unavailable)".to_string(),
    }
}

fn decision_readiness(config: &ConfigFile, bench_name: &str) -> &'static str {
    let scenario_for_bench = config
        .scenarios
        .iter()
        .any(|scenario| scenario.bench == bench_name);
    match (scenario_for_bench, config.tradeoffs.is_empty()) {
        (true, false) => "scenario and tradeoff evidence configured",
        (true, true) => "scenario evidence configured; add tradeoff policy only for real tradeoffs",
        (false, false) => "tradeoff policy exists; connect scenario evidence before relying on it",
        (false, true) => {
            "simple gate first; structured decisions are optional until a tradeoff appears"
        }
    }
}

fn summarize_evidence_source(
    baseline: &BaselineDoctorRow,
    signal: &SignalDoctorRow,
) -> EvidenceSourceSummary {
    let imported = policy_imported_evidence(baseline, signal);
    if let Some(imported) = imported {
        return imported_evidence_summary(imported);
    }

    EvidenceSourceSummary {
        kind: "native perfgate run".to_string(),
        source_path: None,
        sample_model: "native_receipts".to_string(),
        host_context: "perfgate_host_receipt".to_string(),
        noise_support: "native_samples_and_cv_when_present".to_string(),
        metric_mappings: vec!["perfgate run.v1 metrics".to_string()],
        limitations: Vec::new(),
    }
}

fn imported_evidence_summary(imported: &ImportedEvidenceSummary) -> EvidenceSourceSummary {
    EvidenceSourceSummary {
        kind: imported.source_label(),
        source_path: imported.source_path.clone(),
        sample_model: imported.sample_model.to_string(),
        host_context: imported.host_context.to_string(),
        noise_support: imported.noise_support.to_string(),
        metric_mappings: imported.metric_mappings.clone(),
        limitations: imported
            .limitations()
            .iter()
            .map(|limit| (*limit).to_string())
            .collect(),
    }
}

fn policy_imported_evidence<'a>(
    baseline: &'a BaselineDoctorRow,
    signal: &'a SignalDoctorRow,
) -> Option<&'a ImportedEvidenceSummary> {
    signal
        .imported_evidence
        .as_ref()
        .or(baseline.imported_evidence.as_ref())
}

fn policy_reasons(
    baseline: &BaselineDoctorRow,
    signal: &SignalDoctorRow,
    recommended: ReviewPolicyPosture,
    evidence_source: &EvidenceSourceSummary,
) -> Vec<String> {
    let mut reasons = Vec::new();
    reasons.push(format!("baseline {}", baseline.maturity.as_str()));
    reasons.push(format!(
        "signal {}: {}",
        signal.recommendation.as_str(),
        signal.recommendation.meaning()
    ));
    if let Some(samples) = baseline
        .samples
        .or(Some(signal.samples))
        .filter(|samples| *samples > 0)
    {
        reasons.push(format!(
            "{samples} measured sample{} available",
            plural(samples)
        ));
    }
    if let Some(cv) = baseline.cv.or(signal.cv) {
        reasons.push(format!("observed CV {:.1}%", cv * 100.0));
    }
    reasons.push(format!("evidence source {}", evidence_source.kind));
    reasons.push(format!("sample model {}", evidence_source.sample_model));
    match recommended {
        ReviewPolicyPosture::GateCandidate => {
            reasons.push("evidence can be reviewed for gate_candidate, not required_gate".into());
        }
        ReviewPolicyPosture::Quarantined => {
            reasons.push(
                "policy should pause until host, freshness, or setup evidence is repaired".into(),
            );
        }
        ReviewPolicyPosture::Advisory => {
            reasons.push(
                "evidence should remain advisory until missing requirements are resolved".into(),
            );
        }
        ReviewPolicyPosture::Smoke => {
            reasons.push("use this only for setup confidence until evidence exists".into());
        }
    }
    reasons
}

fn policy_missing_requirements(
    baseline: &BaselineDoctorRow,
    signal: &SignalDoctorRow,
    recommended: ReviewPolicyPosture,
    evidence_source: &EvidenceSourceSummary,
) -> Vec<String> {
    let mut missing = Vec::new();
    match baseline.maturity {
        BaselineMaturity::Missing => missing.push("baseline promotion after workload review"),
        BaselineMaturity::New | BaselineMaturity::Immature => {
            missing.push("more measured samples before blocking")
        }
        BaselineMaturity::HighNoise => missing.push("paired-mode or calibration review"),
        BaselineMaturity::HostMismatched => missing.push("compatible runner-class evidence"),
        BaselineMaturity::Stale => missing.push("fresh baseline review"),
        BaselineMaturity::Remote => missing.push("server history review before gating"),
        BaselineMaturity::Mature => {}
    }
    match signal.recommendation {
        SignalRecommendation::SafeToGate => {}
        SignalRecommendation::AdvisoryOnly => missing.push("compare receipt for current evidence"),
        SignalRecommendation::IncreaseSamples => missing.push("signal sample count"),
        SignalRecommendation::UsePairedMode => missing.push("paired-mode evidence"),
        SignalRecommendation::RefreshBaseline => missing.push("baseline refresh"),
        SignalRecommendation::CheckHostMismatch => missing.push("host-compatible rerun"),
        SignalRecommendation::NoDecisionYet => missing.push("complete setup receipts"),
    }
    if evidence_source.host_context == "missing_or_partial" {
        missing.push("imported source host context review");
    }
    if evidence_source.sample_model == "summary_only" {
        missing.push("raw-sample or paired evidence before blocking");
    }
    if recommended == ReviewPolicyPosture::GateCandidate {
        missing.push("required-gate reviewer approval");
        missing.push("reviewable policy patch");
    }
    if missing.is_empty() {
        missing.push("none for advisory gate_candidate review; required_gate still needs approval");
    }
    missing.into_iter().map(str::to_string).collect()
}

fn next_commands(
    config_path: &Path,
    baseline: &BaselineDoctorRow,
    signal: &SignalDoctorRow,
    recommended: ReviewPolicyPosture,
) -> Vec<String> {
    let mut commands = vec![
        check_command(
            config_path,
            Some(&baseline.bench),
            baseline.maturity != BaselineMaturity::Missing,
        ),
        format!(
            "perfgate baseline doctor --config {} --bench {}",
            config_path.display(),
            baseline.bench
        ),
        format!(
            "perfgate doctor signal --config {} --bench {}",
            config_path.display(),
            baseline.bench
        ),
        format!(
            "perfgate policy doctor --config {} --bench {}",
            config_path.display(),
            baseline.bench
        ),
    ];

    match baseline.maturity {
        BaselineMaturity::Missing => commands.push(format!(
            "perfgate baseline promote --config {} --bench {}",
            config_path.display(),
            baseline.bench
        )),
        BaselineMaturity::HighNoise => {
            commands.push(format!(
                "perfgate calibrate --config {} --bench {} --emit-patch",
                config_path.display(),
                baseline.bench
            ));
            commands.push(paired_command(Some(&baseline.bench)));
        }
        BaselineMaturity::HostMismatched | BaselineMaturity::Stale => {
            commands.push("rerun on the intended runner class and review refreshed proof".into());
        }
        BaselineMaturity::New
        | BaselineMaturity::Immature
        | BaselineMaturity::Mature
        | BaselineMaturity::Remote => {}
    }

    if recommended == ReviewPolicyPosture::GateCandidate {
        commands.push(format!(
            "perfgate policy emit-patch --config {} --bench {} --to gate_candidate",
            config_path.display(),
            baseline.bench
        ));
    }
    if signal.recommendation == SignalRecommendation::UsePairedMode {
        commands.push(paired_command(Some(&baseline.bench)));
    }

    commands.sort();
    commands.dedup();
    commands
}

fn human_review_required(
    baseline: &BaselineDoctorRow,
    signal: &SignalDoctorRow,
    recommended: ReviewPolicyPosture,
    evidence_source: &EvidenceSourceSummary,
) -> Vec<String> {
    let mut required = Vec::new();
    if baseline.maturity == BaselineMaturity::Missing {
        required.push("baseline promotion");
    }
    if matches!(
        baseline.maturity,
        BaselineMaturity::HighNoise | BaselineMaturity::HostMismatched | BaselineMaturity::Stale
    ) {
        required.push("evidence repair before policy promotion");
    }
    if signal.recommendation == SignalRecommendation::UsePairedMode {
        required.push("paired-mode or calibration review");
    }
    if recommended == ReviewPolicyPosture::GateCandidate {
        required.push("required_gate approval");
    }
    if evidence_source.sample_model == "summary_only" {
        required.push("summary-only evidence review before blocking");
    }
    required.push("threshold changes");
    required.push("tradeoff acceptance");
    required.push("server ledger requirement");
    required.into_iter().map(str::to_string).collect()
}

fn non_inferences(evidence_source: &EvidenceSourceSummary) -> Vec<String> {
    let mut items = vec![
        "this command did not change config, baselines, thresholds, policy, or server settings"
            .to_string(),
        "advisory output does not approve required_gate".to_string(),
        "missing baseline is setup, not a regression".to_string(),
        "server ledger history is optional team history, not local correctness".to_string(),
    ];
    for limit in &evidence_source.limitations {
        items.push(limit.clone());
    }
    items
}

fn artifact_summaries(
    signal: &SignalDoctorRow,
    out_dir: &Path,
    bench_name: &str,
) -> Vec<ArtifactSummary> {
    vec![
        artifact("run", &signal.run_path, signal.run_found),
        artifact("baseline", &signal.baseline_path, signal.baseline_found),
        artifact("compare", &signal.compare_path, signal.compare_found),
        artifact(
            "report",
            &artifact_path(out_dir, bench_name, "report.json"),
            artifact_path(out_dir, bench_name, "report.json").exists(),
        ),
        artifact(
            "comment",
            &artifact_path(out_dir, bench_name, "comment.md"),
            artifact_path(out_dir, bench_name, "comment.md").exists(),
        ),
        artifact(
            "repair_context",
            &artifact_path(out_dir, bench_name, "repair_context.json"),
            artifact_path(out_dir, bench_name, "repair_context.json").exists(),
        ),
    ]
}

fn artifact(label: &str, path: &Path, exists: bool) -> ArtifactSummary {
    ArtifactSummary {
        label: label.to_string(),
        path: path.display().to_string(),
        exists,
    }
}

fn artifact_path(out_dir: &Path, bench_name: &str, filename: &str) -> PathBuf {
    let per_bench = out_dir.join(bench_name).join(filename);
    if per_bench.exists() {
        per_bench
    } else {
        out_dir.join(filename)
    }
}
