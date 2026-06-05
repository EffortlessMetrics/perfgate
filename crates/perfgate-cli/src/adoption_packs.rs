//! Reviewable adoption packs for common repository shapes.

use crate::storage::atomic_write;
use anyhow::{Context, bail};
use clap::{Subcommand, ValueEnum};
use serde::Serialize;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum AdoptionPackName {
    #[value(name = "rust-cli")]
    RustCli,
    #[value(name = "rust-workspace")]
    RustWorkspace,
    #[value(name = "python-service")]
    PythonService,
    #[value(name = "node-tool-action")]
    NodeToolAction,
    #[value(name = "http-local-smoke")]
    HttpLocalSmoke,
    #[value(name = "generic-command")]
    GenericCommand,
}

impl AdoptionPackName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RustCli => "rust-cli",
            Self::RustWorkspace => "rust-workspace",
            Self::PythonService => "python-service",
            Self::NodeToolAction => "node-tool-action",
            Self::HttpLocalSmoke => "http-local-smoke",
            Self::GenericCommand => "generic-command",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum AdoptionCiPlatform {
    #[value(name = "github")]
    Github,
}

#[derive(Debug, Subcommand)]
pub enum AdoptionAction {
    /// Recommend a reviewable adoption pack from repository shape.
    Recommend {
        /// Repository path to inspect.
        #[arg(long, default_value = ".")]
        path: PathBuf,

        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },

    /// Emit reviewable setup artifacts without changing repository files.
    Apply {
        /// Adoption pack to scaffold review artifacts for.
        #[arg(long)]
        pack: AdoptionPackName,

        /// CI platform to include in the dry-run workflow artifact.
        #[arg(long, value_enum, default_value = "github")]
        ci: AdoptionCiPlatform,

        /// Write review artifacts under the output directory without changing setup files.
        #[arg(long)]
        dry_run: bool,

        /// Directory for generated review artifacts.
        #[arg(long, default_value = "target/perfgate-adoption")]
        out_dir: PathBuf,
    },

    /// List reviewable adoption packs without changing config.
    Packs {
        /// Show one adoption pack instead of the full catalog.
        #[arg(long)]
        pack: Option<AdoptionPackName>,
    },
}

#[derive(Debug)]
pub struct AdoptionPack {
    pub name: &'static str,
    pub repo_shape: &'static str,
    pub start_with: &'static str,
    pub benchmark_source: &'static str,
    pub evidence_intake: &'static str,
    pub expected_artifacts: &'static [&'static str],
    pub starting_policy: &'static str,
    pub action_posture: &'static str,
    pub local_reproduction: &'static [&'static str],
    pub promotion_path: &'static [&'static str],
    pub known_bad_fits: &'static [&'static str],
    pub do_not_infer: &'static [&'static str],
}

#[derive(Debug, Serialize)]
pub struct AdoptionRecommendation {
    pub recommended_pack: &'static str,
    pub confidence: &'static str,
    pub why: Vec<String>,
    pub inspected: Vec<String>,
    pub not_inspected: Vec<&'static str>,
    pub known_bad_fits: Vec<&'static str>,
    pub next_command: String,
}

#[derive(Debug, Clone)]
pub struct AdoptionApplyArtifacts {
    pub config_patch: String,
    pub github_workflow: String,
    pub local_commands: String,
    pub non_inferences: String,
}

