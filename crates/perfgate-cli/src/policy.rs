//! Policy rollout metadata and advisory policy surfaces.

use clap::{Args, Subcommand, ValueEnum};
use perfgate_types::ConfigFile;
use perfgate_types::config::load_config_file;
use perfgate_types::error::ConfigValidationError;
use std::path::{Path, PathBuf};

use crate::baseline_doctor::{
    BaselineDoctorRow, BaselineMaturity, configured_benches, inspect_baseline,
};
use crate::doctor::{SignalDoctorRow, SignalRecommendation, inspect_signal, plural};
use crate::{check_command, paired_command, resolve_configured_out_dir};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum PolicyProfileName {
    #[value(name = "rust-cli-standard")]
    RustCliStandard,
    #[value(name = "rust-workspace-advisory")]
    RustWorkspaceAdvisory,
    #[value(name = "node-command-advisory")]
    NodeCommandAdvisory,
    #[value(name = "python-command-advisory")]
    PythonCommandAdvisory,
    #[value(name = "http-local-smoke")]
    HttpLocalSmoke,
    #[value(name = "generic-command-advisory")]
    GenericCommandAdvisory,
    #[value(name = "agent-heavy-repo")]
    AgentHeavyRepo,
    #[value(name = "server-ledger-optional")]
    ServerLedgerOptional,
}

impl PolicyProfileName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RustCliStandard => "rust-cli-standard",
            Self::RustWorkspaceAdvisory => "rust-workspace-advisory",
            Self::NodeCommandAdvisory => "node-command-advisory",
            Self::PythonCommandAdvisory => "python-command-advisory",
            Self::HttpLocalSmoke => "http-local-smoke",
            Self::GenericCommandAdvisory => "generic-command-advisory",
            Self::AgentHeavyRepo => "agent-heavy-repo",
            Self::ServerLedgerOptional => "server-ledger-optional",
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum PolicyAction {
    /// List reviewable policy rollout profiles without changing config.
    Profiles {
        /// Show one profile instead of the full catalog.
        #[arg(long)]
        profile: Option<PolicyProfileName>,
    },

    /// Report advisory policy promotion readiness without changing config.
    Doctor(PolicyDoctorArgs),
}

#[derive(Debug, Args)]
pub struct PolicyDoctorArgs {
    /// Path to the config file (TOML or JSON).
    #[arg(long, default_value = "perfgate.toml")]
    pub config: PathBuf,

    /// Output directory containing recent artifacts. Defaults to [defaults].out_dir or artifacts/perfgate.
    #[arg(long, value_name = "DIR")]
    pub out_dir: Option<PathBuf>,

    /// Limit promotion readiness output to one configured benchmark.
    #[arg(long)]
    pub bench: Option<String>,
}

#[derive(Debug)]
pub struct PolicyProfile {
    pub name: &'static str,
    pub starting_posture: &'static str,
    pub summary: &'static str,
    pub promotion_requirements: &'static [&'static str],
    pub evidence_expectations: &'static [&'static str],
    pub known_bad_fits: &'static [&'static str],
    pub failure_meaning: &'static str,
    pub not_to_infer: &'static [&'static str],
}

