use anyhow::Context;
use clap::ValueEnum;

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

pub fn policy_profiles() -> &'static [PolicyProfile] {
    POLICY_PROFILES
}

pub fn policy_profile(name: PolicyProfileName) -> anyhow::Result<&'static PolicyProfile> {
    POLICY_PROFILES
        .iter()
        .find(|profile| profile.name == name.as_str())
        .with_context(|| format!("missing policy profile catalog entry for {}", name.as_str()))
}

pub fn render_policy_profiles(filter: Option<PolicyProfileName>) -> anyhow::Result<String> {
    let mut out = String::new();
    out.push_str("Policy profiles are reviewable starting points, not automatic enforcement.\n");
    out.push_str("They do not promote baselines, loosen thresholds, or make checks blocking.\n");

    let profiles: Vec<&PolicyProfile> = match filter {
        Some(name) => vec![policy_profile(name)?],
        None => policy_profiles().iter().collect(),
    };

    for (index, profile) in profiles.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        render_profile(&mut out, profile);
    }

    Ok(out)
}

fn render_profile(out: &mut String, profile: &PolicyProfile) {
    out.push_str(&format!("\nProfile: {}\n", profile.name));
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
        out.push_str(&format!("  - {}\n", item));
    }
}

const POLICY_PROFILES: &[PolicyProfile] = include!("policy_profiles_data.in");