const ADOPTION_PACKS: &[AdoptionPack] = &[
    AdoptionPack {
        name: "rust-cli",
        repo_shape: "Small Rust CLI with one or two fast command workloads.",
        start_with: "perfgate init --ci github --profile standard --suggest-benches rust-cli-smoke",
        benchmark_source: "native perfgate check first; Criterion or hyperfine import when a dedicated benchmark already exists",
        evidence_intake: "perfgate check for first-party command receipts, or perfgate ingest --format criterion/hyperfine for existing benchmark artifacts",
        expected_artifacts: &[
            "artifacts/perfgate/<bench>/run.json",
            "artifacts/perfgate/<bench>/compare.json",
            "artifacts/perfgate/<bench>/report.json",
            "artifacts/perfgate/<bench>/comment.md",
            "artifacts/perfgate/<bench>/repair_context.json",
        ],
        starting_policy: "advisory, then gate_candidate for one mature fast command",
        action_posture: "GitHub Action may block only after baseline and signal maturity are reviewed",
        local_reproduction: &[
            "perfgate check --config perfgate.toml --bench <bench>",
            "perfgate policy review-packet --config perfgate.toml --bench <bench>",
        ],
        promotion_path: &[
            "prove repeated local and CI signal is stable",
            "review perfgate baseline doctor and perfgate doctor signal output",
            "emit a non-mutating policy patch before making the benchmark blocking",
        ],
        known_bad_fits: &[
            "using compile-heavy cargo run --release as the first required gate",
            "treating --help startup smoke as parser or throughput proof",
        ],
        do_not_infer: &[
            "one fast CLI check proves all CLI paths are mature",
            "Criterion confidence intervals replace perfgate maturity policy",
            "server ledger mode is required for correctness",
        ],
    },
    AdoptionPack {
        name: "rust-workspace",
        repo_shape: "Larger Rust workspace where compile, test, and integration setup can dominate.",
        start_with: "perfgate init --ci github --profile standard --suggest-benches rust-workspace-advisory",
        benchmark_source: "advisory workspace command plus smaller package or Criterion imports for gate candidates",
        evidence_intake: "perfgate check for scoped commands, perfgate ingest --format criterion for benchmark artifacts, or hyperfine for command timing",
        expected_artifacts: &[
            "artifacts/perfgate/<workspace-smoke>/run.json",
            "artifacts/perfgate/<package-bench>/compare.json",
            "artifacts/perfgate/<package-bench>/review-packet.md",
        ],
        starting_policy: "advisory until compile/setup noise is separated from workload movement",
        action_posture: "Action summary should explain advisory workspace signal separately from required package gates",
        local_reproduction: &[
            "perfgate check --config perfgate.toml --bench <package-bench>",
            "perfgate doctor signal --config perfgate.toml --bench <package-bench>",
            "perfgate policy doctor --config perfgate.toml --bench <package-bench>",
        ],
        promotion_path: &[
            "keep broad workspace timing advisory",
            "promote one scoped workload at a time",
            "consider paired mode when runner drift changes the verdict",
        ],
        known_bad_fits: &[
            "making cargo test --workspace a required performance gate before calibration",
            "using a broad command with no workload owner",
        ],
        do_not_infer: &[
            "a mature package benchmark proves the whole workspace",
            "compile-heavy command timing is runtime proof",
            "busy-runner fallback proves self-hosted runner performance",
        ],
    },
    AdoptionPack {
        name: "python-service",
        repo_shape: "Python service or library with pytest-benchmark or a dedicated benchmark script.",
        start_with: "perfgate init --ci github --profile standard --suggest-benches python-command",
        benchmark_source: "pytest-benchmark JSON when available; otherwise a stable python benchmark command",
        evidence_intake: "perfgate ingest --format pytest, or perfgate ingest --format custom-json/custom-csv with explicit metric mapping",
        expected_artifacts: &[
            "artifacts/pytest-benchmark.json",
            "artifacts/perfgate/<bench>/run.json",
            "artifacts/perfgate/<bench>/repair_context.json",
        ],
        starting_policy: "advisory until interpreter startup, dependency setup, and sample model are understood",
        action_posture: "Action summary should keep correctness-test failures separate from performance maturity",
        local_reproduction: &[
            "pytest --benchmark-json=artifacts/pytest-benchmark.json",
            "perfgate ingest --format pytest --input artifacts/pytest-benchmark.json --out artifacts/perfgate/<bench>/run.json",
            "perfgate policy review-packet --config perfgate.toml --bench <bench>",
        ],
        promotion_path: &[
            "prefer raw benchmark samples over summary-only evidence",
            "review Python/runtime limits in the imported evidence output",
            "promote only deterministic service workloads",
        ],
        known_bad_fits: &[
            "package installation or network startup inside the timed command",
            "treating passing pytest correctness tests as performance maturity",
        ],
        do_not_infer: &[
            "summary-only pytest evidence has full noise support",
            "unknown host context is compatible",
            "first imported result should become a baseline",
        ],
    },
    AdoptionPack {
        name: "node-tool-action",
        repo_shape: "Node CLI, tool, or GitHub Action with scriptable local benchmarks.",
        start_with: "perfgate init --ci github --profile standard --suggest-benches node-command",
        benchmark_source: "dedicated node benchmark script, hyperfine command timing, or custom JSON/CSV output",
        evidence_intake: "perfgate check for a stable script, hyperfine import for command timing, or custom mapping for tool-specific metrics",
        expected_artifacts: &[
            "artifacts/node-bench.json",
            "artifacts/perfgate/<bench>/run.json",
            "artifacts/perfgate/<bench>/comment.md",
        ],
        starting_policy: "advisory until JIT warmup, dependency cache, and runner variance are reviewed",
        action_posture: "Action summary should show advisory signal before any workflow starts blocking",
        local_reproduction: &[
            "node scripts/bench.js",
            "perfgate ingest --format custom-json --input artifacts/node-bench.json --metric wall_ms=duration_ms,unit=ms,direction=lower_is_better --out artifacts/perfgate/<bench>/run.json",
            "perfgate doctor signal --config perfgate.toml --bench <bench>",
        ],
        promotion_path: &[
            "separate install/build time from measured tool runtime",
            "calibrate after warmup behavior is visible",
            "promote only scripts with fixed local input",
        ],
        known_bad_fits: &[
            "npm install or package download inside the timed path",
            "benchmarks that call shared network services",
        ],
        do_not_infer: &[
            "a fast action smoke proves all workflow paths",
            "JIT-sensitive evidence is stable without repeated samples",
            "custom imports infer metric meaning without explicit mapping",
        ],
    },
    AdoptionPack {
        name: "http-local-smoke",
        repo_shape: "Local HTTP service smoke or load-test path with isolated dependencies.",
        start_with: "perfgate init --ci github --profile standard --suggest-benches http-smoke",
        benchmark_source: "local curl/script smoke first; k6 summary JSON when the team already has load-test output",
        evidence_intake: "perfgate check for local endpoint smoke, or perfgate ingest --format k6 for summary JSON",
        expected_artifacts: &[
            "artifacts/k6-summary.json",
            "artifacts/perfgate/<bench>/run.json",
            "artifacts/perfgate/<bench>/review-packet.md",
        ],
        starting_policy: "smoke or advisory; candidate policy only for isolated local endpoints",
        action_posture: "Action summary should say when HTTP evidence is not production capacity proof",
        local_reproduction: &[
            "k6 run --summary-export artifacts/k6-summary.json scripts/http-smoke.js",
            "perfgate ingest --format k6 --input artifacts/k6-summary.json --name <bench> --out artifacts/perfgate/<bench>/run.json",
            "perfgate policy review-packet --config perfgate.toml --bench <bench>",
        ],
        promotion_path: &[
            "keep shared or internet-backed endpoints advisory",
            "use paired mode or repeat evidence when runner networking dominates",
            "treat throughput/memory tradeoffs as structured decision candidates",
        ],
        known_bad_fits: &[
            "shared staging services as required PR gates",
            "treating local k6 output as production capacity evidence",
        ],
        do_not_infer: &[
            "successful HTTP smoke proves load capacity",
            "summary-only k6 output has raw per-request samples",
            "server ledger history is part of local correctness",
        ],
    },
    AdoptionPack {
        name: "generic-command",
        repo_shape: "Language-neutral repository with an existing benchmark command or artifact.",
        start_with: "perfgate init --ci github --profile standard --suggest-benches generic-command",
        benchmark_source: "stable local command, generic command JSON, or custom JSON/CSV with explicit metric mapping",
        evidence_intake: "perfgate check for direct commands, perfgate ingest --format generic-command-json, or custom-json/custom-csv for existing artifacts",
        expected_artifacts: &[
            "artifacts/source-evidence.json",
            "artifacts/perfgate/<bench>/run.json",
            "artifacts/perfgate/<bench>/repair_context.json",
        ],
        starting_policy: "advisory until unit, direction, host context, and sample model are explicit",
        action_posture: "Action summary should show source limits and the exact local reproduction command",
        local_reproduction: &[
            "./scripts/bench.sh > artifacts/source-evidence.json",
            "perfgate ingest --format generic-command-json --input artifacts/source-evidence.json --out artifacts/perfgate/<bench>/run.json",
            "perfgate baseline doctor --config perfgate.toml --bench <bench>",
        ],
        promotion_path: &[
            "fail closed on ambiguous units or metric direction",
            "prefer raw samples before required gates",
            "emit a reviewable policy patch instead of changing gates silently",
        ],
        known_bad_fits: &[
            "commands with mutable external data",
            "artifacts that omit metric unit or direction",
        ],
        do_not_infer: &[
            "generic import knows what a metric means without mapping",
            "missing host fields prove compatibility",
            "advisory evidence should block CI by default",
        ],
    },
];