const POLICY_PROFILES: &[PolicyProfile] = &[
    PolicyProfile {
        name: "rust-cli-standard",
        starting_posture: "advisory, then gate_candidate for one fast command",
        summary: "Small Rust CLI repos with fast, reproducible command workloads.",
        promotion_requirements: &[
            "baseline mature for the CLI command",
            "signal stable on the intended CI host",
            "calibration reviewed before required gating",
            "reviewer can reproduce with perfgate check",
        ],
        evidence_expectations: &[
            "fast command benchmark or help/startup smoke",
            "low to medium noise after warmup",
            "local artifacts committed only after review",
        ],
        known_bad_fits: &[
            "compile-heavy commands as required first-hour gates",
            "commands whose runtime mostly measures dependency installation",
        ],
        failure_meaning: "a reviewed CLI workload moved outside policy on a compatible host",
        not_to_infer: &[
            "all CLI commands are safe to block",
            "startup smoke proves steady-state throughput",
        ],
    },
    PolicyProfile {
        name: "rust-workspace-advisory",
        starting_posture: "advisory",
        summary: "Larger Rust workspaces where compile and integration noise can dominate.",
        promotion_requirements: &[
            "workspace command split into reviewable workloads",
            "compile and test setup noise understood",
            "paired mode considered for runner drift",
            "required gates approved per benchmark, not for the whole workspace at once",
        ],
        evidence_expectations: &[
            "advisory broad workspace signal",
            "smaller package or command gates promoted individually",
            "maturity reviewed after multiple CI samples",
        ],
        known_bad_fits: &[
            "making cargo test --workspace a required performance gate before calibration",
            "using compile time as a proxy for runtime behavior without saying so",
        ],
        failure_meaning: "a scoped workspace workload moved outside policy after noise review",
        not_to_infer: &[
            "large workspace checks should block by default",
            "one mature package proves the whole workspace is mature",
        ],
    },
    PolicyProfile {
        name: "node-command-advisory",
        starting_posture: "advisory",
        summary: "Node repositories with dedicated benchmark scripts and fixed inputs.",
        promotion_requirements: &[
            "dedicated benchmark script with stable local input",
            "package manager and dependency setup excluded from the measured workload",
            "JIT or runner variance checked with repeats or paired mode",
        ],
        evidence_expectations: &[
            "node or npm benchmark command",
            "fixed fixture data",
            "advisory posture until signal maturity is proven",
        ],
        known_bad_fits: &[
            "npm install or network setup inside the benchmark command",
            "test suites that mix correctness and performance without isolation",
        ],
        failure_meaning: "a stable script workload moved outside policy after JIT/noise review",
        not_to_infer: &[
            "a package script named bench is stable enough to block",
            "JIT warmup noise is automatically solved",
        ],
    },
    PolicyProfile {
        name: "python-command-advisory",
        starting_posture: "advisory",
        summary: "Python repositories with dedicated benchmark modules or scripts.",
        promotion_requirements: &[
            "dedicated benchmark module or script",
            "interpreter startup impact understood",
            "environment and fixture data controlled",
        ],
        evidence_expectations: &[
            "python script or module benchmark",
            "repeat count reviewed for interpreter and import cost",
            "advisory posture before required gating",
        ],
        known_bad_fits: &[
            "pip install or virtualenv setup inside the measured command",
            "pytest correctness suites treated as performance gates without isolation",
        ],
        failure_meaning: "a controlled Python workload moved outside policy on a compatible host",
        not_to_infer: &[
            "module startup proves hot-path performance",
            "local virtualenv timing matches CI host timing",
        ],
    },
    PolicyProfile {
        name: "http-local-smoke",
        starting_posture: "smoke or advisory",
        summary: "Local HTTP endpoint smoke checks and isolated service benchmarks.",
        promotion_requirements: &[
            "service and dependencies are local or intentionally scoped",
            "startup excluded or measured separately",
            "network and host variance reviewed before gating",
        ],
        evidence_expectations: &[
            "local endpoint smoke or scripted HTTP benchmark",
            "medium to high expected noise until isolated",
            "advisory posture by default",
        ],
        known_bad_fits: &[
            "internet or shared staging service calls",
            "benchmarks dominated by service startup or external dependencies",
        ],
        failure_meaning: "a local service workload moved outside policy after isolation review",
        not_to_infer: &[
            "a health endpoint proves product workload performance",
            "remote service timing is safe to block PRs",
        ],
    },
    PolicyProfile {
        name: "generic-command-advisory",
        starting_posture: "advisory",
        summary: "Language-neutral command benchmarks with explicit local inputs.",
        promotion_requirements: &[
            "command directly measures the intended workload",
            "external services removed or intentionally scoped",
            "baseline and signal maturity proven from receipts",
        ],
        evidence_expectations: &[
            "language-neutral command benchmark",
            "explicit local inputs and artifacts",
            "advisory posture until calibrated",
        ],
        known_bad_fits: &[
            "commands that mix setup, install, tests, and performance in one number",
            "commands whose output cannot be reproduced locally",
        ],
        failure_meaning: "the reviewed command workload moved outside policy",
        not_to_infer: &[
            "unknown noise is acceptable for required gates",
            "a successful command is a mature performance signal",
        ],
    },
    PolicyProfile {
        name: "agent-heavy-repo",
        starting_posture: "advisory with review-required policy changes",
        summary: "Repos where agents inspect receipts and propose repairs or config patches.",
        promotion_requirements: &[
            "repair context identifies failure class and safe next action",
            "policy-changing actions are review-required",
            "agents propose patches instead of weakening thresholds",
        ],
        evidence_expectations: &[
            "repair_context.json or review packet available",
            "do-not guidance visible to agents",
            "advisory posture for agent-suggested policy changes",
        ],
        known_bad_fits: &[
            "allowing agents to promote baselines or loosen thresholds without review",
            "treating server upload failure as local correctness failure",
        ],
        failure_meaning: "evidence needs review; agents may summarize but not weaken policy",
        not_to_infer: &[
            "agents are policy authorities",
            "repair context replaces human review for gate promotion",
        ],
    },
    PolicyProfile {
        name: "server-ledger-optional",
        starting_posture: "advisory ledger history",
        summary: "Teams that want optional decision history without making ledger mode correctness.",
        promotion_requirements: &[
            "local receipts remain the merge correctness contract",
            "server URL, API key, export, retention, and restore path are understood",
            "ledger history is useful to the team before uploads become routine",
        ],
        evidence_expectations: &[
            "optional decision history and audit visibility",
            "backup/restore or export/import proof for the selected store",
            "upload failures handled as advisory unless policy says otherwise",
        ],
        known_bad_fits: &[
            "requiring server mode for first-hour adoption",
            "making ledger availability the default merge correctness contract",
        ],
        failure_meaning: "ledger history is unavailable or divergent; local receipts still decide correctness",
        not_to_infer: &[
            "server ledger is required for perfgate correctness",
            "ledger history proves every benchmark is mature",
        ],
    },
];

