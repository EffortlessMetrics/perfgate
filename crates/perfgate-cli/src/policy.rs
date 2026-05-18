//! Policy rollout metadata and advisory policy surfaces.

use clap::{Subcommand, ValueEnum};

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