pub fn adoption_packs() -> &'static [AdoptionPack] {
    ADOPTION_PACKS
}

pub fn adoption_pack(name: AdoptionPackName) -> anyhow::Result<&'static AdoptionPack> {
    adoption_packs()
        .iter()
        .find(|pack| pack.name == name.as_str())
        .with_context(|| format!("missing adoption pack catalog entry for {}", name.as_str()))
}

pub fn recommend_adoption_pack(root: &Path) -> anyhow::Result<AdoptionRecommendation> {
    let mut inspected = Vec::new();

    let cargo_toml = path_exists(root, "Cargo.toml", &mut inspected);
    let rust_main = path_exists(root, "src/main.rs", &mut inspected);
    let rust_lib = path_exists(root, "src/lib.rs", &mut inspected);
    let rust_benches = path_exists(root, "benches", &mut inspected);
    let pytest_ini = path_exists(root, "pytest.ini", &mut inspected);
    let pyproject = path_exists(root, "pyproject.toml", &mut inspected);
    let python_benchmarks = path_exists(root, "benchmarks", &mut inspected);
    let package_json = path_exists(root, "package.json", &mut inspected);
    let action_yml = path_exists(root, "action.yml", &mut inspected);
    let action_yaml = path_exists(root, "action.yaml", &mut inspected);
    let scripts_dir = path_exists(root, "scripts", &mut inspected);

    let cargo_is_workspace = cargo_toml && file_contains(root.join("Cargo.toml"), "[workspace]");
    if cargo_is_workspace {
        inspected.push("Cargo.toml contains [workspace]".to_string());
    }

    let has_k6_script = scripts_dir && directory_contains_name(root.join("scripts"), "k6");
    if has_k6_script {
        inspected.push("scripts/ contains a k6-named file".to_string());
    }

    let has_bench_script = scripts_dir && directory_contains_name(root.join("scripts"), "bench");
    if has_bench_script {
        inspected.push("scripts/ contains a bench-named file".to_string());
    }

    let (pack_name, confidence, why) = if cargo_is_workspace || (cargo_toml && rust_benches) {
        (
            AdoptionPackName::RustWorkspace,
            "high",
            vec![reason(
                "Cargo workspace or benches indicate a larger Rust workspace",
            )],
        )
    } else if cargo_toml && (rust_main || rust_lib) {
        (
            AdoptionPackName::RustCli,
            "high",
            vec![reason(
                "Cargo.toml plus src/main.rs or src/lib.rs indicate a Rust CLI/library shape",
            )],
        )
    } else if (pytest_ini || pyproject) && python_benchmarks {
        (
            AdoptionPackName::PythonService,
            "medium",
            vec![reason(
                "Python project metadata plus benchmarks/ suggests pytest-benchmark or a benchmark script",
            )],
        )
    } else if package_json && (action_yml || action_yaml) {
        (
            AdoptionPackName::NodeToolAction,
            "high",
            vec![reason(
                "package.json plus action.yml/action.yaml indicates a Node tool or GitHub Action",
            )],
        )
    } else if has_k6_script {
        (
            AdoptionPackName::HttpLocalSmoke,
            "medium",
            vec![reason(
                "scripts/ contains a k6-named file, which fits local HTTP smoke intake",
            )],
        )
    } else if has_bench_script {
        (
            AdoptionPackName::GenericCommand,
            "medium",
            vec![reason(
                "scripts/ contains a bench-named file, but the framework is not known",
            )],
        )
    } else {
        (
            AdoptionPackName::GenericCommand,
            "low",
            vec![reason(
                "no known framework markers were found; start with explicit generic command evidence",
            )],
        )
    };

    let pack = adoption_pack(pack_name)?;
    Ok(AdoptionRecommendation {
        recommended_pack: pack.name,
        confidence,
        why,
        inspected,
        not_inspected: vec![
            "benchmark runtime behavior",
            "baseline maturity",
            "signal noise",
            "host compatibility",
            "whether any generated setup should be written",
        ],
        known_bad_fits: pack.known_bad_fits.to_vec(),
        next_command: format!("perfgate adoption packs --pack {}", pack.name),
    })
}