pub fn policy_profiles() -> &'static [PolicyProfile] {
    POLICY_PROFILES
}

pub fn policy_profile(name: PolicyProfileName) -> &'static PolicyProfile {
    policy_profiles()
        .iter()
        .find(|profile| profile.name == name.as_str())
        .expect("all PolicyProfileName values have catalog entries")
}

pub fn render_policy_profiles(filter: Option<PolicyProfileName>) -> String {
    let mut out = String::new();
    out.push_str("Policy profiles are reviewable starting points, not automatic enforcement.\n");
    out.push_str("They do not promote baselines, loosen thresholds, or make checks blocking.\n\n");

    let profiles: Vec<&PolicyProfile> = match filter {
        Some(name) => vec![policy_profile(name)],
        None => policy_profiles().iter().collect(),
    };

    for (idx, profile) in profiles.iter().enumerate() {
        if idx > 0 {
            out.push('\n');
        }
        render_profile(&mut out, profile);
    }

    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PolicyPosture {
    Smoke,
    Advisory,
    GateCandidate,
    Quarantined,
}

impl PolicyPosture {
    fn as_str(self) -> &'static str {
        match self {
            Self::Smoke => "smoke",
            Self::Advisory => "advisory",
            Self::GateCandidate => "gate_candidate",
            Self::Quarantined => "quarantined",
        }
    }
}

#[derive(Default)]
struct PolicyDoctorCounts {
    smoke: usize,
    advisory: usize,
    gate_candidate: usize,
    quarantined: usize,
}

impl PolicyDoctorCounts {
    fn record(&mut self, posture: PolicyPosture) {
        match posture {
            PolicyPosture::Smoke => self.smoke += 1,
            PolicyPosture::Advisory => self.advisory += 1,
            PolicyPosture::GateCandidate => self.gate_candidate += 1,
            PolicyPosture::Quarantined => self.quarantined += 1,
        }
    }
}