pub fn render_adoption_recommendation(recommendation: &AdoptionRecommendation) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Recommended adoption pack: {}",
        recommendation.recommended_pack
    );
    let _ = writeln!(out, "Confidence: {}", recommendation.confidence);
    render_owned_list(&mut out, "Why", &recommendation.why);
    render_owned_list(&mut out, "Inspected", &recommendation.inspected);
    render_list(&mut out, "Not inspected", &recommendation.not_inspected);
    render_list(&mut out, "Known bad fits", &recommendation.known_bad_fits);
    let _ = writeln!(out, "Next: {}", recommendation.next_command);
    out
}

pub fn render_adoption_apply_artifacts(
    pack: &AdoptionPack,
    _ci: AdoptionCiPlatform,
) -> AdoptionApplyArtifacts {
    let starter = starter_bench(pack.name);
    AdoptionApplyArtifacts {
        config_patch: render_config_patch(pack, starter),
        github_workflow: render_github_workflow(),
        local_commands: render_local_commands(pack),
        non_inferences: render_non_inferences(pack),
    }
}

pub fn render_adoption_packs(filter: Option<AdoptionPackName>) -> anyhow::Result<String> {
    let mut out = String::new();
    out.push_str(
        "Adoption packs are reviewable starting points for existing benchmark ecosystems.\n",
    );
    out.push_str(
        "They do not detect benchmarks magically, promote baselines, make checks blocking, loosen thresholds, or require server ledger mode.\n\n",
    );

    let packs: Vec<&AdoptionPack> = match filter {
        Some(name) => vec![adoption_pack(name)?],
        None => adoption_packs().iter().collect(),
    };

    for (idx, pack) in packs.iter().enumerate() {
        if idx > 0 {
            out.push('\n');
        }
        render_pack(&mut out, pack);
    }

    Ok(out)
}

fn render_config_patch(pack: &AdoptionPack, starter: StarterBench) -> String {
    let command = starter
        .command
        .iter()
        .map(|part| format!("\"{part}\""))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        r#"# Generated by `perfgate adoption apply --dry-run`.
# Review before applying. This patch does not promote baselines, loosen thresholds,
# make a required gate, or require server ledger mode.
--- /dev/null
+++ b/perfgate.toml
@@
+# Pack: {pack_name}
+# Repo shape: {repo_shape}
+# Starting policy: {starting_policy}
+[defaults]
+repeat = 7
+warmup = 1
+threshold = 0.20
+warn_factor = 0.50
+noise_threshold = 0.10
+noise_policy = "warn"
+out_dir = "artifacts/perfgate"
+baseline_dir = "baselines"
+
+# Review and replace this starter benchmark before relying on it.
+# Best current fit: {start_with}
+[[bench]]
+name = "{bench_name}"
+command = [{command}]
"#,
        pack_name = pack.name,
        repo_shape = pack.repo_shape,
        starting_policy = pack.starting_policy,
        start_with = pack.start_with,
        bench_name = starter.name,
    )
}

fn render_github_workflow() -> String {
    r#"# Generated by `perfgate adoption apply --dry-run`.
# Review before copying to .github/workflows/perfgate.yml.
name: Performance Evidence

on:
  pull_request:
    branches: [main]
  push:
    branches: [main]

permissions:
  contents: read
  pull-requests: write

jobs:
  perfgate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6

      - uses: EffortlessMetrics/perfgate@v0
        with:
          config: perfgate.toml
          all: "true"
          require_baseline: "false"
          upload_artifact: "true"
"#
    .to_string()
}