pub fn execute_policy_doctor(args: PolicyDoctorArgs) -> anyhow::Result<()> {
    let config = load_config_file(&args.config)?;
    config
        .validate()
        .map_err(ConfigValidationError::ConfigFile)?;
    let benches = configured_benches(&config, args.bench.as_deref())?;
    let out_dir = resolve_configured_out_dir(args.out_dir.as_ref(), Some(&config));

    println!("perfgate policy doctor");
    println!("Config: {}", args.config.display());
    println!();

    if benches.is_empty() {
        println!("No benchmarks are configured.");
        println!("Next:");
        println!(
            "  edit {} and add a reviewed [[bench]] command",
            args.config.display()
        );
        println!("Do not:");
        println!(
            "  promote baselines or policy until the benchmark measures the workload you care about"
        );
        println!();
        println!(
            "Advisory only: no config, baseline, threshold, policy, or server setting was changed."
        );
        return Ok(());
    }

    let mut counts = PolicyDoctorCounts::default();
    for bench in &benches {
        let baseline = inspect_baseline(&config, bench)?;
        let signal = inspect_signal(&config, &out_dir, bench)?;
        let recommended = recommended_posture(&baseline, &signal);
        counts.record(recommended);
        print_policy_doctor_row(&config, &args.config, &baseline, &signal, recommended);
    }

    println!();
    println!(
        "Summary: {} gate_candidate, {} advisory, {} smoke, {} quarantined",
        counts.gate_candidate, counts.advisory, counts.smoke, counts.quarantined
    );
    println!();
    println!("Do not:");
    println!("  do not make a benchmark blocking just because it is mature");
    println!("  do not loosen thresholds or promote baselines from this advisory output");
    println!("  do not require server ledger mode for local correctness");
    println!();
    println!(
        "Advisory only: no config, baseline, threshold, policy, or server setting was changed."
    );

    Ok(())
}

fn print_policy_doctor_row(
    config: &ConfigFile,
    config_path: &Path,
    baseline: &BaselineDoctorRow,
    signal: &SignalDoctorRow,
    recommended: PolicyPosture,
) {
    let current = current_posture(baseline);

    println!("bench: {}", baseline.bench);
    println!("current posture: {}", current.as_str());
    println!("recommended posture: {}", recommended.as_str());
    println!("baseline maturity: {}", baseline.maturity.as_str());
    println!("signal confidence: {}", signal.recommendation.as_str());
    println!("host compatibility: {}", host_compatibility(signal));
    println!("calibration status: {}", calibration_status(signal));
    println!("proof freshness: {}", proof_freshness(baseline, signal));
    println!(
        "decision readiness: {}",
        decision_readiness(config, &baseline.bench)
    );
    println!("artifacts:");
    println!(
        "  run: {}{}",
        signal.run_path.display(),
        if signal.run_found { "" } else { " (missing)" }
    );
    if signal.baseline_remote {
        println!(
            "  baseline: {} (remote, not probed)",
            signal.baseline_path.display()
        );
    } else {
        println!(
            "  baseline: {}{}",
            signal.baseline_path.display(),
            if signal.baseline_found {
                ""
            } else {
                " (missing)"
            }
        );
    }
    println!(
        "  compare: {}{}",
        signal.compare_path.display(),
        if signal.compare_found {
            ""
        } else {
            " (missing)"
        }
    );
    println!("why:");
    for reason in policy_reasons(baseline, signal, recommended) {
        println!("  - {reason}");
    }
    println!("missing:");
    for missing in policy_missing_requirements(baseline, signal, recommended) {
        println!("  - {missing}");
    }
    println!("next:");
    for command in policy_next_commands(config_path, baseline, signal, recommended) {
        println!("  {command}");
    }
    println!("do not:");
    for item in policy_do_not(recommended) {
        println!("  - {item}");
    }
    println!();
}