fn render_local_commands(pack: &AdoptionPack) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Local adoption commands");
    let _ = writeln!(out);
    let _ = writeln!(out, "Pack: `{}`", pack.name);
    let _ = writeln!(out);
    let _ = writeln!(out, "Run after reviewing the generated patch and workflow:");
    let _ = writeln!(out);
    let _ = writeln!(out, "```bash");
    let _ = writeln!(out, "perfgate adoption packs --pack {}", pack.name);
    let _ = writeln!(out, "perfgate check --config perfgate.toml --all");
    let _ = writeln!(out, "perfgate baseline doctor --config perfgate.toml --all");
    let _ = writeln!(out, "perfgate doctor signal --config perfgate.toml --all");
    let _ = writeln!(
        out,
        "perfgate policy review-packet --config perfgate.toml --bench <bench> --out artifacts/perfgate/<bench>/review-packet.md"
    );
    let _ = writeln!(out, "```");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Do not promote baselines or make gates blocking until maturity output has been reviewed."
    );
    out
}

fn render_non_inferences(pack: &AdoptionPack) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Non-inferences");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "These dry-run artifacts are setup review material for `{}`.",
        pack.name
    );
    let _ = writeln!(out, "They do not prove:");
    let _ = writeln!(out);
    render_list(
        &mut out,
        "Universal limits",
        &[
            "the selected benchmark is mature",
            "the first run should become a baseline",
            "signal noise is low enough to block CI",
            "host compatibility has been proven",
            "policy should become required_gate",
            "server ledger history is required for correctness",
        ],
    );
    render_list(&mut out, "Pack-specific limits", pack.do_not_infer);
    out
}

fn render_pack(out: &mut String, pack: &AdoptionPack) {
    let _ = writeln!(out, "Pack: {}", pack.name);
    let _ = writeln!(out, "Repo shape: {}", pack.repo_shape);
    let _ = writeln!(out, "Start with: {}", pack.start_with);
    let _ = writeln!(out, "Benchmark source: {}", pack.benchmark_source);
    let _ = writeln!(out, "Evidence intake: {}", pack.evidence_intake);
    render_list(out, "Expected artifacts", pack.expected_artifacts);
    let _ = writeln!(out, "Starting policy: {}", pack.starting_policy);
    let _ = writeln!(out, "Action posture: {}", pack.action_posture);
    render_list(out, "Local reproduction", pack.local_reproduction);
    render_list(out, "Promotion path", pack.promotion_path);
    render_list(out, "Known bad fits", pack.known_bad_fits);
    render_list(out, "Do not infer", pack.do_not_infer);
}

fn render_list(out: &mut String, label: &str, items: &[&str]) {
    let _ = writeln!(out, "{label}:");
    for item in items {
        let _ = writeln!(out, "  - {item}");
    }
}

fn render_owned_list(out: &mut String, label: &str, items: &[String]) {
    let _ = writeln!(out, "{label}:");
    for item in items {
        let _ = writeln!(out, "  - {item}");
    }
}

#[derive(Debug, Clone, Copy)]
struct StarterBench {
    name: &'static str,
    command: &'static [&'static str],
}

fn starter_bench(pack_name: &str) -> StarterBench {
    match pack_name {
        "rust-cli" => StarterBench {
            name: "cli-help",
            command: &["cargo", "run", "-q", "--", "--help"],
        },
        "rust-workspace" => StarterBench {
            name: "workspace-smoke",
            command: &["cargo", "test", "-p", "your-package", "--no-fail-fast"],
        },
        "python-service" => StarterBench {
            name: "python-bench",
            command: &["python", "scripts/bench.py"],
        },
        "node-tool-action" => StarterBench {
            name: "node-bench",
            command: &["node", "scripts/bench.js"],
        },
        "http-local-smoke" => StarterBench {
            name: "http-health",
            command: &["curl", "-fsS", "http://127.0.0.1:8080/health"],
        },
        _ => StarterBench {
            name: "command-smoke",
            command: &["./scripts/bench.sh"],
        },
    }
}

fn path_exists(root: &Path, relative: &str, inspected: &mut Vec<String>) -> bool {
    let exists = root.join(relative).exists();
    inspected.push(format!(
        "{} {}",
        if exists { "found" } else { "missing" },
        relative
    ));
    exists
}

fn file_contains(path: PathBuf, needle: &str) -> bool {
    std::fs::read_to_string(path)
        .map(|content| content.contains(needle))
        .unwrap_or(false)
}

fn directory_contains_name(path: PathBuf, needle: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(path) else {
        return false;
    };

    entries.filter_map(Result::ok).any(|entry| {
        entry
            .file_name()
            .to_string_lossy()
            .to_ascii_lowercase()
            .contains(needle)
    })
}

fn reason(value: &str) -> String {
    value.to_string()
}

fn write_adoption_apply_artifacts(
    out_dir: &Path,
    artifacts: &AdoptionApplyArtifacts,
) -> anyhow::Result<Vec<PathBuf>> {
    std::fs::create_dir_all(out_dir).with_context(|| format!("create {}", out_dir.display()))?;
    let files = [
        ("perfgate.toml.patch", &artifacts.config_patch),
        ("github-workflow.yml", &artifacts.github_workflow),
        ("local-commands.md", &artifacts.local_commands),
        ("non-inferences.md", &artifacts.non_inferences),
    ];
    let mut written = Vec::with_capacity(files.len());
    for (name, content) in files {
        let path = out_dir.join(name);
        atomic_write(&path, content.as_bytes())
            .with_context(|| format!("write {}", path.display()))?;
        written.push(path);
    }
    Ok(written)
}