fn current_posture(baseline: &BaselineDoctorRow) -> PolicyPosture {
    match baseline.maturity {
        BaselineMaturity::Missing => PolicyPosture::Smoke,
        _ => PolicyPosture::Advisory,
    }
}

fn recommended_posture(baseline: &BaselineDoctorRow, signal: &SignalDoctorRow) -> PolicyPosture {
    match baseline.maturity {
        BaselineMaturity::Missing => return PolicyPosture::Advisory,
        BaselineMaturity::HostMismatched | BaselineMaturity::Stale => {
            return PolicyPosture::Quarantined;
        }
        BaselineMaturity::HighNoise => return PolicyPosture::Advisory,
        BaselineMaturity::New | BaselineMaturity::Immature | BaselineMaturity::Remote => {
            return PolicyPosture::Advisory;
        }
        BaselineMaturity::Mature => {}
    }

    match signal.recommendation {
        SignalRecommendation::SafeToGate => PolicyPosture::GateCandidate,
        SignalRecommendation::CheckHostMismatch | SignalRecommendation::RefreshBaseline => {
            PolicyPosture::Quarantined
        }
        SignalRecommendation::UsePairedMode
        | SignalRecommendation::AdvisoryOnly
        | SignalRecommendation::IncreaseSamples
        | SignalRecommendation::NoDecisionYet => PolicyPosture::Advisory,
    }
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

fn policy_reasons(
    baseline: &BaselineDoctorRow,
    signal: &SignalDoctorRow,
    recommended: PolicyPosture,
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
        reasons.push(format!("observed CV {}", format_percent(cv)));
    }
    match recommended {
        PolicyPosture::GateCandidate => {
            reasons.push("evidence can be reviewed for gate_candidate, not required_gate".into());
        }
        PolicyPosture::Quarantined => {
            reasons.push(
                "policy should pause until host, freshness, or setup evidence is repaired".into(),
            );
        }
        PolicyPosture::Advisory => {
            reasons.push(
                "evidence should remain advisory until missing requirements are resolved".into(),
            );
        }
        PolicyPosture::Smoke => {
            reasons.push("use this only for setup confidence until evidence exists".into());
        }
    }
    reasons
}

fn policy_missing_requirements(
    baseline: &BaselineDoctorRow,
    signal: &SignalDoctorRow,
    recommended: PolicyPosture,
) -> Vec<&'static str> {
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
    if recommended == PolicyPosture::GateCandidate {
        missing.push("required-gate reviewer approval");
        missing.push("reviewable policy patch");
    }
    if missing.is_empty() {
        missing.push("none for advisory gate_candidate review; required_gate still needs approval");
    }
    missing
}

fn policy_next_commands(
    config_path: &Path,
    baseline: &BaselineDoctorRow,
    signal: &SignalDoctorRow,
    recommended: PolicyPosture,
) -> Vec<String> {
    match baseline.maturity {
        BaselineMaturity::Missing => {
            return vec![
                check_command(config_path, Some(&baseline.bench), false),
                format!(
                    "perfgate baseline promote --config {} --bench {}",
                    config_path.display(),
                    baseline.bench
                ),
            ];
        }
        BaselineMaturity::HighNoise => {
            return vec![
                format!(
                    "perfgate calibrate --config {} --bench {} --emit-patch",
                    config_path.display(),
                    baseline.bench
                ),
                paired_command(Some(&baseline.bench)),
            ];
        }
        BaselineMaturity::HostMismatched | BaselineMaturity::Stale => {
            return vec![
                "rerun on the intended runner class and review the refreshed baseline".to_string(),
                check_command(config_path, Some(&baseline.bench), true),
            ];
        }
        BaselineMaturity::New | BaselineMaturity::Immature | BaselineMaturity::Remote => {}
        BaselineMaturity::Mature => {}
    }

    match (recommended, signal.recommendation) {
        (PolicyPosture::GateCandidate, _) => vec![
            check_command(config_path, Some(&baseline.bench), true),
            format!("review promotion evidence for {}", baseline.bench),
        ],
        (_, SignalRecommendation::UsePairedMode) => vec![
            format!(
                "perfgate calibrate --config {} --bench {} --emit-patch",
                config_path.display(),
                baseline.bench
            ),
            paired_command(Some(&baseline.bench)),
        ],
        (_, SignalRecommendation::NoDecisionYet) => vec![
            check_command(config_path, Some(&baseline.bench), false),
            format!(
                "perfgate baseline doctor --config {} --bench {}",
                config_path.display(),
                baseline.bench
            ),
        ],
        _ => vec![check_command(config_path, Some(&baseline.bench), true)],
    }
}