pub fn execute_adoption_action(action: AdoptionAction) -> anyhow::Result<()> {
    match action {
        AdoptionAction::Recommend { path, json } => {
            let root = path.canonicalize().with_context(|| {
                format!(
                    "failed to resolve adoption recommend path: {}",
                    path.display()
                )
            })?;
            if !root.is_dir() {
                bail!(
                    "adoption recommend path must be a directory: {}",
                    root.display()
                );
            }

            let recommendation = recommend_adoption_pack(&root)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&recommendation)?);
            } else {
                print!("{}", render_adoption_recommendation(&recommendation));
            }
            Ok(())
        }
        AdoptionAction::Apply {
            pack,
            ci,
            dry_run,
            out_dir,
        } => {
            if !dry_run {
                bail!(
                    "adoption apply is non-mutating in this lane; rerun with --dry-run to emit review artifacts"
                );
            }

            let pack = adoption_pack(pack)?;
            let artifacts = render_adoption_apply_artifacts(pack, ci);
            let written = write_adoption_apply_artifacts(&out_dir, &artifacts)?;
            println!(
                "Dry-run adoption artifacts written to {}",
                out_dir.display()
            );
            for path in written {
                println!("  - {}", path.display());
            }
            println!(
                "No perfgate.toml, workflow, baselines, thresholds, required gates, or server ledger settings were changed."
            );
            Ok(())
        }
        AdoptionAction::Packs { pack } => {
            print!("{}", render_adoption_packs(pack)?);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_contains_planned_adoption_packs() {
        let names: Vec<_> = adoption_packs().iter().map(|pack| pack.name).collect();
        assert_eq!(
            names,
            vec![
                "rust-cli",
                "rust-workspace",
                "python-service",
                "node-tool-action",
                "http-local-smoke",
                "generic-command",
            ]
        );
    }

    #[test]
    fn rendered_catalog_preserves_review_boundaries() -> anyhow::Result<()> {
        let rendered = render_adoption_packs(None)?;
        assert!(rendered.contains("reviewable starting points"));
        assert!(rendered.contains("do not detect benchmarks magically"));
        assert!(rendered.contains("promote baselines"));
        assert!(rendered.contains("require server ledger mode"));
        assert!(rendered.contains("Pack: rust-cli"));
        assert!(rendered.contains("Pack: generic-command"));
        assert!(rendered.contains("Local reproduction:"));
        assert!(rendered.contains("Do not infer:"));
        Ok(())
    }

    #[test]
    fn rendered_single_pack_excludes_other_packs() -> anyhow::Result<()> {
        let rendered = render_adoption_packs(Some(AdoptionPackName::HttpLocalSmoke))?;
        assert!(rendered.contains("Pack: http-local-smoke"));
        assert!(rendered.contains("k6 summary JSON"));
        assert!(!rendered.contains("Pack: rust-cli"));
        Ok(())
    }

    #[test]
    fn recommendation_prefers_rust_cli_shape() -> anyhow::Result<()> {
        let root = tempfile::tempdir().expect("temp dir");
        std::fs::write(
            root.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\n",
        )
        .expect("write manifest");
        std::fs::create_dir(root.path().join("src")).expect("create src");
        std::fs::write(root.path().join("src/main.rs"), "fn main() {}\n").expect("write main");

        let recommendation = recommend_adoption_pack(root.path())?;

        assert_eq!(recommendation.recommended_pack, "rust-cli");
        assert_eq!(recommendation.confidence, "high");
        assert!(
            recommendation
                .inspected
                .contains(&"found Cargo.toml".to_string())
        );
        assert!(recommendation.not_inspected.contains(&"baseline maturity"));
        Ok(())
    }

    #[test]
    fn recommendation_prefers_workspace_shape_over_cli() -> anyhow::Result<()> {
        let root = tempfile::tempdir().expect("temp dir");
        std::fs::write(
            root.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/demo\"]\n",
        )
        .expect("write manifest");
        std::fs::create_dir(root.path().join("src")).expect("create src");
        std::fs::write(root.path().join("src/main.rs"), "fn main() {}\n").expect("write main");

        let recommendation = recommend_adoption_pack(root.path())?;

        assert_eq!(recommendation.recommended_pack, "rust-workspace");
        assert_eq!(recommendation.confidence, "high");
        assert!(
            recommendation
                .inspected
                .contains(&"Cargo.toml contains [workspace]".to_string())
        );
        Ok(())
    }

    #[test]
    fn recommendation_falls_back_to_generic_command() -> anyhow::Result<()> {
        let root = tempfile::tempdir().expect("temp dir");

        let recommendation = recommend_adoption_pack(root.path())?;

        assert_eq!(recommendation.recommended_pack, "generic-command");
        assert_eq!(recommendation.confidence, "low");
        assert!(recommendation.next_command.contains("generic-command"));
        Ok(())
    }
}