fn policy_do_not(recommended: PolicyPosture) -> Vec<&'static str> {
    match recommended {
        PolicyPosture::GateCandidate => vec![
            "do not make this a required gate without reviewer approval",
            "do not treat gate_candidate as already blocking",
        ],
        PolicyPosture::Quarantined => vec![
            "do not force this benchmark through CI while evidence is untrustworthy",
            "do not loosen thresholds to hide quarantine reasons",
        ],
        PolicyPosture::Advisory => vec![
            "do not make advisory evidence blocking by default",
            "do not promote baselines or thresholds without review",
        ],
        PolicyPosture::Smoke => vec![
            "do not infer workload performance from setup smoke alone",
            "do not require server ledger mode for local correctness",
        ],
    }
}

fn format_percent(value: f64) -> String {
    format!("{:.1}%", value * 100.0)
}

fn render_profile(out: &mut String, profile: &PolicyProfile) {
    out.push_str(&format!("Profile: {}\n", profile.name));
    out.push_str(&format!("Summary: {}\n", profile.summary));
    out.push_str(&format!("Starting posture: {}\n", profile.starting_posture));
    render_list(
        out,
        "Promotion requirements",
        profile.promotion_requirements,
    );
    render_list(
        out,
        "Default evidence expectations",
        profile.evidence_expectations,
    );
    render_list(out, "Known bad fits", profile.known_bad_fits);
    out.push_str(&format!("Failure meaning: {}\n", profile.failure_meaning));
    render_list(out, "Do not infer", profile.not_to_infer);
}

fn render_list(out: &mut String, label: &str, items: &[&str]) {
    out.push_str(&format!("{label}:\n"));
    for item in items {
        out.push_str(&format!("  - {item}\n"));
    }
}

pub fn execute_policy_action(action: PolicyAction) -> anyhow::Result<()> {
    match action {
        PolicyAction::Profiles { profile } => {
            print!("{}", render_policy_profiles(profile));
            Ok(())
        }
        PolicyAction::Doctor(args) => execute_policy_doctor(args),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_contains_all_initial_profiles() {
        let names: Vec<_> = policy_profiles()
            .iter()
            .map(|profile| profile.name)
            .collect();
        assert_eq!(
            names,
            vec![
                "rust-cli-standard",
                "rust-workspace-advisory",
                "node-command-advisory",
                "python-command-advisory",
                "http-local-smoke",
                "generic-command-advisory",
                "agent-heavy-repo",
                "server-ledger-optional",
            ]
        );
    }

    #[test]
    fn rendered_catalog_preserves_advisory_boundary() {
        let rendered = render_policy_profiles(None);
        assert!(rendered.contains("not automatic enforcement"));
        assert!(rendered.contains("They do not promote baselines"));
        assert!(rendered.contains("Profile: rust-cli-standard"));
        assert!(rendered.contains("Profile: server-ledger-optional"));
        assert!(rendered.contains("server ledger is required for perfgate correctness"));
    }

    #[test]
    fn rendered_single_profile_excludes_other_profiles() {
        let rendered = render_policy_profiles(Some(PolicyProfileName::NodeCommandAdvisory));
        assert!(rendered.contains("Profile: node-command-advisory"));
        assert!(rendered.contains("JIT"));
        assert!(!rendered.contains("Profile: rust-cli-standard"));
    }
}
