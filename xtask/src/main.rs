use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use glob::glob;
use regex::Regex;
use schemars::schema_for;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const TARGET_PUBLIC_PACKAGES: [&str; 5] = [
    "perfgate-types",
    "perfgate",
    "perfgate-client",
    "perfgate-server",
    "perfgate-cli",
];

const BADGE_ENDPOINT_DIR: &str = "badges";
const BADGE_ENDPOINT_TARGET_DIR: &str = "target/xtask/badges";
const RIPR_BADGE_VERSION: &str = "0.5.0";
const RIPR_PR_DIR: &str = "target/ripr/pr";
const RIPR_REVIEW_DIR: &str = "target/ripr/review";

const SCHEMA_FILES: [&str; 15] = [
    "perfgate.run.v1.schema.json",
    "perfgate.compare.v1.schema.json",
    "perfgate.probe.v1.schema.json",
    "perfgate.probe_compare.v1.schema.json",
    "perfgate.scenario.v1.schema.json",
    "perfgate.tradeoff.v1.schema.json",
    "perfgate.decision_index.v1.schema.json",
    "perfgate.decision_record.v1.schema.json",
    "perfgate.decision_bundle.v1.schema.json",
    "perfgate.config.v1.schema.json",
    "perfgate.report.v1.schema.json",
    "perfgate.aggregate.v1.schema.json",
    "perfgate.ratchet.v1.schema.json",
    "perfgate.repair_context.v1.schema.json",
    "sensor.report.v1.schema.json",
];

#[derive(Debug, Parser)]
#[command(name = "xtask", about = "Repo automation for perfgate")]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

/// Supported crates for mutation testing
#[derive(Debug, Clone, Copy, ValueEnum)]
enum MutantsCrate {
    #[value(
        name = "perfgate-domain",
        alias = "perfgate-stats",
        alias = "perfgate-significance",
        alias = "perfgate-host-detect",
        alias = "perfgate-budget",
        alias = "perfgate-scaling"
    )]
    Domain,
    #[value(
        name = "perfgate-types",
        alias = "perfgate-validation",
        alias = "perfgate-error",
        alias = "perfgate-config",
        alias = "perfgate-sha256",
        alias = "perfgate-api",
        alias = "perfgate-auth"
    )]
    Types,
    #[value(
        name = "perfgate-app",
        alias = "perfgate-adapters",
        alias = "perfgate-render",
        alias = "perfgate-summary",
        alias = "perfgate-export",
        alias = "perfgate-sensor"
    )]
    App,
    #[value(name = "perfgate-cli")]
    Cli,
    #[value(name = "perfgate-paired")]
    Paired,
    #[value(name = "perfgate-fake")]
    Fake,
}

impl MutantsCrate {
    fn as_package_name(&self) -> &'static str {
        match self {
            MutantsCrate::Domain => "perfgate",
            MutantsCrate::Types => "perfgate-types",
            MutantsCrate::App => "perfgate",
            MutantsCrate::Cli => "perfgate-cli",
            MutantsCrate::Paired => "perfgate",
            MutantsCrate::Fake => "perfgate-fake",
        }
    }

    fn target_kill_rate(&self) -> u8 {
        match self {
            MutantsCrate::Domain => 100,
            MutantsCrate::Types => 95,
            MutantsCrate::App => 90,
            MutantsCrate::Cli => 70,
            MutantsCrate::Paired => 100,
            MutantsCrate::Fake => 70,
        }
    }
}

#[derive(Debug, Subcommand)]
enum Command {
    /// (Re)generate JSON Schemas for receipts and config.
    Schema {
        /// Output directory
        #[arg(long, default_value = "schemas")]
        out_dir: PathBuf,
    },

    /// Verify committed schemas are locked to generated output (byte-for-byte).
    SchemaCheck {
        /// Schemas directory to verify
        #[arg(long, default_value = "schemas")]
        schemas_dir: PathBuf,
    },

    /// Verify old receipt fixtures still deserialize with current types.
    SchemaCompat {
        /// Historical schema fixtures directory.
        #[arg(long, default_value = "fixtures/schema")]
        fixtures_dir: PathBuf,
    },

    /// Run the usual repo checks.
    Ci,

    /// Validate workspace packaging metadata for crates.io publication.
    PublishCheck {
        /// Limit package-list or dry-run checks to one or more publishable packages.
        #[arg(long = "package", value_name = "PACKAGE")]
        packages: Vec<String>,

        /// Run `cargo package --list` for every publishable workspace package.
        #[arg(long)]
        package_list: bool,

        /// Run `cargo publish --dry-run` for selected publishable packages.
        #[arg(long)]
        dry_run: bool,

        /// Pass `--allow-dirty` to Cargo packaging commands.
        #[arg(long)]
        allow_dirty: bool,
    },

    /// Validate GitHub Action install and release asset wiring.
    ActionCheck {
        /// GitHub Action definition to validate.
        #[arg(long, default_value = "action.yml")]
        action: PathBuf,

        /// perfgate-cli manifest containing cargo-binstall metadata.
        #[arg(long, default_value = "crates/perfgate-cli/Cargo.toml")]
        cli_manifest: PathBuf,
    },

    /// Validate the target public crate policy and transition dispositions.
    PublicSurface {
        /// Target public crate policy file.
        #[arg(long, default_value = "policy/public_crates.txt")]
        public_policy: PathBuf,

        /// Absorbed/internal crate disposition file.
        #[arg(long, default_value = "policy/absorbed_crates.txt")]
        absorbed_policy: PathBuf,

        /// Fail if any absorbed package is still publishable.
        #[arg(long)]
        strict: bool,
    },

    /// Policy governance checks.
    Policy {
        #[command(subcommand)]
        action: PolicyAction,
    },

    /// Validate Rails source-of-truth framework artifacts.
    Rails {
        #[command(subcommand)]
        action: RailsAction,
    },

    /// Enforce crate-layer dependency rules for the current architecture.
    Arch,

    /// Validate JSON fixtures against the vendored sensor.report.v1 schema.
    Conform {
        /// Directory of fixtures to validate (default: golden fixtures)
        #[arg(long)]
        fixtures: Option<PathBuf>,

        /// Validate a single file
        #[arg(long)]
        file: Option<PathBuf>,
    },

    /// Copy golden fixtures to contracts/fixtures/ (golden is source of truth).
    SyncFixtures,

    /// Run mutation testing via cargo-mutants (must be installed).
    Mutants {
        /// Run mutation testing on a specific crate only
        #[arg(long = "crate", value_enum)]
        crate_name: Option<MutantsCrate>,

        /// Generate a summary report after mutation testing
        #[arg(long)]
        summary: bool,

        /// Extra args forwarded to cargo-mutants
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// List all microcrates and their purposes.
    Microcrates,

    /// Dogfooding operations.
    Dogfood {
        #[command(subcommand)]
        action: DogfoodAction,
    },

    /// Update generated docs.
    DocsSync,

    /// Fail CI if generated docs differ from committed docs.
    DocsCheck,

    /// Regenerate committed public Shields endpoint badge JSON.
    Badges {
        /// Regenerate into target/ and fail if committed endpoints drift.
        #[arg(long)]
        check: bool,
    },

    /// Produce PR-scoped RIPR repository exposure evidence.
    RiprPr {
        /// Verify the required output contract instead of regenerating evidence.
        #[arg(long)]
        check: bool,
    },

    /// Produce RIPR review guidance artifacts.
    RiprReviewComments {
        /// Verify the required output contract instead of regenerating guidance.
        #[arg(long)]
        check: bool,
    },

    /// Validate non-Rust file policy coverage for common generated surfaces.
    CheckFilePolicy,

    /// Run the standard fast local PR checks.
    Pr,

    /// Validate CLI examples in documentation against actual --help output.
    DocTest {
        /// Additional markdown files to scan in addition to the current-doc default set
        #[arg(long)]
        files: Vec<PathBuf>,
    },

    /// Validate source-of-truth docs metadata, IDs, links, and active goal TOML.
    DocsSourceCheck {
        /// Repository root to validate.
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },

    /// Validate product claim proof-map structure.
    ProductClaimsCheck {
        /// Product claims markdown file.
        #[arg(long, default_value = "docs/status/PRODUCT_CLAIMS.md")]
        path: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum DogfoodAction {
    /// (Re)generate stable compare/check fixtures from controlled inputs.
    Fixtures,
    /// Validate expected artifact layout and allowed exit behavior.
    Verify {
        /// Directory containing perfgate output artifacts
        #[arg(long, default_value = "artifacts/perfgate")]
        dir: PathBuf,
    },
    /// Turn nightly outputs into refreshed baseline files.
    Promote,
    /// Export nightly run/compare receipts into persisted trend files.
    ExportTrends {
        /// Directory containing perfgate output artifacts.
        #[arg(long, default_value = "artifacts/perfgate")]
        artifacts_dir: PathBuf,
        /// Directory where trend files are written.
        #[arg(long, default_value = "artifacts/trends")]
        out_dir: PathBuf,
    },
    /// Generate a compact Markdown/JSON summary of drift, noise, and recommendations.
    Summarize {
        /// Directory containing perfgate export trends
        #[arg(long, default_value = "artifacts/trends")]
        dir: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum PolicyAction {
    /// Scan Rust files for panic-family callsites using exact counted identities.
    CheckNoPanicFamily {
        /// Directory to scan.
        #[arg(long, default_value = ".")]
        root: PathBuf,

        /// Exact no-panic allowlist file.
        #[arg(long, default_value = "policy/no-panic-allowlist.toml")]
        allowlist: PathBuf,

        /// Generated no-panic baseline file.
        #[arg(long, default_value = "policy/no-panic-baseline.toml")]
        baseline: PathBuf,

        /// Refresh the generated baseline after validating it will not absorb new debt.
        #[arg(long)]
        write_baseline: bool,
    },
}

#[derive(Debug, Subcommand)]
enum RailsAction {
    /// Validate the .rails registry, artifact paths, statuses, and links.
    Check {
        /// Repository root to validate.
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.cmd {
        Command::Schema { out_dir } => cmd_schema(&out_dir),
        Command::SchemaCheck { schemas_dir } => cmd_schema_check(&schemas_dir),
        Command::SchemaCompat { fixtures_dir } => cmd_schema_compat(&fixtures_dir),
        Command::Ci => cmd_ci(),
        Command::PublishCheck {
            packages,
            package_list,
            dry_run,
            allow_dirty,
        } => cmd_publish_check(packages, package_list, dry_run, allow_dirty),
        Command::ActionCheck {
            action,
            cli_manifest,
        } => cmd_action_check(&action, &cli_manifest),
        Command::PublicSurface {
            public_policy,
            absorbed_policy,
            strict,
        } => cmd_public_surface(&public_policy, &absorbed_policy, strict),
        Command::Policy { action } => match action {
            PolicyAction::CheckNoPanicFamily {
                root,
                allowlist,
                baseline,
                write_baseline,
            } => cmd_check_no_panic_family(&root, &allowlist, &baseline, write_baseline),
        },
        Command::Rails { action } => match action {
            RailsAction::Check { root } => cmd_rails_check(&root),
        },
        Command::Arch => cmd_arch(),
        Command::Conform { fixtures, file } => cmd_conform(fixtures, file),
        Command::SyncFixtures => cmd_sync_fixtures(),
        Command::Mutants {
            crate_name,
            summary,
            args,
        } => cmd_mutants(crate_name, summary, args),
        Command::Microcrates => cmd_microcrates(),
        Command::Dogfood { action } => cmd_dogfood(action),
        Command::DocsSync => cmd_docs_sync(),
        Command::DocsCheck => cmd_docs_check(),
        Command::Badges { check } => cmd_badges(check),
        Command::RiprPr { check } => cmd_ripr_pr(check),
        Command::RiprReviewComments { check } => cmd_ripr_review_comments(check),
        Command::CheckFilePolicy => cmd_check_file_policy(),
        Command::Pr => cmd_pr(),
        Command::DocTest { files } => cmd_doc_test(files),
        Command::DocsSourceCheck { root } => cmd_docs_source_check(&root),
        Command::ProductClaimsCheck { path } => cmd_product_claims_check(&path),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
struct ShieldsEndpointBadge {
    #[serde(rename = "schemaVersion")]
    schema_version: u8,
    label: String,
    message: String,
    color: String,
}

fn workspace_root_path() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .expect("xtask manifest directory has a workspace parent")
        .to_path_buf()
}

fn cmd_badges(check: bool) -> anyhow::Result<()> {
    let workspace_root = workspace_root_path();
    let target_dir = workspace_root.join(BADGE_ENDPOINT_TARGET_DIR);
    fs::create_dir_all(&target_dir)
        .with_context(|| format!("create dir {}", target_dir.display()))?;

    let ripr_plus = ripr_plus_badge(&workspace_root)?;
    validate_shields_badge(&ripr_plus, Some("ripr+"))?;
    write_json_pretty(&target_dir.join("ripr-plus.json"), &ripr_plus)?;

    let committed_dir = workspace_root.join(BADGE_ENDPOINT_DIR);
    if check {
        compare_files(
            &committed_dir.join("ripr-plus.json"),
            &target_dir.join("ripr-plus.json"),
        )?;
        println!("badges: committed endpoints are current");
        return Ok(());
    }

    fs::create_dir_all(&committed_dir)
        .with_context(|| format!("create dir {}", committed_dir.display()))?;
    fs::copy(
        target_dir.join("ripr-plus.json"),
        committed_dir.join("ripr-plus.json"),
    )
    .with_context(|| "refreshing committed ripr-plus badge endpoint")?;

    println!("badges: refreshed public endpoint JSON under badges/");
    Ok(())
}

fn ripr_plus_badge(workspace_root: &Path) -> anyhow::Result<ShieldsEndpointBadge> {
    let ripr_bin = std::env::var("RIPR_BIN").unwrap_or_else(|_| "ripr".to_string());
    validate_ripr_badge_version(&ripr_bin)?;
    let mut output = run_ripr_check_format(&ripr_bin, workspace_root, "repo-badge-plus-shields")?;

    if !output.status.success()
        && String::from_utf8_lossy(&output.stderr).contains("test-efficiency.json")
    {
        eprintln!(
            "{ripr_bin} repo-badge-plus-shields could not include test-efficiency evidence; falling back to repo-badge-shields"
        );
        output = run_ripr_check_format(&ripr_bin, workspace_root, "repo-badge-shields")?;
    }

    if !output.status.success() {
        anyhow::bail!(
            "{ripr_bin} repo-scoped badge check failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let mut badge: ShieldsEndpointBadge = serde_json::from_slice(&output.stdout)
        .with_context(|| format!("{ripr_bin} emitted invalid Shields endpoint JSON"))?;
    badge.label = "ripr+".to_string();
    Ok(badge)
}

fn validate_ripr_badge_version(ripr_bin: &str) -> anyhow::Result<()> {
    let output = std::process::Command::new(ripr_bin)
        .arg("--version")
        .output()
        .with_context(|| format!("running {ripr_bin} --version"))?;
    if !output.status.success() {
        anyhow::bail!(
            "running {ripr_bin} --version failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let Some(version) = parse_ripr_version(&stdout) else {
        anyhow::bail!("{ripr_bin} --version output was not recognized: {stdout:?}");
    };
    if version != RIPR_BADGE_VERSION {
        anyhow::bail!(
            "ripr+ badge generation requires ripr {RIPR_BADGE_VERSION}, got {version}. \
             Install with: cargo install ripr --version {RIPR_BADGE_VERSION} --locked --force"
        );
    }

    Ok(())
}

fn parse_ripr_version(output: &str) -> Option<&str> {
    let mut parts = output.split_whitespace();
    match (parts.next(), parts.next()) {
        (Some("ripr"), Some(version)) => Some(version),
        _ => None,
    }
}

fn run_ripr_check_format(
    ripr_bin: &str,
    workspace_root: &Path,
    format: &str,
) -> anyhow::Result<std::process::Output> {
    std::process::Command::new(ripr_bin)
        .arg("check")
        .arg("--root")
        .arg(workspace_root)
        .arg("--format")
        .arg(format)
        .current_dir(workspace_root)
        .output()
        .with_context(|| format!("running {ripr_bin} {format}"))
}

fn validate_shields_badge(
    badge: &ShieldsEndpointBadge,
    expected_label: Option<&str>,
) -> anyhow::Result<()> {
    if badge.schema_version != 1 {
        anyhow::bail!("badge `{}` has unsupported schemaVersion", badge.label);
    }

    if let Some(expected_label) = expected_label
        && badge.label != expected_label
    {
        anyhow::bail!(
            "badge label drifted: got `{}`, expected `{expected_label}`",
            badge.label
        );
    }

    if badge.message.trim().is_empty() {
        anyhow::bail!("badge `{}` has empty message", badge.label);
    }

    if badge.color.trim().is_empty() {
        anyhow::bail!("badge `{}` has empty color", badge.label);
    }

    Ok(())
}

fn write_json_pretty<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(value).context("serializing JSON")?;
    fs::write(path, format!("{json}\n")).with_context(|| format!("write {}", path.display()))
}

fn compare_files(committed: &Path, generated: &Path) -> anyhow::Result<()> {
    let committed_contents = fs::read(committed)
        .with_context(|| format!("reading committed endpoint {}", committed.display()))?;
    let generated_contents = fs::read(generated)
        .with_context(|| format!("reading generated endpoint {}", generated.display()))?;

    if committed_contents != generated_contents {
        anyhow::bail!(
            "generated endpoint drift detected for {}. Run: cargo xtask badges",
            committed.display()
        );
    }

    Ok(())
}

fn cmd_ripr_pr(check: bool) -> anyhow::Result<()> {
    let workspace_root = workspace_root_path();
    let out_dir = workspace_root.join(RIPR_PR_DIR);
    if check {
        validate_ripr_pr_contract(&out_dir)
    } else {
        fs::create_dir_all(&out_dir)
            .with_context(|| format!("create dir {}", out_dir.display()))?;
        let ripr_bin = std::env::var("RIPR_BIN").unwrap_or_else(|_| "ripr".to_string());
        let json_path = out_dir.join("repo-exposure.json");
        let md_path = out_dir.join("repo-exposure.md");
        let output = run_ripr_check_format(&ripr_bin, &workspace_root, "repo-exposure-json")?;
        if !output.status.success() {
            anyhow::bail!(
                "{ripr_bin} repo-exposure-json failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        fs::write(&json_path, &output.stdout)
            .with_context(|| format!("write {}", json_path.display()))?;
        let _: serde_json::Value = serde_json::from_slice(&output.stdout)
            .with_context(|| format!("{} must be valid JSON", json_path.display()))?;

        let md_output = run_ripr_check_format(&ripr_bin, &workspace_root, "repo-exposure-md")?;
        if md_output.status.success() {
            fs::write(&md_path, &md_output.stdout)
                .with_context(|| format!("write {}", md_path.display()))?;
        } else {
            write_ripr_pr_markdown(&json_path, &md_path)?;
        }
        validate_ripr_pr_contract(&out_dir)
    }
}

fn write_ripr_pr_markdown(json_path: &Path, md_path: &Path) -> anyhow::Result<()> {
    let md = format!(
        "# RIPR PR Evidence\n\nRepo-scoped static exposure evidence for this pull request was written to `{}`.\n\nThis artifact is diff-scoped PR evidence and must not be reused as a public README badge.\n",
        json_path.display()
    );
    fs::write(md_path, md).with_context(|| format!("write {}", md_path.display()))
}

fn validate_ripr_pr_contract(out_dir: &Path) -> anyhow::Result<()> {
    validate_json_file(&out_dir.join("repo-exposure.json"))?;
    validate_nonempty_file(&out_dir.join("repo-exposure.md"))?;
    println!("ripr-pr: output contract is valid");
    Ok(())
}

fn cmd_ripr_review_comments(check: bool) -> anyhow::Result<()> {
    let workspace_root = workspace_root_path();
    let out_dir = workspace_root.join(RIPR_REVIEW_DIR);
    if check {
        return validate_ripr_review_contract(&out_dir);
    }

    fs::create_dir_all(&out_dir).with_context(|| format!("create dir {}", out_dir.display()))?;
    let ripr_bin = std::env::var("RIPR_BIN").unwrap_or_else(|_| "ripr".to_string());
    let out_path = out_dir.join("comments.json");
    let output = std::process::Command::new(&ripr_bin)
        .arg("review-comments")
        .arg("--root")
        .arg(&workspace_root)
        .arg("--base")
        .arg(resolve_ripr_base(&workspace_root)?)
        .arg("--head")
        .arg("HEAD")
        .arg("--out")
        .arg(&out_path)
        .current_dir(&workspace_root)
        .output()
        .with_context(|| format!("running {ripr_bin} review-comments"))?;
    if !output.status.success() {
        anyhow::bail!(
            "{ripr_bin} review-comments failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    validate_ripr_review_contract(&out_dir)
}

fn resolve_ripr_base(workspace_root: &Path) -> anyhow::Result<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--verify", "origin/main"])
        .current_dir(workspace_root)
        .output()
        .context("checking for origin/main")?;

    if output.status.success() {
        Ok("origin/main".to_string())
    } else {
        eprintln!("origin/main is unavailable; using HEAD as the explicit RIPR review base");
        Ok("HEAD".to_string())
    }
}

fn validate_ripr_review_contract(out_dir: &Path) -> anyhow::Result<()> {
    validate_json_file(&out_dir.join("comments.json"))?;
    validate_nonempty_file(&out_dir.join("comments.md"))?;
    println!("ripr-review-comments: output contract is valid");
    Ok(())
}

fn validate_json_file(path: &Path) -> anyhow::Result<()> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str::<serde_json::Value>(&contents)
        .with_context(|| format!("{} must contain valid JSON", path.display()))?;
    Ok(())
}

fn validate_nonempty_file(path: &Path) -> anyhow::Result<()> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    if contents.trim().is_empty() {
        anyhow::bail!("{} must not be empty", path.display());
    }
    Ok(())
}

fn cmd_check_file_policy() -> anyhow::Result<()> {
    let policy = fs::read_to_string(workspace_root_path().join("policy/non-rust-allowlist.toml"))
        .context("reading policy/non-rust-allowlist.toml")?;
    for required in ["badges/*.json", "scripts/*.py"] {
        if !policy.contains(required) {
            anyhow::bail!("non-Rust file policy is missing `{required}`");
        }
    }
    println!("  OK  non-Rust file policy covers generated badge and script surfaces");
    Ok(())
}

fn cmd_pr() -> anyhow::Result<()> {
    cmd_badges(true)?;
    cmd_docs_check()?;
    cmd_check_file_policy()?;
    run("cargo", ["test", "-p", "xtask", "badge"])?;
    Ok(())
}

fn cmd_ci() -> anyhow::Result<()> {
    let target_dir =
        std::env::var("PERFGATE_CI_TARGET_DIR").unwrap_or_else(|_| "target".to_string());
    let cargo_env = vec![("CARGO_TARGET_DIR", target_dir.as_str())];
    let xtask_target_dir = format!("{target_dir}/xtask-self");
    let xtask_env = vec![("CARGO_TARGET_DIR", xtask_target_dir.as_str())];

    run_with_env("cargo", ["fmt", "--all", "--", "--check"], &cargo_env)?;
    run_with_env(
        "cargo",
        [
            "clippy",
            "--workspace",
            "--exclude",
            "xtask",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ],
        &cargo_env,
    )?;
    run_with_env(
        "cargo",
        [
            "test",
            "--workspace",
            "--exclude",
            "xtask",
            "--all-features",
        ],
        &cargo_env,
    )?;
    run_with_env(
        "cargo",
        [
            "clippy",
            "-p",
            "xtask",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ],
        &xtask_env,
    )?;
    run_with_env(
        "cargo",
        ["test", "-p", "xtask", "--all-features"],
        &xtask_env,
    )?;
    cmd_schema_check(Path::new("schemas"))?;
    cmd_schema_compat(Path::new("fixtures/schema"))?;
    cmd_conform(None, None)?;
    cmd_publish_check(Vec::new(), false, false, false)?;
    cmd_action_check(
        Path::new("action.yml"),
        Path::new("crates/perfgate-cli/Cargo.toml"),
    )?;
    cmd_public_surface(
        Path::new("policy/public_crates.txt"),
        Path::new("policy/absorbed_crates.txt"),
        false,
    )?;
    cmd_arch()?;
    cmd_doc_test(Vec::new())?;
    Ok(())
}

fn cmd_action_check(action: &Path, cli_manifest: &Path) -> anyhow::Result<()> {
    let action_content =
        fs::read_to_string(action).with_context(|| format!("reading {}", action.display()))?;
    let manifest_content = fs::read_to_string(cli_manifest)
        .with_context(|| format!("reading {}", cli_manifest.display()))?;
    let summary_examples_path = Path::new("docs/examples/action-failure-summaries.md");
    let summary_examples = fs::read_to_string(summary_examples_path)
        .with_context(|| format!("reading {}", summary_examples_path.display()))?;
    let mut errors = collect_action_check_errors(&action_content, &manifest_content);
    errors.extend(collect_action_summary_example_errors(&summary_examples));
    errors.extend(collect_workflow_policy_errors(Path::new(
        ".github/workflows",
    ))?);

    if !errors.is_empty() {
        println!(
            "Found {} GitHub Action release/install/diagnostic/workflow policy error(s):",
            errors.len()
        );
        for error in &errors {
            println!("  - {}", error);
        }

        anyhow::bail!(
            "{} GitHub Action release/install/diagnostic/workflow policy issue(s) found. Fix action.yml, binstall metadata, summary examples, or workflow policy.",
            errors.len()
        );
    }

    println!(
        "  OK  GitHub Action install, release asset, failure diagnostic, and workflow policy wiring is aligned"
    );
    Ok(())
}

fn collect_action_check_errors(action: &str, cli_manifest: &str) -> Vec<String> {
    let mut errors = Vec::new();
    let raw_action = action;

    let action = match yaml_serde::from_str::<ActionDefinition>(action) {
        Ok(action) => action,
        Err(err) => {
            errors.push(format!("action.yml must parse as YAML: {err}"));
            return errors;
        }
    };
    let manifest = match toml::from_str::<toml::Value>(cli_manifest) {
        Ok(manifest) => manifest,
        Err(err) => {
            errors.push(format!("perfgate-cli Cargo.toml must parse as TOML: {err}"));
            return errors;
        }
    };

    let version_description = action
        .inputs
        .get("version")
        .and_then(|input| input.description.as_deref());
    if version_description.is_none_or(|description| !description.contains("perfgate-cli")) {
        errors.push("action.yml version input must describe the perfgate-cli crate".to_string());
    }
    if action
        .inputs
        .get("out_dir")
        .and_then(|input| input.default.as_deref())
        != Some("")
    {
        errors.push(
            "action.yml out_dir input must default to empty so [defaults].out_dir can drive artifact paths"
                .to_string(),
        );
    }
    if action
        .inputs
        .get("decision")
        .and_then(|input| input.default.as_deref())
        != Some("false")
    {
        errors.push(
            "action.yml decision input must exist and default to false for opt-in structured decisions"
                .to_string(),
        );
    }
    if action
        .inputs
        .get("review_required")
        .and_then(|input| input.default.as_deref())
        != Some("warn")
    {
        errors.push(
            "action.yml review_required input must exist and default to warn for needs-review decisions"
                .to_string(),
        );
    }

    let Some(binary_install_run) = action.step_run("Install perfgate (pre-built binary)") else {
        errors.push("action.yml must include the pre-built binary install step".to_string());
        return errors;
    };
    let binary_install_lines = active_shell_lines(binary_install_run);
    if !binary_install_lines
        .iter()
        .any(|line| line.contains("releases/download/v${version}/perfgate-${target}.${ext}"))
    {
        errors.push("action.yml prebuilt binary URL must match release archive naming".to_string());
    }

    let Some(cargo_install_run) = action.step_run("Install perfgate (cargo install fallback)")
    else {
        errors.push("action.yml must include the cargo-install fallback step".to_string());
        return errors;
    };
    let cargo_install_lines = active_shell_lines(cargo_install_run);
    if cargo_install_lines.iter().any(|line| {
        line.starts_with("cargo install perfgate ")
            || line == "cargo install perfgate --locked --force --version \"${{ inputs.version }}\""
    }) {
        errors.push(
            "action.yml cargo-install fallback installs `perfgate`; install `perfgate-cli`"
                .to_string(),
        );
    }
    if !cargo_install_lines.iter().any(|line| {
        line == "cargo install perfgate-cli --locked --force --version \"${{ inputs.version }}\""
    }) {
        errors.push(
            "action.yml must cargo-install the published `perfgate-cli` package for versioned fallbacks"
                .to_string(),
        );
    }
    if !cargo_install_lines.iter().any(|line| {
        line == "cargo install --path \"${GITHUB_ACTION_PATH}/crates/perfgate-cli\" --locked --force"
    }) {
        errors.push(
            "action.yml must build the local crates/perfgate-cli package when no version is supplied"
                .to_string(),
        );
    }

    let Some(verify_install_run) = action.step_run("Verify perfgate installation") else {
        errors.push("action.yml must include an installation verification step".to_string());
        return errors;
    };
    let verify_install_lines = active_shell_lines(verify_install_run);
    if !verify_install_lines
        .iter()
        .any(|line| line == "perfgate --version")
    {
        errors.push("action.yml must verify the installed perfgate binary".to_string());
    }
    if !verify_install_lines
        .iter()
        .any(|line| line == "perfgate doctor --help")
    {
        errors.push("action.yml must smoke-test the doctor command after install".to_string());
    }

    let Some(resolve_out_dir_run) = action.step_run("Resolve artifact directory") else {
        errors.push(
            "action.yml must resolve the effective artifact directory before running perfgate"
                .to_string(),
        );
        return errors;
    };
    let resolve_out_dir_lines = active_shell_lines(resolve_out_dir_run);
    if !resolve_out_dir_lines
        .iter()
        .any(|line| line == "default_out_dir=\"artifacts/perfgate\"")
    {
        errors.push(
            "action.yml artifact resolver must keep artifacts/perfgate as the fallback".to_string(),
        );
    }
    if !resolve_out_dir_lines
        .iter()
        .any(|line| line == "echo \"out_dir=${out_dir}\" >> \"${GITHUB_OUTPUT}\"")
    {
        errors
            .push("action.yml artifact resolver must expose out_dir as a step output".to_string());
    }

    let Some(run_check_run) = action.step_run("Run perfgate check") else {
        errors.push("action.yml must include the perfgate check step".to_string());
        return errors;
    };
    let run_check_lines = active_shell_lines(run_check_run);
    if !run_check_lines
        .iter()
        .any(|line| line == "if [[ -n \"${{ inputs.out_dir }}\" ]]; then")
        || !run_check_lines
            .iter()
            .any(|line| line == "args+=(--out-dir \"${{ inputs.out_dir }}\")")
    {
        errors.push(
            "action.yml must pass --out-dir only when the input explicitly overrides config"
                .to_string(),
        );
    }
    if run_check_lines
        .iter()
        .any(|line| line == "--out-dir \"${{ inputs.out_dir }}\"")
    {
        errors.push("action.yml must not unconditionally pass the out_dir input".to_string());
    }
    if !run_check_lines
        .iter()
        .any(|line| line == "echo \"policy_failure_deferred=true\" >> \"${GITHUB_OUTPUT}\"")
        || !run_check_lines.iter().any(|line| {
            line == "if [[ \"${{ inputs.decision }}\" == \"true\" && \"${status}\" == \"2\" ]]; then"
        })
    {
        errors.push(
            "action.yml must defer check policy failures to decision evaluate when decision=true"
                .to_string(),
        );
    }

    let Some(run_decision_run) = action.step_run("Run perfgate decision") else {
        errors.push("action.yml must include the perfgate decision evaluation step".to_string());
        return errors;
    };
    let run_decision_lines = active_shell_lines(run_decision_run);
    if !raw_action.contains(
        "if: always() && inputs.decision == 'true' && (steps.run_check.outputs.exit_code == '0' || steps.run_check.outputs.exit_code == '2')",
    ) {
        errors.push(
            "action.yml decision step must run only after pass or policy-fail check results"
                .to_string(),
        );
    }
    if !run_decision_lines
        .iter()
        .any(|line| line == "args=(decision evaluate --config \"${{ inputs.config }}\")")
    {
        errors.push(
            "action.yml decision step must run `perfgate decision evaluate` with the configured file"
                .to_string(),
        );
    }
    if !run_decision_lines
        .iter()
        .any(|line| line == "if [[ -n \"${{ inputs.out_dir }}\" ]]; then")
        || !run_decision_lines
            .iter()
            .any(|line| line == "args+=(--out-dir \"${{ inputs.out_dir }}\")")
    {
        errors.push(
            "action.yml decision step must pass --out-dir only when the input explicitly overrides config"
                .to_string(),
        );
    }
    if !run_decision_lines
        .iter()
        .any(|line| line == "echo \"exit_code=${status}\" >> \"${GITHUB_OUTPUT}\"")
    {
        errors.push("action.yml decision step must expose its exit code".to_string());
    }

    let Some(handle_review_run) = action.step_run("Handle review-required decision") else {
        errors
            .push("action.yml must include the review-required decision handling step".to_string());
        return errors;
    };
    if action
        .step("Handle review-required decision")
        .and_then(|step| step.id.as_deref())
        != Some("handle_review_required")
    {
        errors.push(
            "action.yml review-required step must use id handle_review_required for downstream outputs"
                .to_string(),
        );
    }
    let handle_review_lines = active_shell_lines(handle_review_run);
    if !raw_action.contains(
        "if: always() && inputs.decision == 'true' && steps.run_decision.outputs.exit_code == '0'",
    ) {
        errors.push(
            "action.yml review-required step must inspect successful decision receipts".to_string(),
        );
    }
    if !handle_review_lines
        .iter()
        .any(|line| line == "policy=\"${{ inputs.review_required }}\"")
        || !handle_review_lines
            .iter()
            .any(|line| line == "pass|warn|fail) ;;")
    {
        errors.push(
            "action.yml review_required input must accept pass, warn, and fail policies"
                .to_string(),
        );
    }
    if !handle_review_lines
        .iter()
        .any(|line| line == "out=\"${{ steps.resolve_out_dir.outputs.out_dir }}\"")
        || !handle_review_lines
            .iter()
            .any(|line| line.contains("${out}/tradeoff.json"))
    {
        errors.push(
            "action.yml review-required step must inspect the resolved tradeoff receipt"
                .to_string(),
        );
    }
    if !raw_action.contains("decision.get(\"review_required\")")
        || !handle_review_lines.iter().any(|line| {
            line == "echo \"review_required=${review_required}\" >> \"${GITHUB_OUTPUT}\""
        })
        || !handle_review_lines.iter().any(|line| {
            line == "echo \"review_required_reason=${review_reason}\" >> \"${GITHUB_OUTPUT}\""
        })
    {
        errors
            .push("action.yml review-required step must expose decision review state".to_string());
    }
    if !handle_review_lines
        .iter()
        .any(|line| line == "echo \"exit_code=2\" >> \"${GITHUB_OUTPUT}\"")
        || !handle_review_lines
            .iter()
            .any(|line| line == "echo \"exit_code=0\" >> \"${GITHUB_OUTPUT}\"")
    {
        errors.push(
            "action.yml review-required step must expose a final review policy exit code"
                .to_string(),
        );
    }

    let Some(decision_summary_run) = action.step_run("Append perfgate decision summary") else {
        errors.push("action.yml must append decision.md to GITHUB_STEP_SUMMARY".to_string());
        return errors;
    };
    let decision_summary_lines = active_shell_lines(decision_summary_run);
    if !raw_action.contains("if: always() && inputs.decision == 'true'")
        || !decision_summary_lines
            .iter()
            .any(|line| line == "out=\"${{ steps.resolve_out_dir.outputs.out_dir }}\"")
        || !decision_summary_lines.iter().any(|line| {
            line == "if [[ -f \"${out}/decision.md\" && -n \"${GITHUB_STEP_SUMMARY:-}\" ]]; then"
        })
        || !decision_summary_lines
            .iter()
            .any(|line| line == "cat \"${out}/decision.md\"")
    {
        errors.push(
            "action.yml must publish generated decision.md to the GitHub step summary".to_string(),
        );
    }

    let Some(policy_summary_run) = action.step_run("Append perfgate policy posture summary") else {
        errors.push(
            "action.yml must append advisory policy posture to GITHUB_STEP_SUMMARY".to_string(),
        );
        return errors;
    };
    let policy_summary_lines = active_shell_lines(policy_summary_run);
    if !raw_action.contains("if: always() && steps.run_check.outputs.exit_code != ''")
        || !policy_summary_lines
            .iter()
            .any(|line| line == "out=\"${{ steps.resolve_out_dir.outputs.out_dir }}\"")
        || !policy_summary_lines.iter().any(|line| {
            line == "policy_args=(policy doctor --config \"${{ inputs.config }}\")"
        })
        || !policy_summary_lines.iter().any(|line| {
            line == "review_packet_args=(policy review-packet --config \"${{ inputs.config }}\" --bench \"${{ inputs.bench }}\")"
        })
    {
        errors.push(
            "action.yml policy posture summary must run policy doctor and expose the review packet command".to_string(),
        );
    }
    if !policy_summary_lines
        .iter()
        .any(|line| line == "review_packet_output=\"\"")
        || !policy_summary_lines
            .iter()
            .any(|line| line == "review_packet_status=\"skipped\"")
        || !policy_summary_lines.iter().any(|line| {
            line == "if ! perfgate \"${review_packet_args[@]}\" > \"${review_packet_output}\" 2>&1; then"
        })
        || !policy_summary_lines
            .iter()
            .any(|line| line == "echo \"Benchmark passport (${review_packet_status}):\"")
        || !policy_summary_lines.iter().any(|line| {
            line == "awk '/^## Benchmark Passport/{flag=1; print; next} /^## / && flag{exit} flag{print}' \"${review_packet_output}\""
        })
    {
        errors.push(
            "action.yml policy posture summary must surface the benchmark passport from review-packet output"
                .to_string(),
        );
    }
    if !policy_summary_lines.iter().any(|line| {
        line == "echo \"Blocking behavior: this action preserves existing perfgate exit-code behavior; maturity guidance is advisory unless your config already makes it blocking.\""
    }) || !policy_summary_lines.iter().any(|line| {
        line == "echo \"Advisory signal: missing baselines remain setup guidance unless this workflow enables required-baseline mode.\""
    }) || !policy_summary_lines.iter().any(|line| {
        line == "echo \"Imported evidence: policy doctor output includes source kind, source path, metric mapping, maturity limits, and advisory boundaries when receipts expose them.\""
    }) || !policy_summary_lines
        .iter()
        .any(|line| line == "echo \"Blocking gate: required-baseline mode is enabled.\"")
    {
        errors.push(
            "action.yml policy posture summary must distinguish advisory and blocking posture"
                .to_string(),
        );
    }
    if !policy_summary_lines.iter().any(|line| {
        line == "review_required=\"${{ steps.handle_review_required.outputs.review_required }}\""
    }) || !policy_summary_lines
        .iter()
        .any(|line| line == "echo \"Policy review required: ${review_reason}\"")
    {
        errors.push(
            "action.yml policy posture summary must surface review-required posture".to_string(),
        );
    }
    if policy_summary_lines
        .iter()
        .any(|line| line.starts_with("echo \"```"))
        || !policy_summary_lines
            .iter()
            .any(|line| line == "printf '%s\\n' '```bash'")
        || !policy_summary_lines
            .iter()
            .any(|line| line == "printf '%s\\n' '```text'")
        || !policy_summary_lines
            .iter()
            .any(|line| line == "printf '%s\\n' '```'")
    {
        errors.push(
            "action.yml policy posture summary must emit Markdown code fences without Bash command substitution"
                .to_string(),
        );
    }
    if !policy_summary_lines
        .iter()
        .any(|line| line == "echo \"### perfgate policy posture\"")
        || !policy_summary_lines
            .iter()
            .any(|line| line == "} >> \"${GITHUB_STEP_SUMMARY}\"")
        || !policy_summary_lines.iter().any(|line| {
            line == "echo \"Do not: make advisory maturity output blocking, loosen thresholds, promote baselines, or require server ledger mode from this summary alone.\""
        })
    {
        errors.push(
            "action.yml policy posture summary must write guarded posture guidance to GITHUB_STEP_SUMMARY"
                .to_string(),
        );
    }

    let Some(failure_summary_run) = action.step_run("Print perfgate failure summary") else {
        errors.push(
            "action.yml must print a local reproduction command when perfgate fails".to_string(),
        );
        return errors;
    };
    let failure_summary_lines = active_shell_lines(failure_summary_run);
    if !raw_action.contains("steps.run_check.outputs.policy_failure_deferred != 'true'")
        || !raw_action.contains("steps.run_decision.outputs.exit_code != '0'")
        || !raw_action.contains("steps.handle_review_required.outputs.exit_code != '0'")
    {
        errors.push(
            "action.yml failure summary must respect decision-mode final verdicts".to_string(),
        );
    }
    if !failure_summary_lines
        .iter()
        .any(|line| line == "out=\"${{ steps.resolve_out_dir.outputs.out_dir }}\"")
    {
        errors.push(
            "action.yml failure summary must use the resolved artifact directory".to_string(),
        );
    }
    if !failure_summary_lines
        .iter()
        .any(|line| line == "exit_code=\"${{ steps.run_check.outputs.exit_code }}\"")
        || !failure_summary_lines.iter().any(|line| {
            line == "review_exit_code=\"${{ steps.handle_review_required.outputs.exit_code }}\""
        })
    {
        errors.push("action.yml failure summary must include the perfgate exit code".to_string());
    }
    if !failure_summary_lines
        .iter()
        .any(|line| line == "verdict=\"${{ steps.run_check.outputs.verdict }}\"")
        || !failure_summary_lines.iter().any(|line| {
            line == "echo \"Verdict: ${verdict} (pass=${pass_count:-0}, warn=${warn_count:-0}, fail=${fail_count:-0}, benches=${bench_count:-unknown})\""
        })
    {
        errors.push(
            "action.yml failure summary must include verdict counts when available".to_string(),
        );
    }
    if !failure_summary_lines.iter().any(|line| {
        line == "review_reason=\"${{ steps.handle_review_required.outputs.review_required_reason }}\""
    }) || !failure_summary_lines
        .iter()
        .any(|line| line == "echo \"Review required: ${review_reason}\"")
    {
        errors.push(
            "action.yml failure summary must include review-required reasons".to_string(),
        );
    }
    if !failure_summary_lines
        .iter()
        .any(|line| line == "repro=(perfgate check --config \"${{ inputs.config }}\")")
        || !failure_summary_lines
            .iter()
            .any(|line| line == "echo \"Reproduce locally:\"")
    {
        errors.push(
            "action.yml failure summary must print a local perfgate check reproduction command"
                .to_string(),
        );
    }
    if !failure_summary_lines.iter().any(|line| {
        line == "decision_repro=(perfgate decision evaluate --config \"${{ inputs.config }}\")"
    }) || !failure_summary_lines
        .iter()
        .any(|line| line == "decision_repro_line=\"\"")
        || !failure_summary_lines
            .iter()
            .any(|line| line == "echo \"  ${decision_repro_line}\"")
        || !failure_summary_lines
            .iter()
            .any(|line| line == "echo \"${decision_repro_line}\"")
    {
        errors.push(
            "action.yml failure summary must print a local perfgate decision reproduction command"
                .to_string(),
        );
    }
    if failure_summary_lines
        .iter()
        .any(|line| line.starts_with("echo \"```"))
        || !failure_summary_lines
            .iter()
            .any(|line| line == "printf '%s\\n' '```bash'")
        || !failure_summary_lines
            .iter()
            .any(|line| line == "printf '%s\\n' '```text'")
        || !failure_summary_lines
            .iter()
            .any(|line| line == "printf '%s\\n' '```'")
    {
        errors.push(
            "action.yml failure summary must emit Markdown code fences without Bash command substitution"
                .to_string(),
        );
    }
    if !failure_summary_lines
        .iter()
        .any(|line| line == "echo \"### perfgate local reproduction\"")
        || !failure_summary_lines
            .iter()
            .any(|line| line == "} >> \"${GITHUB_STEP_SUMMARY}\"")
    {
        errors.push("action.yml failure summary must write to GITHUB_STEP_SUMMARY".to_string());
    }
    if !failure_summary_lines.iter().any(|line| {
        line.contains("-name run.json")
            && line.contains("-name compare.json")
            && line.contains("-name report.json")
            && line.contains("-name probe-compare.json")
            && line.contains("-name scenario.json")
            && line.contains("-name tradeoff.json")
            && line.contains("-name decision.md")
            && line.contains("-name decision.index.json")
            && line.contains("-name comment.md")
            && line.contains("-name 'perfgate.*.json'")
    }) {
        errors.push(
            "action.yml failure summary must list perfgate receipt and probe evidence files"
                .to_string(),
        );
    }
    if !failure_summary_lines
        .iter()
        .any(|line| line == "has_no_baseline_reason() {")
        || !failure_summary_lines
            .iter()
            .any(|line| line.contains("no_baseline"))
        || !failure_summary_lines.iter().any(|line| {
            line == "echo \"  perfgate baseline promote --config ${{ inputs.config }} --all\""
        })
    {
        errors.push(
            "action.yml failure summary must include a baseline promotion hint when no baseline evidence appears"
                .to_string(),
        );
    }
    if !failure_summary_lines.iter().any(|line| {
        line == "artifact_name=\"${{ inputs.artifact_name }}-${{ github.run_id }}-${{ github.run_attempt }}\""
    }) || !failure_summary_lines
        .iter()
        .any(|line| line == "echo \"Uploaded artifact: ${artifact_name}\"")
    {
        errors.push("action.yml failure summary must include the uploaded artifact name".to_string());
    }

    if !raw_action.contains("out=\"${{ steps.resolve_out_dir.outputs.out_dir }}\"")
        || !raw_action.contains("path: ${{ steps.resolve_out_dir.outputs.out_dir }}")
    {
        errors.push(
            "action.yml comment and artifact upload steps must use the resolved artifact directory"
                .to_string(),
        );
    }

    let binstall = toml_path(&manifest, &["package", "metadata", "binstall"]);
    if toml_str_at(binstall, &["pkg-url"])
        != Some("{ repo }/releases/download/v{ version }/perfgate-{ target }.tar.gz")
    {
        errors.push(
            "perfgate-cli binstall metadata must point at perfgate-{target}.tar.gz release assets"
                .to_string(),
        );
    }
    if toml_str_at(
        binstall,
        &["overrides", "x86_64-pc-windows-msvc", "pkg-url"],
    ) != Some("{ repo }/releases/download/v{ version }/perfgate-{ target }.zip")
    {
        errors.push(
            "perfgate-cli Windows binstall metadata must point at perfgate-{target}.zip release assets"
                .to_string(),
        );
    }
    if toml_str_at(binstall, &["bin-dir"]) != Some("perfgate{ binary-ext }") {
        errors.push(
            "perfgate-cli binstall metadata must unpack the perfgate binary from the archive"
                .to_string(),
        );
    }

    errors
}

fn collect_workflow_policy_errors(workflows_dir: &Path) -> anyhow::Result<Vec<String>> {
    if !workflows_dir.exists() {
        return Ok(Vec::new());
    }

    let mut workflows = Vec::new();
    for entry in fs::read_dir(workflows_dir)
        .with_context(|| format!("reading {}", workflows_dir.display()))?
    {
        let path = entry?.path();
        let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
            continue;
        };
        if !matches!(extension, "yml" | "yaml") {
            continue;
        }

        let content =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        workflows.push((path.display().to_string(), content));
    }

    Ok(collect_workflow_policy_errors_from_entries(
        workflows
            .iter()
            .map(|(path, content)| (path.as_str(), content.as_str())),
    ))
}

fn collect_workflow_policy_errors_from_entries<'a>(
    workflows: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Vec<String> {
    let forbidden_pr_merge =
        Regex::new(r"\bgh\s+pr\s+merge\b").expect("workflow merge regex must compile");
    let mut errors = Vec::new();

    for (path, content) in workflows {
        let workflow = match yaml_serde::from_str::<WorkflowDefinition>(content) {
            Ok(workflow) => workflow,
            Err(err) => {
                errors.push(format!("{path} must parse as workflow YAML: {err}"));
                continue;
            }
        };

        for (job_name, job) in workflow.jobs {
            for (index, step) in job.steps.iter().enumerate() {
                let Some(run) = step.run.as_deref() else {
                    continue;
                };
                for line in active_shell_lines(run) {
                    if forbidden_pr_merge.is_match(&line) {
                        let step_name = step
                            .name
                            .as_deref()
                            .map_or_else(|| format!("#{index}"), str::to_string);
                        errors.push(format!(
                            "{path} job `{job_name}` step `{step_name}` must not run `gh pr merge`; generated PRs require maintainer review and squash merge"
                        ));
                    }
                }
            }
        }
    }

    errors
}

fn collect_action_summary_example_errors(summary_examples: &str) -> Vec<String> {
    let mut errors = Vec::new();
    let required_examples = [
        ("missing baseline", "## Missing Baseline"),
        ("policy failure", "## Policy Failure"),
        (
            "warn with accepted tradeoff",
            "## Warn With Accepted Tradeoff",
        ),
        ("review required", "## Review Required"),
        ("artifact upload list", "## Artifact Upload List"),
        ("decision-enabled failure", "## Decision-Enabled Failure"),
        ("missing benchmark command", "## Missing Benchmark Command"),
        ("wrong baseline path", "## Wrong Baseline Path"),
        ("artifact upload disabled", "## Artifact Upload Disabled"),
        (
            "decision missing probe evidence",
            "## Decision Missing Probe Evidence",
        ),
        ("server upload failed", "## Server Upload Failed"),
        (
            "review required fail policy",
            "## Review Required Fail Policy",
        ),
        (
            "windows path or shell quoting",
            "## Windows Path Or Shell Quoting",
        ),
    ];
    for (name, heading) in required_examples {
        if !summary_examples.contains(heading) {
            errors.push(format!(
                "action failure summary examples must include the {name} golden example"
            ));
        }
    }

    let required_copy = [
        ("verdict counts", "Verdict:"),
        (
            "local check reproduction",
            "perfgate check --config perfgate.toml",
        ),
        (
            "baseline promotion hint",
            "perfgate baseline promote --config perfgate.toml --all",
        ),
        (
            "decision reproduction",
            "perfgate decision evaluate --config perfgate.toml",
        ),
        ("review-required reason", "Review required:"),
        ("uploaded artifact name", "Uploaded artifact:"),
        ("compare receipt", "compare.json"),
        ("probe compare receipt", "probe-compare.json"),
        ("scenario receipt", "scenario.json"),
        ("tradeoff receipt", "tradeoff.json"),
        ("decision markdown", "decision.md"),
        ("decision index", "decision.index.json"),
        (
            "missing command setup failure",
            "no perfgate receipt files found",
        ),
        ("wrong baseline path guidance", "wrong `baseline_dir`"),
        (
            "artifact upload disabled guidance",
            "upload_artifact: \"false\"",
        ),
        ("missing probe evidence", "probe evidence missing"),
        ("server upload failure", "server upload failure"),
        ("review-required fail policy", "review_required: \"fail\""),
        ("windows path guidance", "Windows Path Or Shell Quoting"),
        ("policy posture summary", "Policy posture:"),
        (
            "policy doctor command",
            "perfgate policy doctor --config perfgate.toml",
        ),
        (
            "policy review packet command",
            "perfgate policy review-packet --config perfgate.toml",
        ),
        (
            "advisory posture guardrail",
            "make advisory maturity output blocking",
        ),
    ];
    for (name, phrase) in required_copy {
        if !summary_examples.contains(phrase) {
            errors.push(format!(
                "action failure summary examples must include {name}: {phrase}"
            ));
        }
    }

    errors
}

#[derive(Debug, Deserialize)]
struct ActionDefinition {
    #[serde(default)]
    inputs: BTreeMap<String, ActionInput>,
    runs: ActionRuns,
}

impl ActionDefinition {
    fn step(&self, name: &str) -> Option<&ActionStep> {
        self.runs
            .steps
            .iter()
            .find(|step| step.name.as_deref() == Some(name))
    }

    fn step_run(&self, name: &str) -> Option<&str> {
        self.step(name).and_then(|step| step.run.as_deref())
    }
}

#[derive(Debug, Deserialize)]
struct ActionInput {
    description: Option<String>,
    default: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ActionRuns {
    #[serde(default)]
    steps: Vec<ActionStep>,
}

#[derive(Debug, Deserialize)]
struct ActionStep {
    id: Option<String>,
    name: Option<String>,
    run: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorkflowDefinition {
    #[serde(default)]
    jobs: BTreeMap<String, WorkflowJob>,
}

#[derive(Debug, Deserialize)]
struct WorkflowJob {
    #[serde(default)]
    steps: Vec<ActionStep>,
}

fn active_shell_lines(script: &str) -> Vec<String> {
    script
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(String::from)
        .collect()
}

fn toml_path<'a>(value: &'a toml::Value, path: &[&str]) -> Option<&'a toml::Value> {
    path.iter()
        .try_fold(value, |current, segment| current.get(*segment))
}

fn toml_str_at<'a>(value: Option<&'a toml::Value>, path: &[&str]) -> Option<&'a str> {
    let value = value?;
    toml_path(value, path)?.as_str()
}

#[derive(Debug, Deserialize)]
struct CargoMetadata {
    packages: Vec<MetadataPackage>,
}

#[derive(Debug, Deserialize)]
struct MetadataPackage {
    name: String,
    manifest_path: PathBuf,
    publish: Option<Vec<String>>,
    readme: Option<PathBuf>,
    dependencies: Vec<MetadataDependency>,
}

#[derive(Debug, Deserialize)]
struct MetadataDependency {
    name: String,
    kind: Option<String>,
    path: Option<PathBuf>,
}

fn cmd_publish_check(
    packages: Vec<String>,
    package_list: bool,
    dry_run: bool,
    allow_dirty: bool,
) -> anyhow::Result<()> {
    let metadata = load_cargo_metadata()?;
    let errors = collect_publish_errors(&metadata);

    if !errors.is_empty() {
        println!("Found {} publish metadata error(s):", errors.len());
        for error in &errors {
            println!("  - {}", error);
        }

        anyhow::bail!(
            "{} publish metadata issue(s) found. Fix packaging before release.",
            errors.len()
        );
    }

    println!("  OK  publishable workspace packages pass static packaging checks");

    if dry_run && packages.is_empty() {
        anyhow::bail!(
            "`publish-check --dry-run` requires at least one `--package <name>` because \
             Cargo verifies package dependencies against crates.io, not unpublished workspace crates"
        );
    }

    if package_list || dry_run {
        let packages = select_publishable_packages(&metadata, &packages)?;
        println!("      publishable packages: {}", packages.join(", "));
        let cargo_config_path = if dry_run {
            publish_check_dry_run_config(&packages)?
        } else {
            None
        };

        for package in &packages {
            if package_list {
                run_cargo_args(cargo_package_list_args(package, allow_dirty))
                    .with_context(|| format!("checking package file list for {package}"))?;
            }
            if dry_run {
                run_cargo_args(cargo_publish_dry_run_args(
                    package,
                    allow_dirty,
                    cargo_config_path.as_deref(),
                ))
                .with_context(|| format!("running publish dry-run for {package}"))?;
            }
        }
    }

    Ok(())
}

fn publish_check_dry_run_config(packages: &[String]) -> anyhow::Result<Option<PathBuf>> {
    let mut patch_crates = std::collections::BTreeSet::new();
    for package in packages {
        for crate_name in publish_check_patch_crates(package) {
            patch_crates.insert(crate_name.to_string());
        }
    }

    if patch_crates.is_empty() {
        return Ok(None);
    }

    let path = std::env::temp_dir().join("perfgate-publish-dry-run.toml");
    let workspace_root = std::env::current_dir()?;
    let mut contents = String::from("[patch.crates-io]\n");
    for package in patch_crates {
        let package = package.as_str();
        let package_root = workspace_root.join("crates").join(package);
        let package_root = package_root.to_string_lossy().replace('\\', "/");
        contents.push_str(&format!("{package} = {{ path = \"{package_root}\" }}\n"));
    }
    fs::write(&path, contents)
        .with_context(|| format!("writing publish dry-run patch config to {}", path.display()))?;
    Ok(Some(path))
}

fn publish_check_patch_crates(package: &str) -> &'static [&'static str] {
    match package {
        "perfgate" => &["perfgate-types"],
        "perfgate-client" => &["perfgate-types"],
        "perfgate-server" => &["perfgate-types", "perfgate", "perfgate-client"],
        "perfgate-cli" => &[
            "perfgate-types",
            "perfgate",
            "perfgate-client",
            "perfgate-server",
        ],
        _ => &[],
    }
}

fn load_cargo_metadata() -> anyhow::Result<CargoMetadata> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let output = std::process::Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .context("running cargo metadata")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("cargo metadata failed: {}", stderr.trim());
    }

    serde_json::from_slice(&output.stdout).context("parsing cargo metadata JSON")
}

fn collect_publish_errors(metadata: &CargoMetadata) -> Vec<String> {
    let package_map: BTreeMap<&str, &MetadataPackage> = metadata
        .packages
        .iter()
        .map(|package| (package.name.as_str(), package))
        .collect();

    let mut errors = Vec::new();

    for package in metadata
        .packages
        .iter()
        .filter(|package| is_publishable(package))
    {
        if let Some(readme) = &package.readme {
            let readme_path = resolve_manifest_relative_path(&package.manifest_path, readme);
            if !readme_path.exists() {
                errors.push(format!(
                    "{} declares readme '{}' but the file does not exist",
                    package.name,
                    readme_path.display()
                ));
            }
        }

        for dependency in package
            .dependencies
            .iter()
            .filter(|dependency| dependency.kind.as_deref() != Some("dev"))
        {
            if dependency.path.is_none() {
                continue;
            }

            let Some(dep_package) = package_map.get(dependency.name.as_str()) else {
                continue;
            };

            if !is_publishable(dep_package) {
                errors.push(format!(
                    "{} depends on workspace crate {} which is not publishable",
                    package.name, dependency.name
                ));
            }
        }
    }

    errors
}

fn ordered_publishable_packages(metadata: &CargoMetadata) -> Vec<String> {
    let mut publishable: BTreeSet<String> = metadata
        .packages
        .iter()
        .filter(|package| is_publishable(package))
        .map(|package| package.name.clone())
        .collect();

    let mut ordered = Vec::new();
    for package in TARGET_PUBLIC_PACKAGES {
        if publishable.remove(package) {
            ordered.push(package.to_string());
        }
    }
    ordered.extend(publishable);
    ordered
}

fn select_publishable_packages(
    metadata: &CargoMetadata,
    requested: &[String],
) -> anyhow::Result<Vec<String>> {
    let ordered = ordered_publishable_packages(metadata);
    if requested.is_empty() {
        return Ok(ordered);
    }

    let publishable: BTreeSet<&str> = ordered.iter().map(String::as_str).collect();
    let requested: BTreeSet<&str> = requested.iter().map(String::as_str).collect();
    let unknown: Vec<_> = requested
        .iter()
        .filter(|package| !publishable.contains(**package))
        .copied()
        .collect();
    if !unknown.is_empty() {
        anyhow::bail!(
            "requested package(s) are not publishable workspace packages: {}",
            unknown.join(", ")
        );
    }

    Ok(ordered
        .into_iter()
        .filter(|package| requested.contains(package.as_str()))
        .collect())
}

fn cargo_package_list_args(package: &str, allow_dirty: bool) -> Vec<String> {
    let mut args = vec![
        "package".to_string(),
        "-p".to_string(),
        package.to_string(),
        "--list".to_string(),
    ];
    if allow_dirty {
        args.push("--allow-dirty".to_string());
    }
    args
}

fn cargo_publish_dry_run_args(
    package: &str,
    allow_dirty: bool,
    cargo_config_path: Option<&Path>,
) -> Vec<String> {
    let mut args = vec!["publish".to_string(), "-p".to_string(), package.to_string()];
    if let Some(cargo_config_path) = cargo_config_path {
        args.push("--config".to_string());
        args.push(cargo_config_path.to_string_lossy().to_string());
    }
    args.push("--dry-run".to_string());
    if allow_dirty {
        args.push("--allow-dirty".to_string());
    }
    args
}

fn run_cargo_args(args: Vec<String>) -> anyhow::Result<()> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    println!("      running: {} {}", cargo, args.join(" "));
    let status = std::process::Command::new(&cargo)
        .args(&args)
        .status()
        .with_context(|| format!("running {} {}", cargo, args.join(" ")))?;

    if !status.success() {
        anyhow::bail!("{} {} failed: {}", cargo, args.join(" "), status);
    }

    Ok(())
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
struct NoPanicIdentity {
    path: String,
    family: String,
    selector_kind: String,
    selector_callee: String,
    snippet: String,
}

#[derive(Debug, Clone)]
struct NoPanicFinding {
    identity: NoPanicIdentity,
    count: u32,
}

#[derive(Debug, Deserialize)]
struct NoPanicAllowlist {
    schema_version: String,
    #[serde(default)]
    allow: Vec<NoPanicAllowance>,
}

#[derive(Debug, Deserialize)]
struct NoPanicBaseline {
    schema_version: String,
    #[serde(default)]
    baseline: Vec<NoPanicBaselineEntry>,
}

#[derive(Debug, Deserialize)]
struct NoPanicAllowance {
    path: String,
    family: String,
    selector_kind: String,
    selector_callee: String,
    snippet: String,
    count: u32,
    owner: String,
    reason: String,
    review_after: String,
}

#[derive(Debug, Clone, Deserialize)]
struct NoPanicBaselineEntry {
    path: String,
    family: String,
    selector_kind: String,
    selector_callee: String,
    snippet: String,
    count: u32,
}

impl NoPanicAllowance {
    fn identity(&self) -> NoPanicIdentity {
        NoPanicIdentity {
            path: self.path.clone(),
            family: self.family.clone(),
            selector_kind: self.selector_kind.clone(),
            selector_callee: self.selector_callee.clone(),
            snippet: self.snippet.clone(),
        }
    }
}

impl NoPanicBaselineEntry {
    fn identity(&self) -> NoPanicIdentity {
        NoPanicIdentity {
            path: self.path.clone(),
            family: self.family.clone(),
            selector_kind: self.selector_kind.clone(),
            selector_callee: self.selector_callee.clone(),
            snippet: self.snippet.clone(),
        }
    }
}

fn cmd_check_no_panic_family(
    root: &Path,
    allowlist: &Path,
    baseline: &Path,
    write_baseline: bool,
) -> anyhow::Result<()> {
    let findings = scan_no_panic_family(root)?;
    let allowlist = read_no_panic_allowlist(allowlist)?;
    let baseline_exists = baseline.exists();
    let no_panic_baseline = if baseline_exists {
        Some(read_no_panic_baseline(baseline)?)
    } else if write_baseline {
        None
    } else {
        anyhow::bail!(
            "no-panic baseline {} is missing; run `cargo run -p xtask -- policy check-no-panic-family --write-baseline` to create the initial generated baseline",
            baseline.display()
        );
    };
    let errors = collect_no_panic_policy_errors(&findings, &allowlist, no_panic_baseline.as_ref());

    if !errors.is_empty() {
        println!("Found {} no-panic policy error(s):", errors.len());
        for error in &errors {
            println!("  - {}", error);
        }

        anyhow::bail!(
            "{} no-panic policy issue(s) found. Update policy/no-panic-allowlist.toml or the generated baseline.",
            errors.len()
        );
    }

    if write_baseline {
        write_no_panic_baseline(baseline, &findings, &allowlist)?;
        println!(
            "  OK  wrote generated no-panic baseline to {}",
            baseline.display()
        );
        return Ok(());
    }

    let Some(baseline) = no_panic_baseline.as_ref() else {
        anyhow::bail!("no-panic baseline was not loaded");
    };
    let total_callsites: u32 = findings.iter().map(|finding| finding.count).sum();
    let allowed_callsites: u32 = allowlist
        .allow
        .iter()
        .map(|allowance| allowance.count)
        .sum();
    let baseline_callsites: u32 = baseline.baseline.iter().map(|entry| entry.count).sum();
    let baseline_refresh_candidates =
        count_no_panic_baseline_refresh_candidates(&findings, &allowlist, baseline);
    let unbaselined_identities = findings
        .iter()
        .filter(|finding| {
            !is_no_panic_allowed(&finding.identity, &allowlist)
                && !baseline
                    .baseline
                    .iter()
                    .any(|entry| entry.identity() == finding.identity)
        })
        .count();

    println!("  OK  no-panic baseline rejects new unallowlisted debt");
    println!(
        "      scanned {} exact panic-family identity/identities ({} callsite(s))",
        findings.len(),
        total_callsites
    );
    println!(
        "      allowlist covers {} callsite(s); baseline covers {} callsite(s)",
        allowed_callsites, baseline_callsites
    );
    println!(
        "      {} unbaselined identity/identities; {} baseline entry/entries can shrink or disappear on refresh",
        unbaselined_identities, baseline_refresh_candidates
    );
    Ok(())
}

fn read_no_panic_allowlist(path: &Path) -> anyhow::Result<NoPanicAllowlist> {
    let content =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let allowlist: NoPanicAllowlist =
        toml::from_str(&content).with_context(|| format!("parsing {}", path.display()))?;
    Ok(allowlist)
}

fn read_no_panic_baseline(path: &Path) -> anyhow::Result<NoPanicBaseline> {
    let content =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let baseline: NoPanicBaseline =
        toml::from_str(&content).with_context(|| format!("parsing {}", path.display()))?;
    Ok(baseline)
}

fn collect_no_panic_policy_errors(
    findings: &[NoPanicFinding],
    allowlist: &NoPanicAllowlist,
    baseline: Option<&NoPanicBaseline>,
) -> Vec<String> {
    let mut errors = Vec::new();

    if allowlist.schema_version != "1.0" {
        errors.push(format!(
            "policy/no-panic-allowlist.toml schema_version must be 1.0, found {}",
            allowlist.schema_version
        ));
    }

    let mut finding_counts = BTreeMap::new();
    for finding in findings {
        finding_counts.insert(finding.identity.clone(), finding.count);
    }

    let mut seen = BTreeSet::new();
    for allowance in &allowlist.allow {
        validate_no_panic_allowance(allowance, &mut errors);
        let identity = allowance.identity();
        if !seen.insert(identity.clone()) {
            errors.push(format!(
                "duplicate no-panic allowlist identity: {}",
                format_no_panic_identity(&identity)
            ));
            continue;
        }

        match finding_counts.get(&identity) {
            Some(actual) if *actual == allowance.count => {}
            Some(actual) => errors.push(format!(
                "no-panic allowlist count mismatch for {}: expected {}, found {}",
                format_no_panic_identity(&identity),
                allowance.count,
                actual
            )),
            None => errors.push(format!(
                "no-panic allowlist identity is stale or missing from scan: {}",
                format_no_panic_identity(&identity)
            )),
        }
    }

    if let Some(baseline) = baseline {
        collect_no_panic_baseline_errors(findings, allowlist, baseline, &mut errors);
    }

    errors
}

fn collect_no_panic_baseline_errors(
    findings: &[NoPanicFinding],
    allowlist: &NoPanicAllowlist,
    baseline: &NoPanicBaseline,
    errors: &mut Vec<String>,
) {
    if baseline.schema_version != "1.0" {
        errors.push(format!(
            "policy/no-panic-baseline.toml schema_version must be 1.0, found {}",
            baseline.schema_version
        ));
    }

    let mut baseline_counts = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for entry in &baseline.baseline {
        validate_no_panic_baseline_entry(entry, errors);
        let identity = entry.identity();
        if !seen.insert(identity.clone()) {
            errors.push(format!(
                "duplicate no-panic baseline identity: {}",
                format_no_panic_identity(&identity)
            ));
            continue;
        }
        baseline_counts.insert(identity, entry.count);
    }

    for finding in findings {
        if is_no_panic_allowed(&finding.identity, allowlist) {
            continue;
        }
        match baseline_counts.get(&finding.identity) {
            Some(expected) if finding.count <= *expected => {}
            Some(expected) => errors.push(format!(
                "no-panic baseline count increased for {}: baseline {}, found {}",
                format_no_panic_identity(&finding.identity),
                expected,
                finding.count
            )),
            None => errors.push(format!(
                "new unbaselined panic-family identity: {} count={}",
                format_no_panic_identity(&finding.identity),
                finding.count
            )),
        }
    }
}

fn count_no_panic_baseline_refresh_candidates(
    findings: &[NoPanicFinding],
    allowlist: &NoPanicAllowlist,
    baseline: &NoPanicBaseline,
) -> usize {
    let finding_counts: BTreeMap<_, _> = findings
        .iter()
        .map(|finding| (finding.identity.clone(), finding.count))
        .collect();

    baseline
        .baseline
        .iter()
        .filter(|entry| {
            let identity = entry.identity();
            if is_no_panic_allowed(&identity, allowlist) {
                return true;
            }
            match finding_counts.get(&identity) {
                Some(count) => *count < entry.count,
                None => true,
            }
        })
        .count()
}

fn is_no_panic_allowed(identity: &NoPanicIdentity, allowlist: &NoPanicAllowlist) -> bool {
    allowlist
        .allow
        .iter()
        .any(|allowance| allowance.identity() == *identity)
}

fn write_no_panic_baseline(
    path: &Path,
    findings: &[NoPanicFinding],
    allowlist: &NoPanicAllowlist,
) -> anyhow::Result<()> {
    let mut output = String::new();
    output.push_str("schema_version = \"1.0\"\n");
    output.push_str(
        "generated_by = \"cargo run -p xtask -- policy check-no-panic-family --write-baseline\"\n",
    );

    for finding in findings {
        if is_no_panic_allowed(&finding.identity, allowlist) {
            continue;
        }
        output.push('\n');
        output.push_str("[[baseline]]\n");
        output.push_str("path = ");
        output.push_str(&toml_basic_string(&finding.identity.path));
        output.push('\n');
        output.push_str("family = ");
        output.push_str(&toml_basic_string(&finding.identity.family));
        output.push('\n');
        output.push_str("selector_kind = ");
        output.push_str(&toml_basic_string(&finding.identity.selector_kind));
        output.push('\n');
        output.push_str("selector_callee = ");
        output.push_str(&toml_basic_string(&finding.identity.selector_callee));
        output.push('\n');
        output.push_str("snippet = ");
        output.push_str(&toml_basic_string(&finding.identity.snippet));
        output.push('\n');
        output.push_str("count = ");
        output.push_str(&finding.count.to_string());
        output.push('\n');
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(path, output).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn toml_basic_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_ascii_graphic() || ch == ' ' => out.push(ch),
            ch => {
                let code = ch as u32;
                if code <= 0xFFFF {
                    out.push_str(&format!("\\u{code:04X}"));
                } else {
                    out.push_str(&format!("\\U{code:08X}"));
                }
            }
        }
    }
    out.push('"');
    out
}

fn validate_no_panic_baseline_entry(entry: &NoPanicBaselineEntry, errors: &mut Vec<String>) {
    let identity = entry.identity();
    let formatted = format_no_panic_identity(&identity);
    for (field, value) in [
        ("path", entry.path.as_str()),
        ("family", entry.family.as_str()),
        ("selector_kind", entry.selector_kind.as_str()),
        ("selector_callee", entry.selector_callee.as_str()),
        ("snippet", entry.snippet.as_str()),
    ] {
        if value.trim().is_empty() {
            errors.push(format!(
                "no-panic baseline field `{field}` must be non-empty for {formatted}"
            ));
        }
    }
    if entry.count == 0 {
        errors.push(format!(
            "no-panic baseline count must be positive for {formatted}"
        ));
    }
}

fn validate_no_panic_allowance(allowance: &NoPanicAllowance, errors: &mut Vec<String>) {
    let identity = allowance.identity();
    let formatted = format_no_panic_identity(&identity);
    for (field, value) in [
        ("path", allowance.path.as_str()),
        ("family", allowance.family.as_str()),
        ("selector_kind", allowance.selector_kind.as_str()),
        ("selector_callee", allowance.selector_callee.as_str()),
        ("snippet", allowance.snippet.as_str()),
        ("owner", allowance.owner.as_str()),
        ("reason", allowance.reason.as_str()),
        ("review_after", allowance.review_after.as_str()),
    ] {
        if value.trim().is_empty() {
            errors.push(format!(
                "no-panic allowlist field `{field}` must be non-empty for {formatted}"
            ));
        }
    }
    if allowance.count == 0 {
        errors.push(format!(
            "no-panic allowlist count must be positive for {formatted}"
        ));
    }
}

fn scan_no_panic_family(root: &Path) -> anyhow::Result<Vec<NoPanicFinding>> {
    let macro_re = Regex::new(r"\b(panic|unreachable|todo|unimplemented)!\s*\(")?;
    let method_re = Regex::new(r"\.(unwrap|expect)\s*\(")?;
    let mut files = Vec::new();
    collect_rust_files(root, &mut files)?;
    files.sort();

    let mut counts = BTreeMap::<NoPanicIdentity, u32>::new();
    for file in files {
        let content =
            fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?;
        scan_no_panic_file(root, &file, &content, &macro_re, &method_re, &mut counts);
    }

    Ok(counts
        .into_iter()
        .map(|(identity, count)| NoPanicFinding { identity, count })
        .collect())
}

fn collect_rust_files(root: &Path, files: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    if should_skip_scan_path(root) {
        return Ok(());
    }
    if root.is_file() {
        if root.extension().is_some_and(|extension| extension == "rs") {
            files.push(root.to_path_buf());
        }
        return Ok(());
    }
    if !root.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(root).with_context(|| format!("reading {}", root.display()))? {
        let path = entry?.path();
        collect_rust_files(&path, files)?;
    }
    Ok(())
}

fn should_skip_scan_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| matches!(name, ".git" | "target"))
}

fn scan_no_panic_file(
    root: &Path,
    file: &Path,
    content: &str,
    macro_re: &Regex,
    method_re: &Regex,
    counts: &mut BTreeMap<NoPanicIdentity, u32>,
) {
    let policy_path = normalize_policy_path(root, file);
    let mut mask_state = RustMaskState::default();
    for line in content.lines() {
        let masked = mask_rust_code_line(line, &mut mask_state);
        for capture in macro_re.captures_iter(&masked) {
            let callee = format!("{}!", &capture[1]);
            let identity = NoPanicIdentity {
                path: policy_path.clone(),
                family: capture[1].to_string(),
                selector_kind: "macro".to_string(),
                selector_callee: callee,
                snippet: line.trim().to_string(),
            };
            *counts.entry(identity).or_default() += 1;
        }
        for capture in method_re.captures_iter(&masked) {
            let identity = NoPanicIdentity {
                path: policy_path.clone(),
                family: capture[1].to_string(),
                selector_kind: "method".to_string(),
                selector_callee: capture[1].to_string(),
                snippet: line.trim().to_string(),
            };
            *counts.entry(identity).or_default() += 1;
        }
    }
}

#[derive(Debug, Default)]
struct RustMaskState {
    block_comment_depth: usize,
    raw_string_hashes: Option<usize>,
}

fn mask_rust_code_line(line: &str, state: &mut RustMaskState) -> String {
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut index = 0;
    let mut in_string = false;
    let mut in_char = false;
    let mut escaped = false;

    while index < bytes.len() {
        if let Some(hashes) = state.raw_string_hashes {
            if bytes[index] == b'"' && raw_string_closes(bytes, index, hashes) {
                out.push(' ');
                for _ in 0..hashes {
                    if index + 1 < bytes.len() {
                        index += 1;
                        out.push(' ');
                    }
                }
                state.raw_string_hashes = None;
                index += 1;
            } else {
                out.push(' ');
                index += 1;
            }
            continue;
        }

        if state.block_comment_depth > 0 {
            if starts_with(bytes, index, b"/*") {
                state.block_comment_depth += 1;
                out.push_str("  ");
                index += 2;
            } else if starts_with(bytes, index, b"*/") {
                state.block_comment_depth -= 1;
                out.push_str("  ");
                index += 2;
            } else {
                out.push(' ');
                index += 1;
            }
            continue;
        }

        if in_string {
            let current = bytes[index];
            out.push(' ');
            index += 1;
            if escaped {
                escaped = false;
            } else if current == b'\\' {
                escaped = true;
            } else if current == b'"' {
                in_string = false;
            }
            continue;
        }

        if in_char {
            let current = bytes[index];
            out.push(' ');
            index += 1;
            if escaped {
                escaped = false;
            } else if current == b'\\' {
                escaped = true;
            } else if current == b'\'' {
                in_char = false;
            }
            continue;
        }

        if starts_with(bytes, index, b"//") {
            out.extend(std::iter::repeat_n(' ', bytes.len() - index));
            break;
        }
        if starts_with(bytes, index, b"/*") {
            state.block_comment_depth += 1;
            out.push_str("  ");
            index += 2;
            continue;
        }
        if let Some((prefix_len, hashes)) = raw_string_start(bytes, index) {
            out.extend(std::iter::repeat_n(' ', prefix_len));
            index += prefix_len;
            state.raw_string_hashes = Some(hashes);
            continue;
        }
        if bytes[index] == b'"' {
            in_string = true;
            out.push(' ');
            index += 1;
            continue;
        }
        if bytes[index] == b'\'' && looks_like_char_literal(bytes, index) {
            in_char = true;
            out.push(' ');
            index += 1;
            continue;
        }

        out.push(bytes[index] as char);
        index += 1;
    }

    out
}

fn raw_string_start(bytes: &[u8], index: usize) -> Option<(usize, usize)> {
    let mut cursor = index;
    if bytes.get(cursor) == Some(&b'b') {
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'r') {
        return None;
    }
    cursor += 1;
    let mut hashes = 0;
    while bytes.get(cursor) == Some(&b'#') {
        hashes += 1;
        cursor += 1;
    }
    if bytes.get(cursor) == Some(&b'"') {
        Some((cursor - index + 1, hashes))
    } else {
        None
    }
}

fn looks_like_char_literal(bytes: &[u8], quote_index: usize) -> bool {
    let Some(first) = bytes.get(quote_index + 1) else {
        return false;
    };
    let closing = if *first == b'\\' {
        quote_index + 3
    } else {
        quote_index + 2
    };
    bytes.get(closing) == Some(&b'\'')
}

fn raw_string_closes(bytes: &[u8], quote_index: usize, hashes: usize) -> bool {
    (0..hashes).all(|offset| bytes.get(quote_index + 1 + offset) == Some(&b'#'))
}

fn starts_with(bytes: &[u8], index: usize, needle: &[u8]) -> bool {
    bytes
        .get(index..index + needle.len())
        .is_some_and(|candidate| candidate == needle)
}

fn normalize_policy_path(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    relative
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(part) => part.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn format_no_panic_identity(identity: &NoPanicIdentity) -> String {
    format!(
        "{} {} {} {} {:?}",
        identity.path,
        identity.family,
        identity.selector_kind,
        identity.selector_callee,
        identity.snippet
    )
}

fn cmd_public_surface(
    public_policy: &Path,
    absorbed_policy: &Path,
    strict: bool,
) -> anyhow::Result<()> {
    let metadata = load_cargo_metadata()?;
    let public_crates = read_public_crate_policy(public_policy)?;
    let absorbed_crates = read_absorbed_crate_policy(absorbed_policy)?;
    let errors = collect_public_surface_errors(&metadata, &public_crates, &absorbed_crates, strict);

    if !errors.is_empty() {
        println!("Found {} public-surface policy error(s):", errors.len());
        for error in &errors {
            println!("  - {}", error);
        }

        anyhow::bail!(
            "{} public-surface policy issue(s) found. Update policy or crate publication state.",
            errors.len()
        );
    }

    let publishable: Vec<_> = metadata
        .packages
        .iter()
        .filter(|package| is_publishable(package))
        .map(|package| package.name.as_str())
        .collect();
    let transitional: Vec<_> = publishable
        .iter()
        .copied()
        .filter(|name| !public_crates.contains(*name) && absorbed_crates.contains_key(*name))
        .collect();
    let compatibility_wrapper_count = absorbed_crates
        .values()
        .filter(|disposition| is_compatibility_wrapper_disposition(disposition))
        .count();

    println!(
        "  OK  public-surface policy accounts for {} publishable package(s)",
        publishable.len()
    );
    println!("      target public packages: {}", public_crates.len());
    if !transitional.is_empty() {
        println!(
            "      transition publishable packages with dispositions: {}",
            transitional.len()
        );
    }
    if compatibility_wrapper_count > 0 {
        println!(
            "      compatibility wrappers isolated from production deps: {}",
            compatibility_wrapper_count
        );
    }

    Ok(())
}

fn read_public_crate_policy(path: &Path) -> anyhow::Result<BTreeSet<String>> {
    let content = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut names = BTreeSet::new();

    for (idx, raw_line) in content.lines().enumerate() {
        let line = strip_policy_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        if line.contains("->") {
            anyhow::bail!(
                "{}:{} public crate policy entries must be plain package names",
                path.display(),
                idx + 1
            );
        }
        names.insert(line.to_string());
    }

    if names.is_empty() {
        anyhow::bail!("{} contains no public crate entries", path.display());
    }

    Ok(names)
}

fn read_absorbed_crate_policy(path: &Path) -> anyhow::Result<BTreeMap<String, String>> {
    let content = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut dispositions = BTreeMap::new();

    for (idx, raw_line) in content.lines().enumerate() {
        let line = strip_policy_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }

        let Some((package, disposition)) = line.split_once("->") else {
            anyhow::bail!(
                "{}:{} absorbed crate entries must use `package -> disposition`",
                path.display(),
                idx + 1
            );
        };

        let Some(package_name) = package.split_whitespace().next() else {
            anyhow::bail!("{}:{} missing package name", path.display(), idx + 1);
        };
        let disposition = disposition.trim();
        if disposition.is_empty() {
            anyhow::bail!("{}:{} missing disposition", path.display(), idx + 1);
        }

        dispositions.insert(package_name.to_string(), disposition.to_string());
    }

    Ok(dispositions)
}

fn strip_policy_comment(line: &str) -> &str {
    line.split_once('#')
        .map(|(before, _)| before)
        .unwrap_or(line)
}

const COMPATIBILITY_WRAPPER_DISPOSITION: &str = "[compatibility wrapper]";

fn is_compatibility_wrapper_disposition(disposition: &str) -> bool {
    disposition.contains(COMPATIBILITY_WRAPPER_DISPOSITION)
}

fn compatibility_wrapper_owner_path(disposition: &str) -> &str {
    disposition
        .split_once(COMPATIBILITY_WRAPPER_DISPOSITION)
        .map(|(owner, _marker)| owner.trim())
        .filter(|owner| !owner.is_empty())
        .unwrap_or_else(|| disposition.trim())
}

fn collect_public_surface_errors(
    metadata: &CargoMetadata,
    public_crates: &BTreeSet<String>,
    absorbed_crates: &BTreeMap<String, String>,
    strict: bool,
) -> Vec<String> {
    let mut errors = Vec::new();

    for public_crate in public_crates {
        let Some(package) = metadata
            .packages
            .iter()
            .find(|package| package.name == *public_crate)
        else {
            errors.push(format!(
                "policy lists public crate {} but no workspace package has that name",
                public_crate
            ));
            continue;
        };

        if !is_publishable(package) {
            errors.push(format!(
                "policy lists public crate {} but the package is not publishable",
                public_crate
            ));
        }
    }

    for public_crate in public_crates {
        if absorbed_crates.contains_key(public_crate) {
            errors.push(format!(
                "{} is listed as both public and absorbed/internal",
                public_crate
            ));
        }
    }

    for package in metadata
        .packages
        .iter()
        .filter(|package| is_publishable(package))
    {
        if public_crates.contains(&package.name) {
            continue;
        }

        if absorbed_crates.contains_key(&package.name) {
            if strict {
                errors.push(format!(
                    "{} is still publishable but is listed as absorbed/internal",
                    package.name
                ));
            }
            continue;
        }

        errors.push(format!(
            "{} is publishable but is not listed in {} or {}",
            package.name, "policy/public_crates.txt", "policy/absorbed_crates.txt"
        ));
    }

    errors.extend(collect_compatibility_wrapper_dependency_errors(
        metadata,
        absorbed_crates,
    ));
    if strict {
        errors.extend(collect_strict_public_internal_dependency_errors(
            metadata,
            public_crates,
            absorbed_crates,
        ));
    }

    errors
}

fn collect_strict_public_internal_dependency_errors(
    metadata: &CargoMetadata,
    public_crates: &BTreeSet<String>,
    absorbed_crates: &BTreeMap<String, String>,
) -> Vec<String> {
    let package_names: BTreeSet<&str> = metadata
        .packages
        .iter()
        .map(|package| package.name.as_str())
        .collect();
    let mut errors = Vec::new();

    for package in metadata
        .packages
        .iter()
        .filter(|package| public_crates.contains(&package.name))
    {
        for dependency in package
            .dependencies
            .iter()
            .filter(|dependency| dependency.kind.as_deref() != Some("dev"))
            .filter(|dependency| dependency.path.is_some())
            .filter(|dependency| package_names.contains(dependency.name.as_str()))
            .filter(|dependency| absorbed_crates.contains_key(dependency.name.as_str()))
        {
            errors.push(format!(
                "{} is a target public crate but depends on absorbed/internal package {}; absorb or route it through an allowed public crate before strict release",
                package.name, dependency.name
            ));
        }
    }

    errors
}

fn collect_compatibility_wrapper_dependency_errors(
    metadata: &CargoMetadata,
    absorbed_crates: &BTreeMap<String, String>,
) -> Vec<String> {
    let compatibility_wrappers: BTreeMap<&str, &str> = absorbed_crates
        .iter()
        .filter(|(_package, disposition)| is_compatibility_wrapper_disposition(disposition))
        .map(|(package, disposition)| {
            (
                package.as_str(),
                compatibility_wrapper_owner_path(disposition),
            )
        })
        .collect();
    if compatibility_wrappers.is_empty() {
        return Vec::new();
    }

    let package_names: BTreeSet<&str> = metadata
        .packages
        .iter()
        .map(|package| package.name.as_str())
        .collect();
    let mut errors = Vec::new();

    for package in &metadata.packages {
        for dependency in package
            .dependencies
            .iter()
            .filter(|dependency| dependency.kind.as_deref() != Some("dev"))
            .filter(|dependency| dependency.path.is_some())
            .filter(|dependency| package_names.contains(dependency.name.as_str()))
        {
            let Some(disposition) = compatibility_wrappers.get(dependency.name.as_str()) else {
                continue;
            };

            errors.push(format!(
                "{} depends on compatibility wrapper {}; use {} directly",
                package.name, dependency.name, disposition
            ));
        }
    }

    errors
}

#[derive(Debug)]
struct ArchRule {
    name: &'static str,
    sources: &'static [&'static str],
    forbidden: &'static [&'static str],
}

const ARCH_RULES: &[ArchRule] = &[
    ArchRule {
        name: "contract packages stay below runtime/app/entrypoints",
        sources: &["perfgate-types"],
        forbidden: &[
            "perfgate-client",
            "perfgate-server",
            "perfgate-cli",
            "perfgate",
        ],
    },
    ArchRule {
        name: "core/domain packages stay below I/O, presentation, and entrypoints",
        sources: &[],
        forbidden: &[
            "perfgate-client",
            "perfgate-server",
            "perfgate-cli",
            "perfgate",
        ],
    },
    ArchRule {
        name: "presentation packages stay below runtime/app/entrypoints",
        sources: &[],
        forbidden: &[
            "perfgate-client",
            "perfgate-server",
            "perfgate-cli",
            "perfgate",
        ],
    },
    ArchRule {
        name: "runtime/app packages stay below service/client/cli entrypoints",
        sources: &[],
        forbidden: &[
            "perfgate-client",
            "perfgate-server",
            "perfgate-cli",
            "perfgate",
        ],
    },
    ArchRule {
        name: "facade must stay below service/client/cli entrypoints",
        sources: &["perfgate"],
        forbidden: &["perfgate-client", "perfgate-server", "perfgate-cli"],
    },
    ArchRule {
        name: "client must not depend on server or cli",
        sources: &["perfgate-client"],
        forbidden: &["perfgate-server", "perfgate-cli"],
    },
    ArchRule {
        name: "server must not depend on cli",
        sources: &["perfgate-server"],
        forbidden: &["perfgate-cli"],
    },
];

#[derive(Debug)]
struct SourceArchRule {
    packages: &'static [&'static str],
    paths: &'static [&'static str],
    label: &'static str,
    banned_patterns: &'static [&'static str],
}

const CORE_DOMAIN_ARCH_PACKAGES: &[&str] = &[];
const CORE_DOMAIN_ARCH_PATHS: &[&str] = &["crates/perfgate/src/domain"];

const CORE_DOMAIN_BANNED_SOURCE_PATTERNS: &[&str] = &[
    "std::fs",
    "std::process",
    "tokio::fs",
    "tokio::process",
    "Command::new",
];

const PRESENTATION_ARCH_PACKAGES: &[&str] = &[];
const PRESENTATION_ARCH_PATHS: &[&str] = &[
    "crates/perfgate/src/app/export.rs",
    "crates/perfgate/src/app/render.rs",
    "crates/perfgate/src/app/render",
    "crates/perfgate/src/app/sensor.rs",
    "crates/perfgate/src/app/sensor_report.rs",
];

const PRESENTATION_BANNED_SOURCE_PATTERNS: &[&str] =
    &["std::process", "tokio::process", "Command::new"];

const SOURCE_ARCH_RULES: &[SourceArchRule] = &[
    SourceArchRule {
        packages: CORE_DOMAIN_ARCH_PACKAGES,
        paths: CORE_DOMAIN_ARCH_PATHS,
        label: "core/domain source must stay filesystem/process free",
        banned_patterns: CORE_DOMAIN_BANNED_SOURCE_PATTERNS,
    },
    SourceArchRule {
        packages: PRESENTATION_ARCH_PACKAGES,
        paths: PRESENTATION_ARCH_PATHS,
        label: "presentation source must not execute processes",
        banned_patterns: PRESENTATION_BANNED_SOURCE_PATTERNS,
    },
];

fn cmd_arch() -> anyhow::Result<()> {
    let metadata = load_cargo_metadata()?;
    let mut errors = collect_arch_dependency_errors(&metadata);
    errors.extend(collect_arch_source_errors(&metadata)?);

    if !errors.is_empty() {
        println!("Found {} architecture policy error(s):", errors.len());
        for error in &errors {
            println!("  - {}", error);
        }
        anyhow::bail!(
            "{} architecture policy issue(s) found. Keep lower layers independent.",
            errors.len()
        );
    }

    println!("  OK  architecture dependency rules hold");
    println!("      checked {} package-layer rule(s)", ARCH_RULES.len());
    println!(
        "      scanned {} source package(s) for banned filesystem/process usage",
        SOURCE_ARCH_RULES
            .iter()
            .map(|rule| rule.packages.len())
            .sum::<usize>()
    );
    println!(
        "      scanned {} source path(s) for collapsed module seams",
        SOURCE_ARCH_RULES
            .iter()
            .map(|rule| rule.paths.len())
            .sum::<usize>()
    );

    Ok(())
}

fn collect_arch_dependency_errors(metadata: &CargoMetadata) -> Vec<String> {
    let dependency_graph = workspace_dependency_graph(metadata);
    let package_names: BTreeSet<&str> = metadata
        .packages
        .iter()
        .map(|package| package.name.as_str())
        .collect();
    let mut errors = Vec::new();

    for rule in ARCH_RULES {
        for source in rule.sources {
            if !package_names.contains(source) {
                errors.push(format!(
                    "{} references missing source package {}",
                    rule.name, source
                ));
            }
        }

        for forbidden in rule.forbidden {
            if !package_names.contains(forbidden) {
                errors.push(format!(
                    "{} references missing forbidden package {}",
                    rule.name, forbidden
                ));
            }
        }

        for source in rule.sources {
            if !dependency_graph.contains_key(*source) {
                continue;
            }

            let reachable = reachable_workspace_dependencies(source, &dependency_graph);
            for forbidden in rule.forbidden {
                if reachable.contains(*forbidden) {
                    errors.push(format!(
                        "{}: {} must not depend on {}",
                        rule.name, source, forbidden
                    ));
                }
            }
        }
    }

    errors
}

fn workspace_dependency_graph(metadata: &CargoMetadata) -> BTreeMap<String, BTreeSet<String>> {
    let package_names: BTreeSet<&str> = metadata
        .packages
        .iter()
        .map(|package| package.name.as_str())
        .collect();
    let mut graph = BTreeMap::new();

    for package in &metadata.packages {
        let deps = package
            .dependencies
            .iter()
            .filter(|dependency| dependency.kind.as_deref() != Some("dev"))
            .filter(|dependency| dependency.path.is_some())
            .filter(|dependency| package_names.contains(dependency.name.as_str()))
            .map(|dependency| dependency.name.clone())
            .collect();

        graph.insert(package.name.clone(), deps);
    }

    graph
}

fn reachable_workspace_dependencies(
    source: &str,
    graph: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut stack: Vec<String> = graph
        .get(source)
        .into_iter()
        .flat_map(|deps| deps.iter().cloned())
        .collect();

    while let Some(package) = stack.pop() {
        if !seen.insert(package.clone()) {
            continue;
        }

        if let Some(deps) = graph.get(&package) {
            stack.extend(deps.iter().filter(|dep| !seen.contains(*dep)).cloned());
        }
    }

    seen
}

fn collect_arch_source_errors(metadata: &CargoMetadata) -> anyhow::Result<Vec<String>> {
    let package_map: BTreeMap<&str, &MetadataPackage> = metadata
        .packages
        .iter()
        .map(|package| (package.name.as_str(), package))
        .collect();
    let mut errors = Vec::new();

    for rule in SOURCE_ARCH_RULES {
        for package_name in rule.packages {
            let Some(package) = package_map.get(package_name) else {
                errors.push(format!(
                    "{} references missing package {}",
                    rule.label, package_name
                ));
                continue;
            };
            let Some(package_dir) = package.manifest_path.parent() else {
                continue;
            };
            let src_dir = package_dir.join("src");
            if !src_dir.is_dir() {
                continue;
            }

            for path in collect_rust_files_recursive(&src_dir)? {
                collect_arch_source_file_errors(
                    rule.label,
                    package_name,
                    &path,
                    rule.banned_patterns,
                    &mut errors,
                )?;
            }
        }

        for source_path in rule.paths {
            let path = Path::new(source_path);
            let files = collect_rust_files_from_path(path)?;
            for file in files {
                collect_arch_source_file_errors(
                    rule.label,
                    source_path,
                    &file,
                    rule.banned_patterns,
                    &mut errors,
                )?;
            }
        }
    }

    Ok(errors)
}

fn collect_arch_source_file_errors(
    label: &str,
    source_label: &str,
    path: &Path,
    banned_patterns: &[&str],
    errors: &mut Vec<String>,
) -> anyhow::Result<()> {
    let content = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    for (line_idx, line) in content.lines().enumerate() {
        let searchable = rust_code_before_comment(line);
        for pattern in banned_patterns {
            if searchable.contains(pattern) {
                errors.push(format!(
                    "{}: {} uses `{}` at {}:{}",
                    label,
                    source_label,
                    pattern,
                    path.display(),
                    line_idx + 1
                ));
            }
        }
    }

    Ok(())
}

fn collect_rust_files_from_path(path: &Path) -> anyhow::Result<Vec<PathBuf>> {
    if path.is_file() {
        if path.extension().is_some_and(|extension| extension == "rs") {
            return Ok(vec![path.to_path_buf()]);
        }
        return Ok(Vec::new());
    }

    if path.is_dir() {
        return collect_rust_files_recursive(path);
    }

    anyhow::bail!(
        "architecture source path does not exist: {}",
        path.display()
    );
}

fn collect_rust_files_recursive(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_rust_files_recursive_into(dir, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_rust_files_recursive_into(dir: &Path, files: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("read dir {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("read file type {}", path.display()))?;
        if file_type.is_dir() {
            collect_rust_files_recursive_into(&path, files)?;
        } else if file_type.is_file() && path.extension().is_some_and(|extension| extension == "rs")
        {
            files.push(path);
        }
    }

    Ok(())
}

fn rust_code_before_comment(line: &str) -> &str {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
        return "";
    }

    line.split_once("//")
        .map(|(code, _comment)| code)
        .unwrap_or(line)
}

fn is_publishable(package: &MetadataPackage) -> bool {
    if let Some(registries) = &package.publish
        && registries.is_empty()
    {
        return false;
    }
    true
}

fn resolve_manifest_relative_path(manifest_path: &Path, relative_path: &Path) -> PathBuf {
    if relative_path.is_absolute() {
        return relative_path.to_path_buf();
    }

    match manifest_path.parent() {
        Some(parent) => parent.join(relative_path),
        None => relative_path.to_path_buf(),
    }
}

fn cmd_conform(fixtures_dir: Option<PathBuf>, single_file: Option<PathBuf>) -> anyhow::Result<()> {
    let is_default_run = fixtures_dir.is_none() && single_file.is_none();

    // Load vendored schema
    let schema_path = PathBuf::from("contracts/schemas/sensor.report.v1.schema.json");
    let schema_content = fs::read_to_string(&schema_path)
        .with_context(|| format!("read {}", schema_path.display()))?;
    let schema_value: serde_json::Value =
        serde_json::from_str(&schema_content).context("parse vendored schema")?;
    let validator = jsonschema::validator_for(&schema_value)
        .map_err(|e| anyhow::anyhow!("compile schema: {}", e))?;

    let mut files_to_validate: Vec<PathBuf> = Vec::new();

    if let Some(path) = single_file {
        files_to_validate.push(path);
    } else if let Some(dir) = fixtures_dir {
        // Third-party mode: validate every JSON file in the provided directory.
        files_to_validate.extend(collect_json_files(&dir, None)?);
    } else {
        // Default: validate known sensor_report fixtures in golden + contracts dirs.
        let default_dirs = [
            PathBuf::from("crates/perfgate-cli/tests/fixtures/golden"),
            PathBuf::from("contracts/fixtures"),
        ];

        for dir in &default_dirs {
            files_to_validate.extend(collect_json_files(dir, Some("sensor_report_"))?);
        }
    }

    if files_to_validate.is_empty() {
        anyhow::bail!("no fixture files found to validate");
    }

    files_to_validate.sort();

    let mut errors = 0u32;
    for path in &files_to_validate {
        let content =
            fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let instance: serde_json::Value =
            serde_json::from_str(&content).with_context(|| format!("parse {}", path.display()))?;

        let validation_errors: Vec<_> = validator.iter_errors(&instance).collect();
        if validation_errors.is_empty() {
            println!("  OK  {}", path.display());
        } else {
            errors += 1;
            println!("  FAIL  {}", path.display());
            for err in &validation_errors {
                println!("        - {}", err);
            }
        }
    }

    println!(
        "\nValidated {} files, {} errors",
        files_to_validate.len(),
        errors
    );

    if errors > 0 {
        anyhow::bail!("{} fixture(s) failed schema validation", errors);
    }

    // When running default conform (no --file / --fixtures), also check fixture mirror
    if is_default_run {
        check_fixture_mirror()?;
    }

    Ok(())
}

fn collect_json_files(dir: &Path, prefix: Option<&str>) -> anyhow::Result<Vec<PathBuf>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut files: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("read dir {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if !path.extension().map(|e| e == "json").unwrap_or(false) {
            continue;
        }

        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        if let Some(required_prefix) = prefix
            && !name.starts_with(required_prefix)
        {
            continue;
        }

        files.push(path);
    }

    Ok(files)
}

fn cmd_sync_fixtures() -> anyhow::Result<()> {
    let golden_dir = PathBuf::from("crates/perfgate-cli/tests/fixtures/golden");
    let contracts_dir = PathBuf::from("contracts/fixtures");

    sync_fixtures(&golden_dir, &contracts_dir)?;
    Ok(())
}

fn sync_fixtures(golden_dir: &Path, contracts_dir: &Path) -> anyhow::Result<u32> {
    fs::create_dir_all(contracts_dir)
        .with_context(|| format!("create dir {}", contracts_dir.display()))?;

    let mut count = 0u32;
    for entry in
        fs::read_dir(golden_dir).with_context(|| format!("read dir {}", golden_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map(|e| e == "json").unwrap_or(false)
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("sensor_report_"))
                .unwrap_or(false)
        {
            let dest = contracts_dir.join(path.file_name().unwrap());
            fs::copy(&path, &dest)
                .with_context(|| format!("copy {} -> {}", path.display(), dest.display()))?;
            println!("  synced  {}", dest.display());
            count += 1;
        }
    }

    println!("\nSynced {} fixtures from golden -> contracts", count);
    Ok(count)
}

/// Check that golden fixtures and contract fixtures are byte-for-byte identical.
fn check_fixture_mirror() -> anyhow::Result<()> {
    let golden_dir = PathBuf::from("crates/perfgate-cli/tests/fixtures/golden");
    let contracts_dir = PathBuf::from("contracts/fixtures");
    check_fixture_mirror_at(&golden_dir, &contracts_dir)
}

fn check_fixture_mirror_at(golden_dir: &Path, contracts_dir: &Path) -> anyhow::Result<()> {
    if !contracts_dir.is_dir() {
        anyhow::bail!(
            "{} does not exist. Run: cargo run -p xtask -- sync-fixtures",
            contracts_dir.display()
        );
    }

    let mut drift = 0u32;
    for entry in
        fs::read_dir(golden_dir).with_context(|| format!("read dir {}", golden_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map(|e| e == "json").unwrap_or(false)
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("sensor_report_"))
                .unwrap_or(false)
        {
            let contract_path = contracts_dir.join(path.file_name().unwrap());
            if !contract_path.exists() {
                println!(
                    "  DRIFT  {} missing in contracts/fixtures/",
                    path.file_name().unwrap().to_string_lossy()
                );
                drift += 1;
                continue;
            }

            let golden_bytes = fs::read(&path)?;
            let contract_bytes = fs::read(&contract_path)?;
            if golden_bytes != contract_bytes {
                println!(
                    "  DRIFT  {} differs between golden and contracts",
                    path.file_name().unwrap().to_string_lossy()
                );
                drift += 1;
            }
        }
    }

    // Check for extra files in contracts/fixtures/ (contract -> golden)
    for entry in fs::read_dir(contracts_dir)
        .with_context(|| format!("read dir {}", contracts_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map(|e| e == "json").unwrap_or(false)
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("sensor_report_"))
                .unwrap_or(false)
        {
            let golden_path = golden_dir.join(path.file_name().unwrap());
            if !golden_path.exists() {
                println!(
                    "  DRIFT  {} unexpected in contracts/fixtures/ (not in golden)",
                    path.file_name().unwrap().to_string_lossy()
                );
                drift += 1;
            }
        }
    }

    if drift > 0 {
        anyhow::bail!(
            "{} fixture(s) drifted. Run: cargo run -p xtask -- sync-fixtures",
            drift
        );
    }

    println!("  OK  golden and contracts fixtures are in sync");
    Ok(())
}

fn cmd_mutants(
    crate_name: Option<MutantsCrate>,
    summary: bool,
    args: Vec<String>,
) -> anyhow::Result<()> {
    // Typical usage: `cargo install cargo-mutants` then `cargo run -p xtask -- mutants`.
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut cmd = std::process::Command::new(cargo);
    cmd.arg("mutants");

    // Add --package flag if a specific crate is requested
    if let Some(krate) = crate_name {
        cmd.arg("--package").arg(krate.as_package_name());
    }

    // Forward any extra args
    for a in args {
        cmd.arg(a);
    }

    let status = cmd.status().context("running cargo mutants")?;

    // Generate summary report if requested, regardless of exit status
    // cargo-mutants exits 2 for missed mutants, 3 for timeouts - we still want the summary
    if summary {
        generate_mutation_summary(crate_name)?;
    }

    // Propagate cargo-mutants exit code
    if !status.success() {
        let code = status.code().unwrap_or(1);
        std::process::exit(code);
    }

    Ok(())
}

/// Generate a summary report of mutation testing results
fn generate_mutation_summary(crate_name: Option<MutantsCrate>) -> anyhow::Result<()> {
    let outcomes_path = PathBuf::from("mutants.out/outcomes.json");

    if !outcomes_path.exists() {
        println!("\n⚠️  No mutation testing results found at mutants.out/outcomes.json");
        println!("   Run mutation testing first to generate results.");
        return Ok(());
    }

    let outcomes_content =
        fs::read_to_string(&outcomes_path).context("reading mutation outcomes")?;
    let outcomes: serde_json::Value =
        serde_json::from_str(&outcomes_content).context("parsing mutation outcomes")?;

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║              MUTATION TESTING SUMMARY REPORT                 ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    if let Some(krate) = crate_name {
        println!("Crate: {}", krate.as_package_name());
        println!("Target kill rate: {}%\n", krate.target_kill_rate());
    } else {
        println!("Scope: All workspace crates\n");
        println!("Target kill rates by crate:");
        println!("  • perfgate-domain:   100%");
        println!("  • perfgate-types:     95%");
        println!("  • perfgate-app:       90% (includes runtime adapters)");
        println!("  • perfgate-cli:       70%\n");
    }

    // Parse outcomes and count results
    let mut killed = 0u32;
    let mut survived = 0u32;
    let mut timeout = 0u32;
    let mut unviable = 0u32;

    if let Some(outcomes_array) = outcomes.as_array() {
        for outcome in outcomes_array {
            if let Some(summary) = outcome.get("summary").and_then(|s| s.as_str()) {
                // cargo-mutants uses: CaughtMutant, MissedMutant, Timeout, Unviable
                match summary {
                    "CaughtMutant" => killed += 1,
                    "MissedMutant" => survived += 1,
                    "Timeout" => timeout += 1,
                    "Unviable" => unviable += 1,
                    _ => {}
                }
            }
        }
    }

    let total = killed + survived + timeout;
    let kill_rate = if total > 0 {
        (killed as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    println!("┌─────────────────────────────────────────────────────────────┐");
    println!("│ Results                                                     │");
    println!("├─────────────────────────────────────────────────────────────┤");
    println!(
        "│  ✓ Killed:    {:>5}                                        │",
        killed
    );
    println!(
        "│  ✗ Survived:  {:>5}                                        │",
        survived
    );
    println!(
        "│  ⏱ Timeout:   {:>5}                                        │",
        timeout
    );
    println!(
        "│  ⊘ Unviable:  {:>5}                                        │",
        unviable
    );
    println!("├─────────────────────────────────────────────────────────────┤");
    println!(
        "│  Total:       {:>5}                                        │",
        total
    );
    println!(
        "│  Kill Rate:   {:>5.1}%                                       │",
        kill_rate
    );
    println!("└─────────────────────────────────────────────────────────────┘");

    // Check against target if a specific crate was tested
    if let Some(krate) = crate_name {
        let target = krate.target_kill_rate() as f64;
        println!();
        if kill_rate >= target {
            println!(
                "✅ Kill rate meets target ({:.1}% >= {}%)",
                kill_rate, target as u8
            );
        } else {
            println!(
                "❌ Kill rate below target ({:.1}% < {}%)",
                kill_rate, target as u8
            );
            println!("\n   Consider adding tests to kill surviving mutants.");
            println!("   Check mutants.out/caught.txt and mutants.out/missed.txt for details.");
        }
    }

    // List surviving mutants if any
    if survived > 0 {
        let missed_path = PathBuf::from("mutants.out/missed.txt");
        if missed_path.exists() {
            println!("\n┌─────────────────────────────────────────────────────────────┐");
            println!("│ Surviving Mutants (tests needed)                            │");
            println!("└─────────────────────────────────────────────────────────────┘");
            let missed_content = fs::read_to_string(&missed_path).unwrap_or_default();
            for (i, line) in missed_content.lines().take(10).enumerate() {
                println!("  {}. {}", i + 1, line);
            }
            if missed_content.lines().count() > 10 {
                println!(
                    "  ... and {} more (see mutants.out/missed.txt)",
                    missed_content.lines().count() - 10
                );
            }
        }
    }

    println!();
    Ok(())
}

fn run<const N: usize>(bin: &str, args: [&str; N]) -> anyhow::Result<()> {
    let status = std::process::Command::new(bin)
        .args(args)
        .status()
        .with_context(|| format!("running {bin}"))?;
    if !status.success() {
        anyhow::bail!("{bin} failed: {status}");
    }
    Ok(())
}

fn run_command(command: &mut std::process::Command) -> anyhow::Result<()> {
    let status = command
        .status()
        .with_context(|| format!("running {:?}", command))?;
    if !status.success() {
        anyhow::bail!("{:?} failed: {status}", command);
    }
    Ok(())
}

fn run_with_env<const N: usize>(
    bin: &str,
    args: [&str; N],
    envs: &[(&str, &str)],
) -> anyhow::Result<()> {
    if envs.is_empty() {
        return run(bin, args);
    }

    let mut command = std::process::Command::new(bin);
    command.args(args);
    for &(k, v) in envs {
        command.env(k, v);
    }
    let status = command.status().with_context(|| format!("running {bin}"))?;
    if !status.success() {
        anyhow::bail!("{bin} failed: {status}");
    }
    Ok(())
}

fn cmd_schema(out_dir: &PathBuf) -> anyhow::Result<()> {
    fs::create_dir_all(out_dir).with_context(|| format!("create dir {}", out_dir.display()))?;

    write_schema(
        out_dir,
        SCHEMA_FILES[0],
        schema_for!(perfgate_types::RunReceipt),
    )?;

    write_schema(
        out_dir,
        SCHEMA_FILES[1],
        schema_for!(perfgate_types::CompareReceipt),
    )?;

    write_schema(
        out_dir,
        SCHEMA_FILES[2],
        schema_for!(perfgate_types::ProbeReceipt),
    )?;

    write_schema(
        out_dir,
        SCHEMA_FILES[3],
        schema_for!(perfgate_types::ProbeCompareReceipt),
    )?;

    write_schema(
        out_dir,
        SCHEMA_FILES[4],
        schema_for!(perfgate_types::ScenarioReceipt),
    )?;

    write_schema(
        out_dir,
        SCHEMA_FILES[5],
        schema_for!(perfgate_types::TradeoffReceipt),
    )?;

    write_schema(
        out_dir,
        SCHEMA_FILES[6],
        schema_for!(perfgate_types::DecisionArtifactIndex),
    )?;

    write_schema(
        out_dir,
        SCHEMA_FILES[7],
        schema_for!(perfgate_types::baseline_service::DecisionRecord),
    )?;

    write_schema(
        out_dir,
        SCHEMA_FILES[8],
        schema_for!(perfgate_types::DecisionBundleReceipt),
    )?;

    write_schema(
        out_dir,
        SCHEMA_FILES[9],
        schema_for!(perfgate_types::ConfigFile),
    )?;

    write_schema(
        out_dir,
        SCHEMA_FILES[10],
        schema_for!(perfgate_types::PerfgateReport),
    )?;

    write_schema(
        out_dir,
        SCHEMA_FILES[11],
        schema_for!(perfgate_types::AggregateReceipt),
    )?;

    write_schema(
        out_dir,
        SCHEMA_FILES[12],
        schema_for!(perfgate_types::RatchetReceipt),
    )?;

    write_schema(
        out_dir,
        SCHEMA_FILES[13],
        schema_for!(perfgate_types::RepairContextReceipt),
    )?;

    // Sensor report schema is vendored from contracts/, not generated.
    let vendored_schema = PathBuf::from("contracts/schemas/sensor.report.v1.schema.json");
    let dest = out_dir.join(SCHEMA_FILES[14]);
    fs::copy(&vendored_schema, &dest).with_context(|| {
        format!(
            "copy vendored schema {} -> {}",
            vendored_schema.display(),
            dest.display()
        )
    })?;

    Ok(())
}

fn cmd_schema_check(schemas_dir: &Path) -> anyhow::Result<()> {
    if !schemas_dir.exists() {
        anyhow::bail!(
            "{} does not exist. Run: cargo run -p xtask -- schema",
            schemas_dir.display()
        );
    }
    if !schemas_dir.is_dir() {
        anyhow::bail!(
            "{} is not a directory. Run: cargo run -p xtask -- schema",
            schemas_dir.display()
        );
    }

    let generated_dir = xtask::unique_temp_dir("perfgate_schema_check");
    let result = (|| -> anyhow::Result<()> {
        cmd_schema(&generated_dir)?;
        check_schema_mirror_at(&generated_dir, schemas_dir)
    })();

    let _ = fs::remove_dir_all(&generated_dir);
    result
}

fn cmd_schema_compat(fixtures_dir: &Path) -> anyhow::Result<()> {
    if !fixtures_dir.exists() {
        anyhow::bail!(
            "{} does not exist. Add historical fixtures or pass --fixtures-dir.",
            fixtures_dir.display()
        );
    }
    if !fixtures_dir.is_dir() {
        anyhow::bail!("{} is not a directory", fixtures_dir.display());
    }

    let mut files: Vec<PathBuf> = Vec::new();
    collect_schema_compat_json_files(fixtures_dir, &mut files)?;
    files.sort();

    if files.is_empty() {
        anyhow::bail!("no JSON fixtures found under {}", fixtures_dir.display());
    }

    let mut checked = BTreeMap::<String, u32>::new();
    let mut errors = Vec::new();

    for path in &files {
        let raw = match fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(err) => {
                errors.push(format!("{}: {}", path.display(), err));
                continue;
            }
        };

        let value: serde_json::Value = match serde_json::from_str(&raw) {
            Ok(value) => value,
            Err(err) => {
                errors.push(format!("{}: invalid JSON: {}", path.display(), err));
                continue;
            }
        };

        let schema = value
            .get("schema")
            .or_else(|| value.get("report_type"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .or_else(|| infer_schema_from_fixture_path(path));

        let Some(schema) = schema else {
            errors.push(format!(
                "{}: missing schema or report_type field",
                path.display()
            ));
            continue;
        };

        let result = match schema.as_str() {
            "perfgate.run.v1" => {
                serde_json::from_value::<perfgate_types::RunReceipt>(value).map(|_| ())
            }
            "perfgate.compare.v1" => {
                serde_json::from_value::<perfgate_types::CompareReceipt>(value).map(|_| ())
            }
            "perfgate.probe.v1" => {
                serde_json::from_value::<perfgate_types::ProbeReceipt>(value).map(|_| ())
            }
            "perfgate.probe_compare.v1" => {
                serde_json::from_value::<perfgate_types::ProbeCompareReceipt>(value).map(|_| ())
            }
            "perfgate.scenario.v1" => {
                serde_json::from_value::<perfgate_types::ScenarioReceipt>(value).map(|_| ())
            }
            "perfgate.tradeoff.v1" => {
                serde_json::from_value::<perfgate_types::TradeoffReceipt>(value).map(|_| ())
            }
            "perfgate.decision_index.v1" => {
                serde_json::from_value::<perfgate_types::DecisionArtifactIndex>(value).map(|_| ())
            }
            "perfgate.decision_bundle.v1" => {
                serde_json::from_value::<perfgate_types::DecisionBundleReceipt>(value).map(|_| ())
            }
            "perfgate.decision_record.v1" => {
                serde_json::from_value::<perfgate_types::baseline_service::DecisionRecord>(value)
                    .map(|_| ())
            }
            "perfgate.report.v1" => {
                serde_json::from_value::<perfgate_types::PerfgateReport>(value).map(|_| ())
            }
            "sensor.report.v1" => {
                serde_json::from_value::<perfgate_types::SensorReport>(value).map(|_| ())
            }
            "perfgate.baseline.v1" => {
                serde_json::from_value::<perfgate_types::baseline_service::BaselineRecord>(value)
                    .map(|_| ())
            }
            "perfgate.verdict.v1" => {
                serde_json::from_value::<perfgate_types::baseline_service::VerdictRecord>(value)
                    .map(|_| ())
            }
            "perfgate.audit.v1" => {
                serde_json::from_value::<perfgate_types::baseline_service::AuditEvent>(value)
                    .map(|_| ())
            }
            "perfgate.health.v1" => {
                serde_json::from_value::<perfgate_types::baseline_service::HealthResponse>(value)
                    .map(|_| ())
            }
            "perfgate.dependency_event.v1" => {
                serde_json::from_value::<perfgate_types::baseline_service::DependencyEvent>(value)
                    .map(|_| ())
            }
            "perfgate.fleet_alert.v1" => {
                serde_json::from_value::<perfgate_types::baseline_service::FleetAlert>(value)
                    .map(|_| ())
            }
            other => {
                errors.push(format!("{}: unsupported schema {}", path.display(), other));
                continue;
            }
        };

        match result {
            Ok(()) => {
                *checked.entry(schema.clone()).or_default() += 1;
                println!("  OK  {} ({})", path.display(), schema);
            }
            Err(err) => errors.push(format!(
                "{}: failed to deserialize {}: {}",
                path.display(),
                schema,
                err
            )),
        }
    }

    if !errors.is_empty() {
        println!("Found {} schema compatibility error(s):", errors.len());
        for error in &errors {
            println!("  - {error}");
        }
        anyhow::bail!(
            "{} historical schema fixture(s) failed compatibility checks",
            errors.len()
        );
    }

    println!(
        "  OK  {} historical schema fixture(s) deserialize with current types",
        checked.values().copied().sum::<u32>()
    );
    for (schema, count) in checked {
        println!("      {schema}: {count}");
    }

    Ok(())
}

fn collect_schema_compat_json_files(dir: &Path, out: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("read dir {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_schema_compat_json_files(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "json") {
            out.push(path);
        }
    }
    Ok(())
}

fn infer_schema_from_fixture_path(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    match name {
        "perfgate.audit.v1.json" => Some("perfgate.audit.v1".to_string()),
        "perfgate.health.v1.json" => Some("perfgate.health.v1".to_string()),
        _ => None,
    }
}

fn check_schema_mirror_at(generated_dir: &Path, committed_dir: &Path) -> anyhow::Result<()> {
    let mut drift = 0u32;

    for name in SCHEMA_FILES {
        let generated_path = generated_dir.join(name);
        let committed_path = committed_dir.join(name);

        if !committed_path.exists() {
            println!("  DRIFT  {} missing in {}", name, committed_dir.display());
            drift += 1;
            continue;
        }

        let generated_str = fs::read_to_string(&generated_path)
            .with_context(|| format!("read {}", generated_path.display()))?
            .replace("\r\n", "\n");
        let committed_str = fs::read_to_string(&committed_path)
            .with_context(|| format!("read {}", committed_path.display()))?
            .replace("\r\n", "\n");

        if generated_str != committed_str {
            println!("  DRIFT  {} differs from generated schema", name);
            drift += 1;
        }
    }

    for entry in fs::read_dir(committed_dir)
        .with_context(|| format!("read dir {}", committed_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.extension().map(|e| e == "json").unwrap_or(false) {
            continue;
        }

        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        if !SCHEMA_FILES.contains(&name) {
            println!(
                "  DRIFT  {} unexpected in {}",
                name,
                committed_dir.display()
            );
            drift += 1;
        }
    }

    if drift > 0 {
        anyhow::bail!(
            "{} schema file(s) drifted. Run: cargo run -p xtask -- schema",
            drift
        );
    }

    println!("  OK  schema files are locked and up to date");
    Ok(())
}

fn write_schema<T: serde::Serialize>(
    out_dir: &std::path::Path,
    name: &str,
    schema: T,
) -> anyhow::Result<()> {
    let path = out_dir.join(name);
    let json = serde_json::to_vec_pretty(&schema)?;
    fs::write(&path, json).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// List all microcrates and their purposes.
fn cmd_microcrates() -> anyhow::Result<()> {
    println!("Perfgate Microcrates");
    println!("===================\n");

    let microcrates = [(
        "perfgate-fake",
        "Test utilities and fake implementations",
        70,
    )];

    println!("{:<25} {:<55} {:>10}", "Crate", "Description", "Kill Rate");
    println!("{:-<25} {:-<55} {:->10}", "", "", "");

    for (name, desc, rate) in &microcrates {
        println!("{:<25} {:<55} {:>9}%", name, desc, rate);
    }

    println!("\nCore Crates");
    println!("-----------\n");

    let core_crates = [
        (
            "perfgate-types",
            "Receipt/config structs, JSON schema types",
            95,
        ),
        (
            "perfgate",
            "Facade with domain, app, runtime, and presentation modules",
            90,
        ),
        (
            "perfgate-cli",
            "CLI argument parsing and command dispatch",
            70,
        ),
    ];

    println!("{:<25} {:<55} {:>10}", "Crate", "Description", "Kill Rate");
    println!("{:-<25} {:-<55} {:->10}", "", "", "");

    for (name, desc, rate) in &core_crates {
        println!("{:<25} {:<55} {:>9}%", name, desc, rate);
    }

    println!("\nDependency Flow");
    println!("--------------\n");
    println!("  perfgate-types::error (unified errors)");
    println!("         ↓");
    println!("  perfgate-types::fingerprint (deterministic hashes)");
    println!("         ↓");
    println!("  perfgate-types::validation, perfgate::domain::host (pure logic)");
    println!("         ↓");
    println!("  perfgate-types (data contracts)");
    println!("         ↓");
    println!(
        "  perfgate::domain::budget, perfgate::domain::significance, perfgate::domain::scaling"
    );
    println!("         ↓");
    println!("  perfgate::presentation::{{render, export, sensor}}, perfgate::domain::paired");
    println!("         ↓");
    println!("  perfgate::domain (policy)");
    println!("         ↓");
    println!("  perfgate::runtime (platform I/O)");
    println!("         ↓");
    println!("  perfgate::app (use cases)");
    println!("         ↓");
    println!("  perfgate-cli (entry point)");

    Ok(())
}

fn cmd_dogfood(action: DogfoodAction) -> anyhow::Result<()> {
    match action {
        DogfoodAction::Fixtures => {
            println!("Regenerating dogfooding fixtures...");
            // Ensure selfbench is built
            run("cargo", ["build", "--release", "-p", "perfgate-selfbench"])?;

            let selfbench_bin = if cfg!(windows) {
                "./target/release/perfgate-selfbench.exe"
            } else {
                "./target/release/perfgate-selfbench"
            };

            run_with_env(
                "cargo",
                [
                    "run",
                    "--release",
                    "-p",
                    "perfgate-cli",
                    "--bin",
                    "perfgate",
                    "--",
                    "run",
                    "--name",
                    "test-bench",
                    "--repeat",
                    "5",
                    "--warmup",
                    "1",
                    "--out",
                    ".ci/fixtures/compare/small-baseline.json",
                    "--",
                    selfbench_bin,
                    "noop",
                ],
                &[],
            )?;
            run_with_env(
                "cargo",
                [
                    "run",
                    "--release",
                    "-p",
                    "perfgate-cli",
                    "--bin",
                    "perfgate",
                    "--",
                    "run",
                    "--name",
                    "test-bench",
                    "--repeat",
                    "5",
                    "--warmup",
                    "1",
                    "--out",
                    ".ci/fixtures/compare/small-current.json",
                    "--",
                    selfbench_bin,
                    "noop",
                ],
                &[],
            )?;
            run_with_env(
                "cargo",
                [
                    "run",
                    "--release",
                    "-p",
                    "perfgate-cli",
                    "--bin",
                    "perfgate",
                    "--",
                    "compare",
                    "--baseline",
                    ".ci/fixtures/compare/small-baseline.json",
                    "--current",
                    ".ci/fixtures/compare/small-current.json",
                    "--out",
                    ".ci/fixtures/compare/compare-receipt.json",
                ],
                &[],
            )
            .ok(); // Ignore exit code 2 (policy fail) here
            println!("Fixtures regenerated successfully.");
            Ok(())
        }
        DogfoodAction::Verify { dir } => {
            println!("Verifying dogfooding artifacts in {}...", dir.display());
            let required_files = ["report.json", "comment.md"];
            for file in &required_files {
                let path = dir.join(file);
                if !path.exists() {
                    anyhow::bail!("Missing required artifact: {}", path.display());
                }
                println!("  OK  {}", path.display());
            }

            // Also check extras
            let pattern = format!("{}/extras/**/perfgate.run.v1.json", dir.display());
            let mut count = 0;
            for entry in glob(&pattern)? {
                let path = entry?;
                validate_dogfood_run_receipt(&path)?;
                println!("  OK  {}", path.display());
                count += 1;
            }
            if count == 0 {
                anyhow::bail!("No native receipts found matching {}", pattern);
            }
            println!("Verified {} native receipts.", count);
            Ok(())
        }
        DogfoodAction::Promote => {
            println!("Promoting nightly outputs to baselines...");
            let target_root = Path::new("baselines/gha-ubuntu-24.04-x86_64/");
            fs::create_dir_all(target_root)?;

            let pattern = "artifacts/perfgate/extras/**/perfgate.run.v1.json";
            let mut count = 0;
            for entry in glob(pattern)? {
                let src = entry?;
                validate_dogfood_run_receipt(&src)?;
                let rel = src
                    .strip_prefix("artifacts/perfgate/extras/")
                    .context("invalid path")?;
                let bench_path = rel.parent().context("no bench parent")?;
                let bench_name = bench_path.to_str().context("non-utf8 bench name")?;

                let dest = target_root.join(format!("{}.json", bench_name));
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)?;
                }

                println!("  Promoting {} -> {}", bench_name, dest.display());

                run_with_env(
                    "cargo",
                    [
                        "run",
                        "--release",
                        "-p",
                        "perfgate-cli",
                        "--bin",
                        "perfgate",
                        "--",
                        "promote",
                        "--current",
                        src.to_str().context("src path")?,
                        "--to",
                        dest.to_str().context("dest path")?,
                        "--normalize",
                    ],
                    &[],
                )?;
                count += 1;
            }
            if count == 0 {
                anyhow::bail!("No nightly run receipts found matching {}", pattern);
            }
            println!("Promoted {} baselines.", count);
            Ok(())
        }
        DogfoodAction::ExportTrends {
            artifacts_dir,
            out_dir,
        } => export_dogfood_trends(&artifacts_dir, &out_dir),
        DogfoodAction::Summarize { dir } => {
            println!("Generating trend variance summary...");
            let pattern = format!("{}/**/*.jsonl", dir.display());

            let mut all_rows: Vec<perfgate::app::export::RunExportRow> = Vec::new();
            for entry in glob(&pattern)? {
                let path = entry?;
                let content = fs::read_to_string(&path)?;
                for line in content.lines() {
                    if let Ok(row) =
                        serde_json::from_str::<perfgate::app::export::RunExportRow>(line)
                    {
                        all_rows.push(row);
                    }
                }
            }

            if all_rows.is_empty() {
                println!("No trend data found in {}", dir.display());
                return Ok(());
            }

            // Group by bench name
            let mut by_bench: std::collections::BTreeMap<String, Vec<f64>> =
                std::collections::BTreeMap::new();
            for row in all_rows {
                by_bench
                    .entry(row.bench_name)
                    .or_default()
                    .push(row.wall_ms_median as f64);
            }

            println!("\n## Weekly Variance Summary");
            println!("\n| Benchmark | Samples | Mean (ms) | StdDev | CV (%) | Rec. Threshold |");
            println!("|-----------|---------|-----------|--------|--------|----------------|");

            for (bench, mut vals) in by_bench {
                vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let count = vals.len();
                if count < 2 {
                    let mean = vals.first().unwrap_or(&0.0);
                    println!("| {} | {} | {:.2} | N/A | N/A | N/A |", bench, count, mean);
                    continue;
                }

                let (mean, variance) =
                    perfgate::domain::stats::mean_and_variance(&vals).unwrap_or((0.0, 0.0));
                let stddev = variance.sqrt();
                let cv = if mean > 0.0 {
                    (stddev / mean) * 100.0
                } else {
                    0.0
                };

                // Recommended Threshold: usually 3x CV + small buffer
                let rec_thresh = (cv * 3.0).max(5.0); // minimum 5% threshold

                println!(
                    "| {} | {} | {:.2} | {:.2} | {:.2}% | {:.1}% |",
                    bench, count, mean, stddev, cv, rec_thresh
                );
            }

            Ok(())
        }
    }
}

fn export_dogfood_trends(artifacts_dir: &Path, out_dir: &Path) -> anyhow::Result<()> {
    println!(
        "Exporting dogfooding trends from {} to {}...",
        artifacts_dir.display(),
        out_dir.display()
    );
    fs::create_dir_all(out_dir)?;

    let extras_dir = artifacts_dir.join("extras");

    let run_pattern = dogfood_receipt_pattern(&extras_dir, "perfgate.run.v1.json");
    let run_receipts: Vec<_> = glob(&run_pattern)?.collect::<Result<Vec<_>, _>>()?;
    let compare_pattern = dogfood_receipt_pattern(&extras_dir, "perfgate.compare.v1.json");
    let compare_receipts: Vec<_> = glob(&compare_pattern)?.collect::<Result<Vec<_>, _>>()?;

    if run_receipts.is_empty() && compare_receipts.is_empty() {
        anyhow::bail!(
            "no dogfooding receipts found under {}",
            extras_dir.display()
        );
    }

    let perfgate = release_perfgate_bin()?;

    let mut run_count = 0;
    for receipt in run_receipts {
        let bench = dogfood_bench_slug(&extras_dir, &receipt, "perfgate.run.v1.json")?;
        let out = out_dir.join(format!("history-{bench}.jsonl"));
        println!(
            "  Exporting run trend {} -> {}",
            receipt.display(),
            out.display()
        );
        run_command(
            std::process::Command::new(&perfgate)
                .arg("export")
                .arg("--run")
                .arg(&receipt)
                .arg("--format")
                .arg("jsonl")
                .arg("--out")
                .arg(&out),
        )?;
        run_count += 1;
    }

    let mut compare_count = 0;
    for receipt in compare_receipts {
        let bench = dogfood_bench_slug(&extras_dir, &receipt, "perfgate.compare.v1.json")?;
        let out = out_dir.join(format!("metrics-{bench}.prom"));
        println!(
            "  Exporting compare trend {} -> {}",
            receipt.display(),
            out.display()
        );
        run_command(
            std::process::Command::new(&perfgate)
                .arg("export")
                .arg("--compare")
                .arg(&receipt)
                .arg("--format")
                .arg("prometheus")
                .arg("--out")
                .arg(&out),
        )?;
        compare_count += 1;
    }

    println!("Exported {run_count} run trend file(s) and {compare_count} compare metric file(s).");
    Ok(())
}

fn dogfood_receipt_pattern(extras_dir: &Path, file_name: &str) -> String {
    let root = extras_dir.display().to_string().replace('\\', "/");
    format!("{}/**/{}", root.trim_end_matches('/'), file_name)
}

fn dogfood_bench_slug(
    extras_dir: &Path,
    receipt: &Path,
    file_name: &str,
) -> anyhow::Result<String> {
    let rel = receipt.strip_prefix(extras_dir).with_context(|| {
        format!(
            "{} is not under {}",
            receipt.display(),
            extras_dir.display()
        )
    })?;
    let bench_path = rel
        .parent()
        .with_context(|| format!("{} has no benchmark parent", receipt.display()))?;

    if receipt.file_name().and_then(|name| name.to_str()) != Some(file_name) {
        anyhow::bail!("unexpected receipt path: {}", receipt.display());
    }

    let slug = bench_path
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("-");

    if slug.is_empty() {
        anyhow::bail!("{} does not identify a benchmark", receipt.display());
    }

    Ok(slug)
}

fn release_perfgate_bin() -> anyhow::Result<PathBuf> {
    let binary_name = if cfg!(windows) {
        "perfgate.exe"
    } else {
        "perfgate"
    };
    let path = Path::new("target").join("release").join(binary_name);
    if path.is_file() {
        Ok(path)
    } else {
        anyhow::bail!(
            "perfgate release binary not found at {}; build it with `cargo build --release -p perfgate-cli --bin perfgate`",
            path.display()
        );
    }
}

fn validate_dogfood_run_receipt(path: &Path) -> anyhow::Result<()> {
    let content =
        fs::read_to_string(path).with_context(|| format!("read run receipt {}", path.display()))?;
    let receipt: perfgate_types::RunReceipt = serde_json::from_str(&content)
        .with_context(|| format!("deserialize run receipt {}", path.display()))?;

    if receipt.samples.is_empty() {
        anyhow::bail!("dogfood run receipt {} has no samples", path.display());
    }

    let failed: Vec<_> = receipt
        .samples
        .iter()
        .enumerate()
        .filter(|(_, sample)| sample.exit_code != 0 || sample.timed_out)
        .collect();

    if failed.is_empty() {
        return Ok(());
    }

    let preview = failed
        .iter()
        .take(3)
        .map(|(idx, sample)| {
            let first_stderr_line = sample
                .stderr
                .as_deref()
                .and_then(|stderr| stderr.lines().find(|line| !line.trim().is_empty()))
                .unwrap_or("")
                .trim();
            if first_stderr_line.is_empty() {
                format!(
                    "#{} exit_code={} timed_out={}",
                    idx + 1,
                    sample.exit_code,
                    sample.timed_out
                )
            } else {
                format!(
                    "#{} exit_code={} timed_out={} stderr={}",
                    idx + 1,
                    sample.exit_code,
                    sample.timed_out,
                    first_stderr_line
                )
            }
        })
        .collect::<Vec<_>>()
        .join("; ");

    anyhow::bail!(
        "dogfood run receipt {} contains {} failed or timed-out sample(s): {}",
        path.display(),
        failed.len(),
        preview
    );
}

fn cmd_docs_sync() -> anyhow::Result<()> {
    let md = generate_workspace_inventory_md();
    let path = Path::new("docs/WORKSPACE.md");
    fs::write(path, md).with_context(|| format!("write {}", path.display()))?;
    println!("  OK  {}", path.display());

    Ok(())
}

fn generate_workspace_inventory_md() -> String {
    let mut md = String::new();
    md.push_str("# Perfgate Workspace Inventory\n\n");
    md.push_str("This file is automatically generated by `cargo run -p xtask -- docs-sync`.\n\n");

    md.push_str("## Micro-crates\n\n");
    md.push_str("| Crate | Description | Kill Rate Target |\n");
    md.push_str("|-------|-------------|------------------|\n");

    let microcrates = [(
        "perfgate-fake",
        "Test utilities and fake implementations",
        70,
    )];

    for (name, desc, rate) in &microcrates {
        md.push_str(&format!("| `{}` | {} | {}% |\n", name, desc, rate));
    }

    md.push_str("\n## Core Crates\n\n");
    md.push_str("| Crate | Description | Kill Rate Target |\n");
    md.push_str("|-------|-------------|------------------|\n");

    let core_crates = [
        (
            "perfgate-types",
            "Receipt/config structs, JSON schema types",
            95,
        ),
        (
            "perfgate-cli",
            "CLI argument parsing and command dispatch",
            70,
        ),
        (
            "perfgate-server",
            "REST API server for baseline management",
            90,
        ),
        (
            "perfgate-client",
            "API client for baseline server interaction",
            90,
        ),
        (
            "perfgate",
            "Facade with domain, app, runtime, and presentation modules",
            90,
        ),
    ];

    for (name, desc, rate) in &core_crates {
        md.push_str(&format!("| `{}` | {} | {}% |\n", name, desc, rate));
    }

    md.push_str("\n## Dependency Flow\n\n");
    md.push_str("```mermaid\ngraph TD\n");
    md.push_str("  types --> fingerprint[perfgate-types::fingerprint]\n");
    md.push_str("  facade[perfgate] --> domain[perfgate::domain]\n");
    md.push_str("  domain --> stats[perfgate::domain::stats]\n");
    md.push_str("  types --> val[perfgate-types::validation]\n");
    md.push_str("  domain --> host[perfgate::domain::host]\n");
    md.push_str("  domain --> budget[perfgate::domain::budget]\n");
    md.push_str("  domain --> sig[perfgate::domain::significance]\n");
    md.push_str("  domain --> scaling[perfgate::domain::scaling]\n");
    md.push_str("  facade --> runtime[perfgate::runtime]\n");
    md.push_str("  runtime --> app[perfgate::app]\n");
    md.push_str("  app --> cli[perfgate-cli]\n");
    md.push_str("  types --> client[perfgate-client]\n");
    md.push_str("  types --> server[perfgate-server]\n");
    md.push_str("```\n");
    md
}

fn cmd_docs_check() -> anyhow::Result<()> {
    println!("Checking documentation drift...");
    let path = Path::new("docs/WORKSPACE.md");
    if !path.exists() {
        anyhow::bail!(
            "Missing documentation: {}. Run: cargo run -p xtask -- docs-sync",
            path.display()
        );
    }

    let committed = fs::read_to_string(path)?;
    let generated = generate_workspace_inventory_md();

    if committed.replace("\r\n", "\n") != generated.replace("\r\n", "\n") {
        anyhow::bail!(
            "Documentation drift detected in {}. Run: cargo run -p xtask -- docs-sync",
            path.display()
        );
    }

    println!("  OK  documentation is up to date");
    Ok(())
}

fn cmd_docs_source_check(root: &Path) -> anyhow::Result<()> {
    let errors = collect_docs_source_errors(root)?;

    if !errors.is_empty() {
        println!(
            "Found {} source-of-truth documentation error(s):",
            errors.len()
        );
        for error in &errors {
            println!("  - {}", error);
        }

        anyhow::bail!(
            "{} source-of-truth documentation issue(s) found. Fix metadata, links, or .codex/goals/active.toml.",
            errors.len()
        );
    }

    println!("  OK  source-of-truth docs metadata, IDs, links, and active goal are valid");
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum SourceDocKind {
    Proposal,
    Spec,
    Adr,
    Plan,
}

impl SourceDocKind {
    const fn label(self) -> &'static str {
        match self {
            SourceDocKind::Proposal => "proposal",
            SourceDocKind::Spec => "spec",
            SourceDocKind::Adr => "ADR",
            SourceDocKind::Plan => "plan",
        }
    }

    const fn required_headers(self) -> &'static [&'static str] {
        match self {
            SourceDocKind::Proposal => &[
                "Status",
                "Owner",
                "Created",
                "Target milestone",
                "Linked specs",
                "Linked ADRs",
                "Linked plan",
                "Support/status impact",
                "Policy impact",
            ],
            SourceDocKind::Spec => &[
                "Status",
                "Owner",
                "Created",
                "Milestone",
                "Behavior version",
                "Product surface",
                "CI surface",
                "Schema impact",
                "Action impact",
                "Server impact",
                "Linked proposal",
                "Linked ADRs",
                "Linked plan",
                "Linked policy",
                "Support/status impact",
                "Proof commands",
            ],
            SourceDocKind::Adr => &["Status", "Date", "Owner", "Linked proposal", "Linked specs"],
            SourceDocKind::Plan => &[
                "Status",
                "Owner",
                "Created",
                "Milestone",
                "Current PR",
                "Linked proposal",
                "Linked specs",
                "Proof commands",
                "Rollback",
            ],
        }
    }
}

#[derive(Debug)]
struct SourceDoc {
    kind: SourceDocKind,
    path: PathBuf,
    metadata: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RailsIndex {
    #[serde(default)]
    schema_version: String,
    project: RailsProject,
    #[serde(default)]
    conventions: RailsConventions,
    #[serde(default)]
    external_namespaces: RailsExternalNamespaces,
    #[serde(default)]
    artifact: Vec<RailsArtifact>,
    #[serde(default)]
    lane: Vec<RailsLane>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RailsProject {
    repo: String,
    framework: String,
    root: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RailsConventions {
    proposal_prefix: String,
    spec_prefix: String,
    adr_prefix: String,
    lane_prefix: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RailsExternalNamespaces {
    codex: String,
    speckit: String,
    claude: String,
    jules: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RailsArtifact {
    id: String,
    kind: String,
    path: String,
    status: String,
    owner: String,
    #[serde(default)]
    linked_proposal: Option<String>,
    #[serde(default)]
    linked_specs: Vec<String>,
    #[serde(default)]
    linked_adrs: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RailsLane {
    id: String,
    name: String,
    path: String,
    status: String,
    owner: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RailsLaneTracker {
    schema_version: String,
    id: String,
    name: String,
    status: String,
    owner: String,
    #[serde(default)]
    objective: String,
    #[serde(default)]
    end_state: Vec<String>,
    #[serde(default)]
    work_item: Vec<RailsLaneWorkItem>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RailsLaneWorkItem {
    id: String,
    status: String,
    #[serde(default)]
    proposal: String,
    #[serde(default)]
    spec: String,
    #[serde(default)]
    adr: String,
    #[serde(default)]
    implementation_plan: String,
    #[serde(default)]
    blocks: Vec<String>,
    #[serde(default)]
    blocked_by: Vec<String>,
    #[serde(default)]
    proof: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RailsSupportMap {
    schema_version: String,
    #[serde(default)]
    claim: Vec<RailsSupportClaim>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RailsSupportClaim {
    id: String,
    statement: String,
    #[serde(default)]
    proof: Vec<String>,
    #[serde(default)]
    references: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RailsPolicyReference {
    schema_version: String,
    #[serde(default)]
    ledger: Vec<RailsPolicyLedger>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RailsPolicyLedger {
    id: String,
    path: String,
    owner: String,
}

fn cmd_rails_check(root: &Path) -> anyhow::Result<()> {
    let errors = collect_rails_errors(root)?;

    if !errors.is_empty() {
        println!("Found {} Rails framework error(s):", errors.len());
        for error in &errors {
            println!("  - {}", error);
        }

        anyhow::bail!(
            "{} Rails framework issue(s) found. Fix .rails/index.toml or registered artifacts.",
            errors.len()
        );
    }

    println!("  OK  Rails index, artifact paths, statuses, and links are valid");
    Ok(())
}

fn collect_rails_errors(root: &Path) -> anyhow::Result<Vec<String>> {
    let mut errors = Vec::new();

    let legacy_root = root.join(".perfgate-spec");
    if legacy_root.exists() {
        errors.push(format!(
            "legacy source-of-truth namespace {} must not exist; use .rails/",
            relative_display(root, &legacy_root)
        ));
    }

    for required_doc in ["docs/rails.md", "docs/contributing/rails.md"] {
        let path = root.join(required_doc);
        if !path.exists() {
            errors.push(format!("missing Rails human guidance `{required_doc}`"));
        }
    }

    let rails_root = root.join(".rails");
    let index_path = rails_root.join("index.toml");
    if !index_path.exists() {
        errors.push("missing Rails registry `.rails/index.toml`".to_string());
        return Ok(errors);
    }

    let raw = fs::read_to_string(&index_path)
        .with_context(|| format!("reading {}", index_path.display()))?;
    let index = match toml::from_str::<RailsIndex>(&raw) {
        Ok(index) => index,
        Err(err) => {
            errors.push(format!(
                "{} must parse as TOML: {err}",
                relative_display(root, &index_path)
            ));
            return Ok(errors);
        }
    };

    validate_rails_index_contract(&index, &mut errors);

    let mut artifact_ids = BTreeMap::<String, String>::new();
    let mut artifact_paths = BTreeMap::<String, String>::new();
    let mut artifact_kinds = BTreeMap::<String, String>::new();
    for artifact in &index.artifact {
        if artifact.owner.trim().is_empty() {
            errors.push(format!("artifact {} has an empty owner", artifact.id));
        }
        validate_rails_artifact_kind(&artifact.kind, &artifact.id, &mut errors);
        validate_rails_status(
            "artifact",
            &artifact.id,
            &artifact.status,
            &[
                "proposed",
                "accepted",
                "implemented",
                "superseded",
                "deprecated",
            ],
            &mut errors,
        );
        validate_rails_registered_path(root, "artifact", &artifact.id, &artifact.path, &mut errors);
        validate_rails_artifact_kind_path(artifact, &mut errors);
        validate_rails_artifact_path_identity(artifact, &mut errors);

        if let Some(previous_path) = artifact_ids.insert(artifact.id.clone(), artifact.path.clone())
        {
            errors.push(format!(
                "duplicate Rails artifact id {} in `{}` and `{}`",
                artifact.id, previous_path, artifact.path
            ));
        }
        artifact_kinds.insert(artifact.id.clone(), artifact.kind.clone());
        if let Some(previous_id) = artifact_paths.insert(artifact.path.clone(), artifact.id.clone())
        {
            errors.push(format!(
                "duplicate Rails artifact path `{}` registered by {} and {}",
                artifact.path, previous_id, artifact.id
            ));
        }
    }

    for artifact in &index.artifact {
        if let Some(linked_proposal) = artifact.linked_proposal.as_deref() {
            validate_rails_link(
                &artifact.id,
                "linked_proposal",
                linked_proposal,
                "proposal",
                &artifact_kinds,
                &mut errors,
            );
        }
        for linked_spec in &artifact.linked_specs {
            validate_rails_link(
                &artifact.id,
                "linked_specs",
                linked_spec,
                "spec",
                &artifact_kinds,
                &mut errors,
            );
        }
        for linked_adr in &artifact.linked_adrs {
            validate_rails_link(
                &artifact.id,
                "linked_adrs",
                linked_adr,
                "adr",
                &artifact_kinds,
                &mut errors,
            );
        }
    }

    validate_rails_owned_artifacts_registered(root, &artifact_paths, &mut errors);
    validate_rails_support_and_policy(root, &index.artifact, &mut errors);

    let implemented_closeout_paths = index
        .artifact
        .iter()
        .filter(|artifact| artifact.kind == "closeout" && artifact.status == "implemented")
        .map(|artifact| artifact.path.as_str())
        .collect::<Vec<_>>();

    let mut lane_ids = BTreeSet::<String>::new();
    for lane in &index.lane {
        if lane.name.trim().is_empty() {
            errors.push(format!("lane {} has an empty name", lane.id));
        }
        if lane.owner.trim().is_empty() {
            errors.push(format!("lane {} has an empty owner", lane.id));
        }
        validate_rails_status(
            "lane",
            &lane.id,
            &lane.status,
            &[
                "planned",
                "active",
                "blocked",
                "implemented",
                "closed",
                "superseded",
            ],
            &mut errors,
        );
        validate_rails_registered_path(root, "lane", &lane.id, &lane.path, &mut errors);
        validate_rails_lane_path(lane, &mut errors);
        validate_rails_lane_tracker(root, lane, &artifact_kinds, &mut errors);
        if lane.status == "implemented"
            && !implemented_closeout_paths
                .iter()
                .any(|path| rails_path_file_name(path).contains(&lane.id))
        {
            errors.push(format!(
                "implemented Rails lane {} must have a registered implemented closeout artifact whose filename contains `{}`",
                lane.id, lane.id
            ));
        }
        if !lane_ids.insert(lane.id.clone()) {
            errors.push(format!("duplicate Rails lane id {}", lane.id));
        }
    }

    Ok(errors)
}

fn validate_rails_index_contract(index: &RailsIndex, errors: &mut Vec<String>) {
    if index.schema_version != "1.0" {
        errors.push(format!(
            ".rails/index.toml schema_version must be `1.0`, got `{}`",
            index.schema_version
        ));
    }
    if index.project.repo != "perfgate" {
        errors.push(format!(
            ".rails/index.toml project.repo must be `perfgate`, got `{}`",
            index.project.repo
        ));
    }
    if index.project.framework != "rails" {
        errors.push(format!(
            ".rails/index.toml project.framework must be `rails`, got `{}`",
            index.project.framework
        ));
    }
    if index.project.root != ".rails" {
        errors.push(format!(
            ".rails/index.toml project.root must be `.rails`, got `{}`",
            index.project.root
        ));
    }

    validate_rails_index_value(
        "conventions.proposal_prefix",
        &index.conventions.proposal_prefix,
        "PERFGATE-PROP",
        errors,
    );
    validate_rails_index_value(
        "conventions.spec_prefix",
        &index.conventions.spec_prefix,
        "PERFGATE-SPEC",
        errors,
    );
    validate_rails_index_value(
        "conventions.adr_prefix",
        &index.conventions.adr_prefix,
        "PERFGATE-ADR",
        errors,
    );
    validate_rails_index_value(
        "conventions.lane_prefix",
        &index.conventions.lane_prefix,
        "PERFGATE-LANE",
        errors,
    );
    validate_rails_index_value(
        "external_namespaces.codex",
        &index.external_namespaces.codex,
        ".codex",
        errors,
    );
    validate_rails_index_value(
        "external_namespaces.speckit",
        &index.external_namespaces.speckit,
        ".spec",
        errors,
    );
    validate_rails_index_value(
        "external_namespaces.claude",
        &index.external_namespaces.claude,
        ".claude",
        errors,
    );
    validate_rails_index_value(
        "external_namespaces.jules",
        &index.external_namespaces.jules,
        ".jules",
        errors,
    );
}

fn validate_rails_index_value(field: &str, actual: &str, expected: &str, errors: &mut Vec<String>) {
    if actual != expected {
        errors.push(format!(
            ".rails/index.toml {field} must be `{expected}`, got `{actual}`"
        ));
    }
}

fn validate_rails_support_and_policy(
    root: &Path,
    artifacts: &[RailsArtifact],
    errors: &mut Vec<String>,
) {
    for artifact in artifacts {
        match artifact.kind.as_str() {
            "support" => validate_rails_support_artifact(root, artifact, errors),
            "policy" => validate_rails_policy_artifact(root, artifact, errors),
            _ => {}
        }
    }
}

fn validate_rails_support_artifact(
    root: &Path,
    artifact: &RailsArtifact,
    errors: &mut Vec<String>,
) {
    let path = root.join(&artifact.path);
    if !path.exists() {
        return;
    }

    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(err) => {
            errors.push(format!(
                "Rails support artifact {} `{}` could not be read: {err}",
                artifact.id, artifact.path
            ));
            return;
        }
    };
    let support = match toml::from_str::<RailsSupportMap>(&raw) {
        Ok(support) => support,
        Err(err) => {
            errors.push(format!(
                "Rails support artifact {} `{}` must parse as TOML: {err}",
                artifact.id, artifact.path
            ));
            return;
        }
    };

    if support.schema_version != "1.0" {
        errors.push(format!(
            "Rails support artifact {} uses schema_version `{}`; expected `1.0`",
            artifact.id, support.schema_version
        ));
    }
    if support.claim.is_empty() {
        errors.push(format!(
            "Rails support artifact {} must define at least one claim",
            artifact.id
        ));
    }
    let mut claim_ids = BTreeSet::<&str>::new();
    for claim in &support.claim {
        if claim.id.trim().is_empty() {
            errors.push(format!(
                "Rails support artifact {} has a claim with an empty id",
                artifact.id
            ));
        } else {
            if !claim.id.starts_with("PERFGATE-CLAIM-") {
                errors.push(format!(
                    "Rails support claim {} must use id prefix `PERFGATE-CLAIM-`",
                    claim.id
                ));
            }
            if !claim_ids.insert(claim.id.as_str()) {
                errors.push(format!(
                    "Rails support artifact {} has duplicate claim id {}",
                    artifact.id, claim.id
                ));
            }
        }
        if claim.statement.trim().is_empty() {
            errors.push(format!(
                "Rails support claim {} has an empty statement",
                claim.id
            ));
        }
        if claim.proof.is_empty() {
            errors.push(format!(
                "Rails support claim {} must list proof commands",
                claim.id
            ));
        }
        for proof in &claim.proof {
            if proof.trim().is_empty() {
                errors.push(format!(
                    "Rails support claim {} has an empty proof command",
                    claim.id
                ));
            }
        }
        if claim.references.is_empty() {
            errors.push(format!(
                "Rails support claim {} must list reference paths",
                claim.id
            ));
        }
        for reference in &claim.references {
            validate_rails_reference_path(
                root,
                &format!("Rails support claim {}", claim.id),
                "reference",
                reference,
                errors,
            );
        }
    }
}

fn validate_rails_policy_artifact(root: &Path, artifact: &RailsArtifact, errors: &mut Vec<String>) {
    let path = root.join(&artifact.path);
    if !path.exists() {
        return;
    }

    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(err) => {
            errors.push(format!(
                "Rails policy artifact {} `{}` could not be read: {err}",
                artifact.id, artifact.path
            ));
            return;
        }
    };
    let policy = match toml::from_str::<RailsPolicyReference>(&raw) {
        Ok(policy) => policy,
        Err(err) => {
            errors.push(format!(
                "Rails policy artifact {} `{}` must parse as TOML: {err}",
                artifact.id, artifact.path
            ));
            return;
        }
    };

    if policy.schema_version != "1.0" {
        errors.push(format!(
            "Rails policy artifact {} uses schema_version `{}`; expected `1.0`",
            artifact.id, policy.schema_version
        ));
    }
    if policy.ledger.is_empty() {
        errors.push(format!(
            "Rails policy artifact {} must define at least one ledger",
            artifact.id
        ));
    }
    let mut ledger_ids = BTreeSet::<&str>::new();
    for ledger in &policy.ledger {
        if ledger.id.trim().is_empty() {
            errors.push(format!(
                "Rails policy artifact {} has a ledger with an empty id",
                artifact.id
            ));
        } else if !ledger_ids.insert(ledger.id.as_str()) {
            errors.push(format!(
                "Rails policy artifact {} has duplicate ledger id {}",
                artifact.id, ledger.id
            ));
        }
        if ledger.owner.trim().is_empty() {
            errors.push(format!(
                "Rails policy ledger {} has an empty owner",
                ledger.id
            ));
        }
        validate_rails_reference_path(
            root,
            &format!("Rails policy ledger {}", ledger.id),
            "path",
            &ledger.path,
            errors,
        );
    }
}

fn validate_rails_reference_path(
    root: &Path,
    source: &str,
    field: &str,
    raw_path: &str,
    errors: &mut Vec<String>,
) {
    if raw_path.trim().is_empty() {
        errors.push(format!("{source} has an empty {field}"));
        return;
    }
    if raw_path.contains('\\') {
        errors.push(format!(
            "{source} {field} `{raw_path}` must use forward slashes"
        ));
    }
    if !root.join(raw_path).exists() {
        errors.push(format!("{source} {field} `{raw_path}` does not exist"));
    }
}

fn validate_rails_owned_artifacts_registered(
    root: &Path,
    artifact_paths: &BTreeMap<String, String>,
    errors: &mut Vec<String>,
) {
    for artifact_dir in [
        ".rails/proposals",
        ".rails/specs",
        ".rails/adr",
        ".rails/closeouts",
        ".rails/plans",
        ".rails/support",
        ".rails/policy",
        ".rails/templates",
    ] {
        let path = root.join(artifact_dir);
        if path.exists() {
            validate_rails_owned_artifact_dir(root, &path, artifact_paths, errors);
        }
    }
}

fn validate_rails_owned_artifact_dir(
    root: &Path,
    dir: &Path,
    artifact_paths: &BTreeMap<String, String>,
    errors: &mut Vec<String>,
) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) => {
            errors.push(format!(
                "Rails-owned artifact directory `{}` could not be read: {err}",
                relative_display(root, dir)
            ));
            return;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                errors.push(format!(
                    "Rails-owned artifact directory `{}` has unreadable entry: {err}",
                    relative_display(root, dir)
                ));
                continue;
            }
        };
        let path = entry.path();
        if path.is_dir() {
            validate_rails_owned_artifact_dir(root, &path, artifact_paths, errors);
            continue;
        }
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if file_name == "README.md" {
            continue;
        }
        let relative = relative_display(root, &path);
        if !artifact_paths.contains_key(&relative) {
            errors.push(format!(
                "Rails-owned artifact `{relative}` must be registered in .rails/index.toml"
            ));
        }
    }
}

fn validate_rails_lane_tracker(
    root: &Path,
    lane: &RailsLane,
    artifact_kinds: &BTreeMap<String, String>,
    errors: &mut Vec<String>,
) {
    let path = root.join(&lane.path);
    if !path.exists() {
        return;
    }

    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(err) => {
            errors.push(format!(
                "Rails lane {} tracker `{}` could not be read: {err}",
                lane.id, lane.path
            ));
            return;
        }
    };
    let tracker = match toml::from_str::<RailsLaneTracker>(&raw) {
        Ok(tracker) => tracker,
        Err(err) => {
            errors.push(format!(
                "Rails lane {} tracker `{}` must parse as TOML: {err}",
                lane.id, lane.path
            ));
            return;
        }
    };

    if tracker.schema_version != "1.0" {
        errors.push(format!(
            "Rails lane {} tracker schema_version `{}` must be `1.0`",
            lane.id, tracker.schema_version
        ));
    }
    if tracker.id != lane.id {
        errors.push(format!(
            "Rails lane {} tracker id `{}` must match index id `{}`",
            lane.id, tracker.id, lane.id
        ));
    }
    if tracker.name != lane.name {
        errors.push(format!(
            "Rails lane {} tracker name `{}` must match index name `{}`",
            lane.id, tracker.name, lane.name
        ));
    }
    if tracker.status != lane.status {
        errors.push(format!(
            "Rails lane {} tracker status `{}` must match index status `{}`",
            lane.id, tracker.status, lane.status
        ));
    }
    if tracker.owner != lane.owner {
        errors.push(format!(
            "Rails lane {} tracker owner `{}` must match index owner `{}`",
            lane.id, tracker.owner, lane.owner
        ));
    }
    if tracker.objective.trim().is_empty() {
        errors.push(format!(
            "Rails lane {} tracker objective must be non-empty",
            lane.id
        ));
    }
    if tracker.end_state.is_empty() {
        errors.push(format!(
            "Rails lane {} tracker must list end_state entries",
            lane.id
        ));
    }
    for end_state in &tracker.end_state {
        if end_state.trim().is_empty() {
            errors.push(format!(
                "Rails lane {} tracker has an empty end_state entry",
                lane.id
            ));
        }
    }

    let mut work_item_ids = BTreeSet::<&str>::new();
    for work_item in &tracker.work_item {
        if work_item.id.trim().is_empty() {
            errors.push(format!(
                "Rails lane {} tracker has a work item with an empty id",
                lane.id
            ));
        } else if !work_item_ids.insert(work_item.id.as_str()) {
            errors.push(format!(
                "Rails lane {} tracker has duplicate work item id {}",
                lane.id, work_item.id
            ));
        }
    }

    for work_item in &tracker.work_item {
        validate_rails_status(
            "lane work item",
            &format!("{}/{}", lane.id, work_item.id),
            &work_item.status,
            &[
                "planned",
                "ready",
                "active",
                "blocked",
                "implemented",
                "superseded",
            ],
            errors,
        );
        if lane.status == "implemented"
            && !matches!(work_item.status.as_str(), "implemented" | "superseded")
        {
            errors.push(format!(
                "implemented Rails lane {} work item {} has status `{}`; work items must be implemented or superseded",
                lane.id, work_item.id, work_item.status
            ));
        }
        if work_item.proof.is_empty() {
            errors.push(format!(
                "Rails lane {} work item {} must list proof commands",
                lane.id, work_item.id
            ));
        }
        let source = format!("Rails lane {} work item {}", lane.id, work_item.id);
        validate_rails_lane_work_item_link(
            &source,
            "proposal",
            &work_item.proposal,
            "proposal",
            true,
            artifact_kinds,
            errors,
        );
        validate_rails_lane_work_item_link(
            &source,
            "spec",
            &work_item.spec,
            "spec",
            true,
            artifact_kinds,
            errors,
        );
        validate_rails_lane_work_item_link(
            &source,
            "adr",
            &work_item.adr,
            "adr",
            false,
            artifact_kinds,
            errors,
        );
        validate_rails_reference_path(
            root,
            &format!("Rails lane {} work item {}", lane.id, work_item.id),
            "implementation_plan",
            &work_item.implementation_plan,
            errors,
        );
        validate_rails_lane_work_item_dependencies(
            &source,
            "blocks",
            &work_item.blocks,
            &work_item.id,
            &work_item_ids,
            errors,
        );
        validate_rails_lane_work_item_dependencies(
            &source,
            "blocked_by",
            &work_item.blocked_by,
            &work_item.id,
            &work_item_ids,
            errors,
        );
        for proof in &work_item.proof {
            if proof.trim().is_empty() {
                errors.push(format!(
                    "Rails lane {} work item {} has an empty proof command",
                    lane.id, work_item.id
                ));
            }
        }
    }
}

fn validate_rails_lane_work_item_dependencies(
    source: &str,
    field: &str,
    dependencies: &[String],
    work_item_id: &str,
    work_item_ids: &BTreeSet<&str>,
    errors: &mut Vec<String>,
) {
    for dependency in dependencies {
        let dependency = dependency.trim();
        if dependency.is_empty() {
            errors.push(format!("{source} has an empty {field} reference"));
            continue;
        }
        if dependency == work_item_id {
            errors.push(format!(
                "{source} {field} references itself as {dependency}"
            ));
            continue;
        }
        if !work_item_ids.contains(dependency) {
            errors.push(format!(
                "{source} {field} references unknown work item id {dependency}"
            ));
        }
    }
}

fn validate_rails_lane_work_item_link(
    source: &str,
    field: &str,
    target_id: &str,
    expected_kind: &str,
    required: bool,
    artifact_kinds: &BTreeMap<String, String>,
    errors: &mut Vec<String>,
) {
    if target_id.trim().is_empty() {
        if required {
            errors.push(format!("{source} has an empty {field}"));
        }
        return;
    }

    let Some(actual_kind) = artifact_kinds.get(target_id) else {
        errors.push(format!(
            "{source} {field} references unknown artifact id {target_id}"
        ));
        return;
    };

    if actual_kind != expected_kind {
        errors.push(format!(
            "{source} {field} references {target_id}, which is kind `{actual_kind}`; expected `{expected_kind}`"
        ));
    }
}

fn validate_rails_artifact_kind(kind: &str, id: &str, errors: &mut Vec<String>) {
    let expected_prefix = match kind {
        "proposal" => Some("PERFGATE-PROP-"),
        "spec" => Some("PERFGATE-SPEC-"),
        "adr" => Some("PERFGATE-ADR-"),
        "plan" | "support" | "policy" | "closeout" | "template" => Some("PERFGATE-"),
        _ => None,
    };

    match expected_prefix {
        Some(prefix) if id.starts_with(prefix) => {}
        Some(prefix) => errors.push(format!(
            "Rails artifact {} kind `{}` must use id prefix `{}`",
            id, kind, prefix
        )),
        None => errors.push(format!(
            "Rails artifact {} uses unknown kind `{}`",
            id, kind
        )),
    }
}

fn validate_rails_status(
    label: &str,
    id: &str,
    status: &str,
    allowed: &[&str],
    errors: &mut Vec<String>,
) {
    if !allowed.contains(&status) {
        errors.push(format!(
            "Rails {label} {id} uses unknown status `{status}`; allowed: {}",
            allowed.join(", ")
        ));
    }
}

fn validate_rails_registered_path(
    root: &Path,
    label: &str,
    id: &str,
    raw_path: &str,
    errors: &mut Vec<String>,
) {
    if raw_path.contains('\\') {
        errors.push(format!(
            "Rails {label} {id} path `{raw_path}` must use forward slashes"
        ));
    }
    if !raw_path.starts_with(".rails/") {
        errors.push(format!(
            "Rails {label} {id} path `{raw_path}` must live under .rails/"
        ));
    }
    let path = root.join(raw_path);
    if !path.exists() {
        errors.push(format!(
            "Rails {label} {id} links to missing path `{raw_path}`"
        ));
    }
}

fn validate_rails_artifact_kind_path(artifact: &RailsArtifact, errors: &mut Vec<String>) {
    let expected_prefix = match artifact.kind.as_str() {
        "proposal" => ".rails/proposals/",
        "spec" => ".rails/specs/",
        "adr" => ".rails/adr/",
        "support" => ".rails/support/",
        "policy" => ".rails/policy/",
        "closeout" => ".rails/closeouts/",
        "plan" => ".rails/plans/",
        "template" => ".rails/templates/",
        _ => return,
    };

    if !artifact.path.starts_with(expected_prefix) {
        errors.push(format!(
            "Rails artifact {} kind `{}` path `{}` must live under `{}`",
            artifact.id, artifact.kind, artifact.path, expected_prefix
        ));
    }
}

fn validate_rails_artifact_path_identity(artifact: &RailsArtifact, errors: &mut Vec<String>) {
    if !matches!(
        artifact.kind.as_str(),
        "proposal" | "spec" | "adr" | "plan" | "closeout"
    ) {
        return;
    }

    let file_name = rails_path_file_name(&artifact.path);
    if !file_name.starts_with(&artifact.id) {
        errors.push(format!(
            "Rails artifact {} path `{}` filename must start with `{}`",
            artifact.id, artifact.path, artifact.id
        ));
    }
}

fn validate_rails_lane_path(lane: &RailsLane, errors: &mut Vec<String>) {
    let expected_path = format!(".rails/lanes/{}/tracker.toml", lane.id);
    if lane.path != expected_path {
        errors.push(format!(
            "Rails lane {} path `{}` must be `{}`",
            lane.id, lane.path, expected_path
        ));
    }
}

fn validate_rails_link(
    source_id: &str,
    field: &str,
    target_id: &str,
    expected_kind: &str,
    artifact_kinds: &BTreeMap<String, String>,
    errors: &mut Vec<String>,
) {
    let Some(actual_kind) = artifact_kinds.get(target_id) else {
        errors.push(format!(
            "Rails artifact {source_id} {field} references unknown artifact id {target_id}"
        ));
        return;
    };

    if actual_kind != expected_kind {
        errors.push(format!(
            "Rails artifact {source_id} {field} references {target_id}, which is kind `{actual_kind}`; expected `{expected_kind}`"
        ));
    }
}

fn rails_path_file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn collect_docs_source_errors(root: &Path) -> anyhow::Result<Vec<String>> {
    let mut errors = Vec::new();
    let docs = collect_source_docs(root)?;
    let id_re = Regex::new(r"^(PERFGATE-(?:PROP|SPEC|ADR)-\d{4})")
        .expect("source doc id regex should compile");
    let mut ids = BTreeMap::<String, PathBuf>::new();

    for doc in &docs {
        let display = relative_display(root, &doc.path);
        for header in doc.kind.required_headers() {
            if !doc.metadata.contains_key(*header) {
                errors.push(format!(
                    "{} {} is missing required `{}` metadata",
                    doc.kind.label(),
                    display,
                    header
                ));
            }
        }

        if matches!(doc.kind, SourceDocKind::Spec) {
            let status = doc.metadata.get("Status").map(String::as_str).unwrap_or("");
            if !matches!(
                status,
                "proposed" | "accepted" | "implemented" | "superseded"
            ) {
                errors.push(format!("spec {} uses unknown Status `{}`", display, status));
            }
        }

        if matches!(
            doc.kind,
            SourceDocKind::Proposal | SourceDocKind::Spec | SourceDocKind::Adr
        ) {
            let stem = doc
                .path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("");
            if let Some(captures) = id_re.captures(stem) {
                let id = captures[1].to_string();
                if let Some(previous) = ids.insert(id.clone(), doc.path.clone()) {
                    errors.push(format!(
                        "duplicate source-of-truth ID {} in {} and {}",
                        id,
                        relative_display(root, &previous),
                        display
                    ));
                }
            } else {
                errors.push(format!(
                    "{} {} filename must start with a PERFGATE ID",
                    doc.kind.label(),
                    display
                ));
            }
        }

        if matches!(doc.kind, SourceDocKind::Plan) {
            let linked_proposal = doc
                .metadata
                .get("Linked proposal")
                .map(|value| !is_blank_metadata_value(value))
                .unwrap_or(false);
            let linked_specs = doc
                .metadata
                .get("Linked specs")
                .map(|value| !is_blank_metadata_value(value))
                .unwrap_or(false);
            if !linked_proposal && !linked_specs {
                errors.push(format!(
                    "plan {} must link to at least one proposal or spec",
                    display
                ));
            }
        }

        for raw_path in extract_path_references_from_metadata(&doc.metadata) {
            validate_linked_path(root, &doc.path, &raw_path, &mut errors)?;
        }
    }

    validate_active_goal_toml(root, &mut errors)?;

    Ok(errors)
}

fn collect_source_docs(root: &Path) -> anyhow::Result<Vec<SourceDoc>> {
    let mut docs = Vec::new();
    for (kind, dir) in [
        (SourceDocKind::Proposal, "docs/proposals"),
        (SourceDocKind::Spec, "docs/specs"),
        (SourceDocKind::Adr, "docs/adr"),
    ] {
        for path in markdown_files_in(root.join(dir), false)? {
            docs.push(read_source_doc(kind, path)?);
        }
    }

    for path in markdown_files_in(root.join("plans"), true)? {
        docs.push(read_source_doc(SourceDocKind::Plan, path)?);
    }

    Ok(docs)
}

fn collect_existing_spec_ids(root: &Path) -> anyhow::Result<BTreeSet<String>> {
    let id_re =
        Regex::new(r"^(PERFGATE-SPEC-\d{4})").expect("source doc spec id regex should compile");
    let mut ids = BTreeSet::new();
    for path in markdown_files_in(root.join("docs/specs"), false)? {
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("");
        if let Some(captures) = id_re.captures(stem) {
            ids.insert(captures[1].to_string());
        }
    }
    Ok(ids)
}

fn read_source_doc(kind: SourceDocKind, path: PathBuf) -> anyhow::Result<SourceDoc> {
    let content =
        fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let metadata = parse_source_doc_metadata(&content);
    Ok(SourceDoc {
        kind,
        path,
        metadata,
    })
}

fn markdown_files_in(dir: PathBuf, recursive: bool) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if !dir.exists() {
        return Ok(files);
    }

    let mut pending = vec![dir];
    while let Some(current) = pending.pop() {
        for entry in fs::read_dir(&current)
            .with_context(|| format!("reading directory {}", current.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                if recursive {
                    pending.push(path);
                }
                continue;
            }
            if path.file_name().and_then(|name| name.to_str()) == Some("README.md") {
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
                files.push(path);
            }
        }
    }

    files.sort();
    Ok(files)
}

fn parse_source_doc_metadata(content: &str) -> BTreeMap<String, String> {
    let mut metadata = BTreeMap::new();
    let mut seen_title = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("# ") {
            seen_title = true;
            continue;
        }
        if !seen_title {
            continue;
        }
        if trimmed.starts_with("## ") {
            break;
        }
        if trimmed.is_empty() {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once(':')
            && key
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '/' || ch == ' ')
        {
            metadata.insert(key.trim().to_string(), value.trim().to_string());
        }
    }

    metadata
}

fn extract_path_references_from_metadata(metadata: &BTreeMap<String, String>) -> Vec<String> {
    let path_re = source_doc_path_regex();
    let mut paths = BTreeSet::new();

    for (key, value) in metadata {
        if !key.starts_with("Linked ") && key != "Support/status impact" {
            continue;
        }
        for captures in path_re.captures_iter(value) {
            paths.insert(clean_source_doc_path(&captures[0]));
        }
    }

    paths.into_iter().collect()
}

fn validate_active_goal_toml(root: &Path, errors: &mut Vec<String>) -> anyhow::Result<()> {
    let path = root.join(".codex/goals/active.toml");
    if !path.exists() {
        return Ok(());
    }

    let raw = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let value = match toml::from_str::<toml::Value>(&raw) {
        Ok(value) => value,
        Err(err) => {
            errors.push(format!(
                "{} must parse as TOML: {err}",
                relative_display(root, &path)
            ));
            return Ok(());
        }
    };

    let mut linked_paths = BTreeSet::new();
    collect_active_goal_link_references(&value, &mut linked_paths);
    for raw_path in linked_paths {
        validate_linked_path(root, &path, &raw_path, errors)?;
    }

    Ok(())
}

fn collect_active_goal_link_references(value: &toml::Value, paths: &mut BTreeSet<String>) {
    let Some(table) = value.as_table() else {
        return;
    };

    for key in [
        "linked_proposal",
        "linked_plan",
        "linked_specs",
        "linked_adrs",
        "linked_status",
        "linked_policy",
    ] {
        if let Some(value) = table.get(key) {
            collect_path_references_from_toml_leaf(value, paths);
        }
    }

    if let Some(work_items) = table.get("work_item").and_then(toml::Value::as_array) {
        for item in work_items {
            let Some(item) = item.as_table() else {
                continue;
            };
            for key in ["proposal", "spec", "adr", "plan", "implementation_plan"] {
                if let Some(value) = item.get(key) {
                    collect_path_references_from_toml_leaf(value, paths);
                }
            }
        }
    }
}

fn collect_path_references_from_toml_leaf(value: &toml::Value, paths: &mut BTreeSet<String>) {
    match value {
        toml::Value::String(value) => {
            for captures in source_doc_path_regex().captures_iter(value) {
                paths.insert(clean_source_doc_path(&captures[0]));
            }
        }
        toml::Value::Array(values) => {
            for value in values {
                collect_path_references_from_toml_leaf(value, paths);
            }
        }
        _ => {}
    }
}

fn validate_linked_path(
    root: &Path,
    source: &Path,
    raw_path: &str,
    errors: &mut Vec<String>,
) -> anyhow::Result<()> {
    if raw_path.contains('*') {
        let pattern = root.join(raw_path).to_string_lossy().replace('\\', "/");
        let mut matched = false;
        for entry in glob(&pattern).with_context(|| format!("glob pattern {pattern}"))? {
            if entry?.exists() {
                matched = true;
                break;
            }
        }
        if !matched {
            errors.push(format!(
                "{} links to glob `{}` but it matches no files",
                relative_display(root, source),
                raw_path
            ));
        }
    } else if !root.join(raw_path).exists() {
        errors.push(format!(
            "{} links to missing file `{}`",
            relative_display(root, source),
            raw_path
        ));
    }

    Ok(())
}

fn source_doc_path_regex() -> Regex {
    Regex::new(
        r"(?:\.codex|\.github|\.rails|docs|plans|policy|schemas|fixtures|examples|crates|xtask)/[A-Za-z0-9_./*{}-]+",
    )
    .expect("source doc path regex should compile")
}

fn clean_source_doc_path(path: &str) -> String {
    path.trim_end_matches(['.', ',', ';', ')', ']', '"', '\''])
        .to_string()
}

fn is_blank_metadata_value(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none")
}

fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn cmd_product_claims_check(path: &Path) -> anyhow::Result<()> {
    let content =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let spec_ids = collect_existing_spec_ids(&workspace_root_path())?;
    let errors = collect_product_claim_errors(&content, &spec_ids);

    if !errors.is_empty() {
        println!("Found {} product claim proof-map error(s):", errors.len());
        for error in &errors {
            println!("  - {}", error);
        }

        anyhow::bail!(
            "{} product claim proof-map issue(s) found. Fix {}.",
            errors.len(),
            path.display()
        );
    }

    println!("  OK  product claim proof map is valid");
    Ok(())
}

#[derive(Debug)]
struct ProductClaimSection {
    id: String,
    line: usize,
    body: String,
}

fn collect_product_claim_errors(
    content: &str,
    concrete_spec_ids: &BTreeSet<String>,
) -> Vec<String> {
    let mut errors = Vec::new();
    let claims = extract_product_claim_sections(content);
    let mut ids = BTreeSet::new();
    let planned_spec_re = Regex::new(r"\b(PERFGATE-SPEC-\d{4})\b.*\bplanned\b")
        .expect("planned spec regex should compile");

    if claims.is_empty() {
        errors
            .push("PRODUCT_CLAIMS.md must contain at least one `## PG-CLAIM-NNNN` section".into());
        return errors;
    }

    for claim in claims {
        if !ids.insert(claim.id.clone()) {
            errors.push(format!("duplicate claim id `{}`", claim.id));
        }

        let tier = claim_field(&claim.body, "Tier");
        match tier.as_deref() {
            Some("stable" | "supported" | "advisory" | "experimental" | "deprecated") => {}
            Some(value) => errors.push(format!(
                "{} line {} uses unknown tier `{}`",
                claim.id, claim.line, value
            )),
            None => errors.push(format!(
                "{} line {} is missing `Tier:`",
                claim.id, claim.line
            )),
        }

        if let Some(freshness) = claim_field(&claim.body, "Proof freshness") {
            match freshness.as_str() {
                "current" | "recent" | "stale" | "superseded" | "unproven" => {}
                value => errors.push(format!(
                    "{} line {} uses unknown proof freshness `{}`",
                    claim.id, claim.line, value
                )),
            }

            if matches!(tier.as_deref(), Some("stable" | "supported"))
                && matches!(freshness.as_str(), "stale" | "superseded" | "unproven")
            {
                errors.push(format!(
                    "{} line {} uses `{}` proof freshness for a `{}` claim; refresh proof or lower the claim language",
                    claim.id,
                    claim.line,
                    freshness,
                    tier.as_deref().unwrap_or("unknown")
                ));
            }
        }

        if claim_field(&claim.body, "Surface").is_none() {
            errors.push(format!(
                "{} line {} is missing `Surface:`",
                claim.id, claim.line
            ));
        }

        if claim_field(&claim.body, "Review after").is_none() {
            errors.push(format!(
                "{} line {} is missing `Review after:`",
                claim.id, claim.line
            ));
        }

        if !claim.body.contains("Proof commands:") {
            errors.push(format!(
                "{} line {} is missing `Proof commands:`",
                claim.id, claim.line
            ));
        } else if !claim
            .body
            .lines()
            .any(|line| line.trim_start().starts_with("cargo +1.95.0 "))
        {
            errors.push(format!(
                "{} line {} must list at least one cargo +1.95.0 proof command",
                claim.id, claim.line
            ));
        }

        if !claim.body.contains("Linked tests:")
            && !claim.body.contains("Linked policy:")
            && !claim.body.contains("Linked gates:")
        {
            errors.push(format!(
                "{} line {} must include linked tests, policy, or gates",
                claim.id, claim.line
            ));
        }

        for (offset, line) in claim.body.lines().enumerate() {
            let Some(captures) = planned_spec_re.captures(line) else {
                continue;
            };
            let spec_id = &captures[1];
            if concrete_spec_ids.contains(spec_id) {
                errors.push(format!(
                    "{} line {} references `{}` as planned, but a concrete spec file exists",
                    claim.id,
                    claim.line + offset + 1,
                    spec_id
                ));
            }
        }
    }

    errors
}

fn extract_product_claim_sections(content: &str) -> Vec<ProductClaimSection> {
    let heading_re =
        Regex::new(r"^## (PG-CLAIM-\d{4})\b").expect("product claim heading regex should compile");
    let mut claims = Vec::new();
    let mut active: Option<(String, usize, String)> = None;

    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;
        if line.starts_with("## ") {
            if let Some((id, line, body)) = active.take() {
                claims.push(ProductClaimSection { id, line, body });
            }
            if let Some(captures) = heading_re.captures(line) {
                active = Some((captures[1].to_string(), line_num, String::new()));
            }
            continue;
        }

        if let Some((_, _, body)) = active.as_mut() {
            body.push_str(line);
            body.push('\n');
        }
    }

    if let Some((id, line, body)) = active.take() {
        claims.push(ProductClaimSection { id, line, body });
    }

    claims
}

fn claim_field(body: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    body.lines().find_map(|line| {
        let line = line.trim();
        let value = line.strip_prefix(&prefix)?.trim();
        if value.is_empty() {
            None
        } else {
            Some(value.to_string())
        }
    })
}

// ---------------------------------------------------------------------------
// doc-test: validate CLI examples in documentation
// ---------------------------------------------------------------------------

/// A CLI invocation extracted from a documentation file.
#[derive(Debug, Clone)]
struct DocCommand {
    /// Source file
    file: PathBuf,
    /// Line number (1-based) where the command was found
    line: usize,
    /// The raw command text
    raw: String,
    /// Subcommand path (e.g. ["check"] or ["baseline", "list"])
    subcommand: Vec<String>,
    /// Flags used (e.g. ["--config", "--bench", "--mode"])
    flags: Vec<String>,
}

/// A structured data snippet extracted from a documentation file.
#[derive(Debug, Clone)]
struct DocDataSnippet {
    /// Source file
    file: PathBuf,
    /// Line number (1-based) where the fenced block starts
    line: usize,
    /// Fence language
    kind: DocDataKind,
    /// Fenced block contents
    raw: String,
}

/// Structured documentation snippet formats that `doc-test` can validate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DocDataKind {
    Toml,
    Json,
    Yaml,
}

impl DocDataKind {
    fn from_fence(fence: &str) -> Option<Self> {
        let info = fence
            .trim_start_matches("```")
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();

        match info.as_str() {
            "toml" => Some(Self::Toml),
            "json" => Some(Self::Json),
            "yaml" | "yml" => Some(Self::Yaml),
            _ => None,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Toml => "TOML",
            Self::Json => "JSON",
            Self::Yaml => "YAML",
        }
    }
}

/// Collect default current-user docs.
fn default_doc_files() -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for name in [
        "README.md",
        "docs/CONFIG.md",
        "docs/DEBUGGING_FIRST_CI_RUN.md",
        "docs/EVIDENCE_INTAKE.md",
        "docs/FLAKINESS.md",
        "docs/FLEET_AGGREGATION.md",
        "docs/PAIRED_BENCHMARKING.md",
        "docs/PIPELINE.md",
        "docs/BASELINE_SERVICE_DESIGN.md",
    ] {
        let p = PathBuf::from(name);
        if p.exists() {
            files.push(p);
        }
    }

    for entry in glob("docs/GETTING_STARTED_*.md")? {
        files.push(entry?);
    }

    files.sort();
    Ok(files)
}

/// Extract perfgate CLI invocations from markdown fenced code blocks.
///
/// Handles both direct `perfgate <subcommand>` and
/// `cargo run -p perfgate-cli -- <subcommand>` patterns.
/// Multi-line commands joined with trailing backslash are supported.
fn extract_commands(file: &Path, content: &str) -> Vec<DocCommand> {
    let mut commands = Vec::new();
    let mut in_code_block = false;
    let mut scan_code_block = false;
    // Accumulated lines for multi-line commands (trailing backslash)
    let mut continuation: Option<(usize, String)> = None;

    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;

        let trimmed_start = line.trim_start();
        if trimmed_start.starts_with("```") {
            if in_code_block {
                // Closing fence -- flush any pending continuation
                if scan_code_block
                    && let Some((start, acc)) = continuation.take()
                    && let Some(cmd) = parse_perfgate_line(file, start, &acc)
                {
                    commands.push(cmd);
                }
                scan_code_block = false;
            } else {
                scan_code_block = is_shell_code_fence(trimmed_start);
            }
            in_code_block = !in_code_block;
            continue;
        }

        if !in_code_block || !scan_code_block {
            continue;
        }

        let trimmed = line.trim();

        // Handle line continuation (trailing backslash)
        if let Some((start, ref mut acc)) = continuation {
            // Append this line (strip leading whitespace, it's a continuation)
            acc.push(' ');
            if let Some(stripped) = trimmed.strip_suffix('\\') {
                acc.push_str(stripped.trim());
            } else {
                acc.push_str(trimmed);
                // End of continuation -- parse accumulated line
                let full = std::mem::take(acc);
                let start_line = start;
                continuation = None;
                if let Some(cmd) = parse_perfgate_line(file, start_line, &full) {
                    commands.push(cmd);
                }
            }
            continue;
        }

        // New line -- check for trailing backslash
        if let Some(stripped) = trimmed.strip_suffix('\\') {
            // Start a continuation
            continuation = Some((line_num, stripped.trim().to_string()));
            continue;
        }

        if let Some(cmd) = parse_perfgate_line(file, line_num, trimmed) {
            commands.push(cmd);
        }
    }

    // Flush any trailing continuation (shouldn't normally happen)
    if let Some((start, acc)) = continuation
        && let Some(cmd) = parse_perfgate_line(file, start, &acc)
    {
        commands.push(cmd);
    }

    commands
}

/// Extract structured data snippets from markdown fenced code blocks.
fn extract_data_snippets(file: &Path, content: &str) -> Vec<DocDataSnippet> {
    let mut snippets = Vec::new();
    let mut in_code_block = false;
    let mut active: Option<(DocDataKind, usize, String)> = None;

    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;
        let trimmed_start = line.trim_start();

        if trimmed_start.starts_with("```") {
            if in_code_block {
                if let Some((kind, start_line, raw)) = active.take() {
                    snippets.push(DocDataSnippet {
                        file: file.to_path_buf(),
                        line: start_line,
                        kind,
                        raw,
                    });
                }
            } else if let Some(kind) = DocDataKind::from_fence(trimmed_start) {
                active = Some((kind, line_num, String::new()));
            }

            in_code_block = !in_code_block;
            continue;
        }

        if let Some((_, _, raw)) = active.as_mut() {
            raw.push_str(line);
            raw.push('\n');
        }
    }

    snippets
}

fn is_shell_code_fence(fence: &str) -> bool {
    let info = fence
        .trim_start_matches("```")
        .split_whitespace()
        .next()
        .unwrap_or("");

    matches!(
        info.to_ascii_lowercase().as_str(),
        "" | "bash" | "sh" | "shell" | "console" | "terminal" | "powershell" | "pwsh"
    )
}

/// Try to parse a single line as a perfgate CLI invocation.
fn parse_perfgate_line(file: &Path, line: usize, text: &str) -> Option<DocCommand> {
    // Strip shell prefixes like `$ ` or `> `
    let text = text
        .strip_prefix("$ ")
        .or_else(|| text.strip_prefix("> "))
        .unwrap_or(text);

    // Match `perfgate <args>` or `cargo run -p perfgate-cli [--bin perfgate] -- <args>`
    let args_str = if let Some(rest) = strip_cargo_run_prefix(text) {
        rest
    } else if let Some(rest) = text.strip_prefix("perfgate ") {
        rest
    } else if text == "perfgate" {
        ""
    } else {
        return None;
    };

    let tokens = shell_tokenize(args_str);
    if tokens.is_empty() {
        return None;
    }

    // Extract subcommand path and flags
    let mut subcommand = Vec::new();
    let mut flags = Vec::new();

    for token in &tokens {
        if token == "--" {
            // End-of-flags separator: everything after is passed to the sub-process
            break;
        }
        if token.starts_with('-') {
            // It's a flag -- extract just the flag name (e.g. "--config" from "--config=foo")
            let flag = token.split('=').next().unwrap_or(token).to_string();
            flags.push(flag);
        } else if flags.is_empty() {
            // Before any flags, it's part of the subcommand path
            subcommand.push(token.clone());
        }
        // After the first flag, positional args are arguments, not subcommands
    }

    // Skip if no subcommand found
    if subcommand.is_empty() {
        return None;
    }

    // The subcommand path might include positional arguments after the actual subcommand.
    // We'll validate against --help to determine which are real subcommands.

    Some(DocCommand {
        file: file.to_path_buf(),
        line,
        raw: text.to_string(),
        subcommand,
        flags,
    })
}

/// Strip the `cargo run -p perfgate-cli [--bin perfgate] [--release] -- ` prefix.
fn strip_cargo_run_prefix(text: &str) -> Option<&str> {
    // Look for `cargo run -p perfgate-cli` followed by optional flags and then `--`
    let re = Regex::new(
        r"^cargo\s+run\s+(?:--release\s+)?-p\s+perfgate-cli(?:\s+--bin\s+perfgate)?(?:\s+--release)?\s+--\s+",
    )
    .ok()?;

    re.find(text).map(|m| &text[m.end()..])
}

/// Simple shell tokenizer: splits on whitespace, respects double quotes.
fn shell_tokenize(s: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for ch in s.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
            }
            ' ' | '\t' if !in_quotes => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

/// Get the list of valid subcommands from `perfgate --help` output.
fn get_valid_subcommands(cargo: &str) -> anyhow::Result<BTreeSet<String>> {
    let output = std::process::Command::new(cargo)
        .args(["run", "-p", "perfgate-cli", "--", "--help"])
        .output()
        .context("running perfgate --help")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_subcommands_from_help(&stdout)
}

/// Parse subcommand names from --help output.
fn parse_subcommands_from_help(help_text: &str) -> anyhow::Result<BTreeSet<String>> {
    let mut subcommands = BTreeSet::new();
    let mut in_commands_section = false;

    for line in help_text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Commands:") {
            in_commands_section = true;
            continue;
        }
        if !in_commands_section {
            continue;
        }

        // Blank lines within the section are fine -- skip them
        if trimmed.is_empty() {
            continue;
        }

        // A non-indented, non-empty line means we've left the Commands section
        if !line.starts_with(' ') {
            in_commands_section = false;
            continue;
        }

        // Parse "  subcommand   description"
        if let Some(name) = trimmed.split_whitespace().next()
            && name != "help"
        {
            subcommands.insert(name.to_string());
        }
    }

    Ok(subcommands)
}

/// Get valid flags for a specific subcommand by running `perfgate <subcmd> --help`.
fn get_valid_flags(cargo: &str, subcmd: &[String]) -> anyhow::Result<BTreeSet<String>> {
    let mut args = vec!["run", "-p", "perfgate-cli", "--"];
    let subcmd_strs: Vec<&str> = subcmd.iter().map(|s| s.as_str()).collect();
    args.extend_from_slice(&subcmd_strs);
    args.push("--help");

    let output = std::process::Command::new(cargo)
        .args(&args)
        .output()
        .with_context(|| format!("running perfgate {} --help", subcmd.join(" ")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    parse_flags_from_help(&combined)
}

/// Parse flag names from --help output.
fn parse_flags_from_help(help_text: &str) -> anyhow::Result<BTreeSet<String>> {
    let mut flags = BTreeSet::new();
    let re = Regex::new(r"--[a-zA-Z][a-zA-Z0-9_-]*").context("compile flag regex")?;

    for mat in re.find_iter(help_text) {
        flags.insert(mat.as_str().to_string());
    }

    // Always include --help and --version as universally valid
    flags.insert("--help".to_string());
    flags.insert("--version".to_string());

    Ok(flags)
}

fn cmd_doc_test(extra_files: Vec<PathBuf>) -> anyhow::Result<()> {
    println!("Validating CLI examples in documentation...\n");

    // Collect files to scan
    let mut files = default_doc_files()?;
    files.extend(extra_files);
    files.sort();
    files.dedup();

    if files.is_empty() {
        anyhow::bail!("no documentation files found");
    }

    // Extract all commands from all files
    let mut all_commands = Vec::new();
    let mut all_data_snippets = Vec::new();
    for file in &files {
        let content =
            fs::read_to_string(file).with_context(|| format!("read {}", file.display()))?;
        let cmds = extract_commands(file, &content);
        all_commands.extend(cmds);
        let snippets = extract_data_snippets(file, &content);
        all_data_snippets.extend(snippets);
    }

    println!(
        "Found {} CLI examples and {} structured snippets in {} files\n",
        all_commands.len(),
        all_data_snippets.len(),
        files.len()
    );

    let mut errors: Vec<String> = Vec::new();
    let mut checked = 0u32;

    if all_commands.is_empty() {
        println!("No perfgate CLI examples found in documentation.");
    } else {
        // Get valid subcommands from the binary
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
        let valid_subcommands = get_valid_subcommands(&cargo)?;

        println!(
            "Valid subcommands: {}\n",
            valid_subcommands
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        );

        // For each unique subcommand, get valid flags (caching)
        let mut flag_cache: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

        for cmd in &all_commands {
            checked += 1;
            let first_subcmd = &cmd.subcommand[0];

            // Check if the top-level subcommand is valid
            if !valid_subcommands.contains(first_subcmd) {
                errors.push(format!(
                    "  {}:{}: unknown subcommand '{}'\n    {}",
                    cmd.file.display(),
                    cmd.line,
                    first_subcmd,
                    cmd.raw
                ));
                continue;
            }

            // Determine the effective subcommand path for --help.
            // For subcommands like "baseline list", we need to pass both words.
            // We try the longest prefix that is a valid sub-subcommand.
            let subcmd_path = resolve_subcommand_path(&cargo, &cmd.subcommand, &mut flag_cache)?;

            // Get valid flags for this subcommand
            let cache_key = subcmd_path.join(" ");
            if !flag_cache.contains_key(&cache_key) {
                let flags = get_valid_flags(&cargo, &subcmd_path)?;
                flag_cache.insert(cache_key.clone(), flags);
            }
            let valid_flags = &flag_cache[&cache_key];

            // Check each flag
            for flag in &cmd.flags {
                if !valid_flags.contains(flag) {
                    errors.push(format!(
                        "  {}:{}: unknown flag '{}' for 'perfgate {}'\n    {}",
                        cmd.file.display(),
                        cmd.line,
                        flag,
                        cache_key,
                        cmd.raw
                    ));
                }
            }
        }
    }

    let mut structured_checked = 0u32;
    let mut schema_checked = 0u32;
    for snippet in &all_data_snippets {
        structured_checked += 1;
        match validate_data_snippet(snippet) {
            Ok(Some(_schema)) => {
                schema_checked += 1;
            }
            Ok(None) => {}
            Err(err) => errors.push(format!(
                "  {}:{}: invalid {} snippet: {err:#}",
                snippet.file.display(),
                snippet.line,
                snippet.kind.label()
            )),
        }
    }

    println!(
        "Checked {} CLI examples and {} structured snippets",
        checked, structured_checked
    );
    if schema_checked > 0 {
        println!(
            "Validated {} versioned JSON schema example(s)",
            schema_checked
        );
    }

    if errors.is_empty() {
        println!("\n  OK  all documentation examples are valid");
        Ok(())
    } else {
        println!("\nFound {} error(s):\n", errors.len());
        for err in &errors {
            println!("{}\n", err);
        }
        anyhow::bail!(
            "{} documentation example(s) failed validation",
            errors.len()
        );
    }
}

fn validate_data_snippet(snippet: &DocDataSnippet) -> anyhow::Result<Option<&'static str>> {
    match snippet.kind {
        DocDataKind::Toml => {
            toml::from_str::<toml::Value>(&snippet.raw).context("parse TOML")?;
            Ok(None)
        }
        DocDataKind::Json => {
            let value: serde_json::Value =
                serde_json::from_str(&snippet.raw).context("parse JSON")?;
            validate_versioned_json_example(value)
        }
        DocDataKind::Yaml => {
            yaml_serde::from_str::<yaml_serde::Value>(&snippet.raw).context("parse YAML")?;
            Ok(None)
        }
    }
}

fn validate_versioned_json_example(
    value: serde_json::Value,
) -> anyhow::Result<Option<&'static str>> {
    let schema = value.get("schema").and_then(serde_json::Value::as_str);
    match schema {
        Some(perfgate_types::RUN_SCHEMA_V1) => {
            serde_json::from_value::<perfgate_types::RunReceipt>(value)
                .context("deserialize perfgate.run.v1 example")?;
            Ok(Some(perfgate_types::RUN_SCHEMA_V1))
        }
        Some(perfgate_types::COMPARE_SCHEMA_V1) => {
            serde_json::from_value::<perfgate_types::CompareReceipt>(value)
                .context("deserialize perfgate.compare.v1 example")?;
            Ok(Some(perfgate_types::COMPARE_SCHEMA_V1))
        }
        Some(perfgate_types::PROBE_SCHEMA_V1) => {
            serde_json::from_value::<perfgate_types::ProbeReceipt>(value)
                .context("deserialize perfgate.probe.v1 example")?;
            Ok(Some(perfgate_types::PROBE_SCHEMA_V1))
        }
        Some(perfgate_types::PROBE_COMPARE_SCHEMA_V1) => {
            serde_json::from_value::<perfgate_types::ProbeCompareReceipt>(value)
                .context("deserialize perfgate.probe_compare.v1 example")?;
            Ok(Some(perfgate_types::PROBE_COMPARE_SCHEMA_V1))
        }
        Some(perfgate_types::SCENARIO_SCHEMA_V1) => {
            serde_json::from_value::<perfgate_types::ScenarioReceipt>(value)
                .context("deserialize perfgate.scenario.v1 example")?;
            Ok(Some(perfgate_types::SCENARIO_SCHEMA_V1))
        }
        Some(perfgate_types::TRADEOFF_SCHEMA_V1) => {
            serde_json::from_value::<perfgate_types::TradeoffReceipt>(value)
                .context("deserialize perfgate.tradeoff.v1 example")?;
            Ok(Some(perfgate_types::TRADEOFF_SCHEMA_V1))
        }
        Some(perfgate_types::DECISION_INDEX_SCHEMA_V1) => {
            serde_json::from_value::<perfgate_types::DecisionArtifactIndex>(value)
                .context("deserialize perfgate.decision_index.v1 example")?;
            Ok(Some(perfgate_types::DECISION_INDEX_SCHEMA_V1))
        }
        Some(perfgate_types::DECISION_BUNDLE_SCHEMA_V1) => {
            serde_json::from_value::<perfgate_types::DecisionBundleReceipt>(value)
                .context("deserialize perfgate.decision_bundle.v1 example")?;
            Ok(Some(perfgate_types::DECISION_BUNDLE_SCHEMA_V1))
        }
        Some(perfgate_types::baseline_service::DECISION_RECORD_SCHEMA_V1) => {
            serde_json::from_value::<perfgate_types::baseline_service::DecisionRecord>(value)
                .context("deserialize perfgate.decision_record.v1 example")?;
            Ok(Some(
                perfgate_types::baseline_service::DECISION_RECORD_SCHEMA_V1,
            ))
        }
        Some(perfgate_types::AGGREGATE_SCHEMA_V1) => {
            serde_json::from_value::<perfgate_types::AggregateReceipt>(value)
                .context("deserialize perfgate.aggregate.v1 example")?;
            Ok(Some(perfgate_types::AGGREGATE_SCHEMA_V1))
        }
        Some(perfgate_types::RATCHET_SCHEMA_V1) => {
            serde_json::from_value::<perfgate_types::RatchetReceipt>(value)
                .context("deserialize perfgate.ratchet.v1 example")?;
            Ok(Some(perfgate_types::RATCHET_SCHEMA_V1))
        }
        Some(perfgate_types::REPAIR_CONTEXT_SCHEMA_V1) => {
            serde_json::from_value::<perfgate_types::RepairContextReceipt>(value)
                .context("deserialize perfgate.repair_context.v1 example")?;
            Ok(Some(perfgate_types::REPAIR_CONTEXT_SCHEMA_V1))
        }
        Some(perfgate_types::SENSOR_REPORT_SCHEMA_V1) => {
            serde_json::from_value::<perfgate_types::SensorReport>(value)
                .context("deserialize sensor.report.v1 example")?;
            Ok(Some(perfgate_types::SENSOR_REPORT_SCHEMA_V1))
        }
        _ => {
            if value.get("report_type").and_then(serde_json::Value::as_str)
                == Some(perfgate_types::REPORT_SCHEMA_V1)
            {
                serde_json::from_value::<perfgate_types::PerfgateReport>(value)
                    .context("deserialize perfgate.report.v1 example")?;
                Ok(Some(perfgate_types::REPORT_SCHEMA_V1))
            } else {
                Ok(None)
            }
        }
    }
}

/// Resolve the subcommand path (e.g., ["baseline", "list"]) by checking
/// if deeper subcommands are valid.
fn resolve_subcommand_path(
    cargo: &str,
    tokens: &[String],
    cache: &mut BTreeMap<String, BTreeSet<String>>,
) -> anyhow::Result<Vec<String>> {
    if tokens.len() <= 1 {
        return Ok(tokens.to_vec());
    }

    // Try the first two tokens as a subcommand path (e.g. "baseline list")
    let two_deep = &tokens[..2];
    let cache_key = two_deep.join(" ");

    if let std::collections::btree_map::Entry::Vacant(e) = cache.entry(cache_key) {
        // Try getting help for the two-level subcommand
        if let Ok(flags) = get_valid_flags(cargo, two_deep)
            && !flags.is_empty()
        {
            e.insert(flags);
            return Ok(two_deep.to_vec());
        }
    } else {
        return Ok(two_deep.to_vec());
    }

    // Fall back to just the first token
    Ok(vec![tokens[0].clone()])
}

#[cfg(test)]
mod tests {
    use super::*;
    use xtask::*;

    #[test]
    fn mutants_crate_mapping_and_targets() {
        assert_eq!(MutantsCrate::Domain.as_package_name(), "perfgate");
        assert_eq!(MutantsCrate::Types.as_package_name(), "perfgate-types");
        assert_eq!(MutantsCrate::App.as_package_name(), "perfgate");
        assert_eq!(MutantsCrate::Cli.as_package_name(), "perfgate-cli");

        assert_eq!(MutantsCrate::Domain.target_kill_rate(), 100);
        assert_eq!(MutantsCrate::Types.target_kill_rate(), 95);
        assert_eq!(MutantsCrate::App.target_kill_rate(), 90);
        assert_eq!(MutantsCrate::Cli.target_kill_rate(), 70);
    }

    #[test]
    fn ripr_plus_badge_shape_is_stable() {
        let badge = ShieldsEndpointBadge {
            schema_version: 1,
            label: "ripr+".to_string(),
            message: "0".to_string(),
            color: "brightgreen".to_string(),
        };

        validate_shields_badge(&badge, Some("ripr+")).unwrap();
    }

    #[test]
    fn ripr_version_parser_accepts_expected_shape() {
        assert_eq!(parse_ripr_version("ripr 0.5.0\n"), Some("0.5.0"));
        assert_eq!(parse_ripr_version("ripr 0.7.0"), Some("0.7.0"));
    }

    #[test]
    fn ripr_version_parser_rejects_unknown_shape() {
        assert_eq!(parse_ripr_version("0.5.0"), None);
        assert_eq!(parse_ripr_version("perfgate 0.5.0"), None);
    }

    #[test]
    fn scanner_safe_badge_shape_is_stable() {
        let badge = ShieldsEndpointBadge {
            schema_version: 1,
            label: "fixtures".to_string(),
            message: "scanner-safe".to_string(),
            color: "brightgreen".to_string(),
        };

        validate_shields_badge(&badge, Some("fixtures")).unwrap();
    }

    #[test]
    fn shields_badge_rejects_empty_message() {
        let badge = ShieldsEndpointBadge {
            schema_version: 1,
            label: "ripr+".to_string(),
            message: " ".to_string(),
            color: "brightgreen".to_string(),
        };

        assert!(validate_shields_badge(&badge, Some("ripr+")).is_err());
    }

    #[test]
    fn run_reports_failure_and_success() {
        #[cfg(windows)]
        {
            assert!(run("cmd", ["/c", "exit", "1"]).is_err());
            assert!(run("cmd", ["/c", "exit", "0"]).is_ok());
        }

        #[cfg(unix)]
        {
            assert!(run("sh", ["-c", "exit 1"]).is_err());
            assert!(run("sh", ["-c", "exit 0"]).is_ok());
        }
    }

    fn write_test_file(root: &Path, relative: &str, content: &str) {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        fs::write(path, content).expect("write test file");
    }

    fn write_minimal_rails_stack(root: &Path, include_closeout: bool) {
        write_test_file(root, "docs/rails.md", "# Rails\n");
        write_test_file(root, "docs/contributing/rails.md", "# Rails contributing\n");
        write_test_file(
            root,
            ".rails/proposals/PERFGATE-PROP-9999-demo.md",
            "# Proposal\n",
        );
        write_test_file(root, ".rails/specs/PERFGATE-SPEC-9999-demo.md", "# Spec\n");
        write_test_file(
            root,
            ".rails/lanes/demo-lane/tracker.toml",
            r#"schema_version = "1.0"

id = "demo-lane"
name = "Demo lane"
status = "implemented"
owner = "test"

objective = """
Demo lane objective.
"""

end_state = [
  "Demo lane done.",
]
"#,
        );
        write_test_file(
            root,
            ".rails/lanes/demo-lane/implementation-plan.md",
            "# Demo lane plan\n",
        );
        if include_closeout {
            write_test_file(
                root,
                ".rails/closeouts/PERFGATE-CLOSEOUT-9999-demo-lane.md",
                "# Demo lane: closeout\n",
            );
        }

        let closeout_artifact = if include_closeout {
            r#"
[[artifact]]
id = "PERFGATE-CLOSEOUT-9999"
kind = "closeout"
path = ".rails/closeouts/PERFGATE-CLOSEOUT-9999-demo-lane.md"
status = "implemented"
owner = "test"
linked_proposal = "PERFGATE-PROP-9999"
linked_specs = ["PERFGATE-SPEC-9999"]
"#
        } else {
            ""
        };

        write_test_file(
            root,
            ".rails/index.toml",
            &format!(
                r#"schema_version = "1.0"

[project]
repo = "perfgate"
framework = "rails"
root = ".rails"

[conventions]
proposal_prefix = "PERFGATE-PROP"
spec_prefix = "PERFGATE-SPEC"
adr_prefix = "PERFGATE-ADR"
lane_prefix = "PERFGATE-LANE"

[external_namespaces]
codex = ".codex"
speckit = ".spec"
claude = ".claude"
jules = ".jules"

[[artifact]]
id = "PERFGATE-PROP-9999"
kind = "proposal"
path = ".rails/proposals/PERFGATE-PROP-9999-demo.md"
status = "implemented"
owner = "test"

[[artifact]]
id = "PERFGATE-SPEC-9999"
kind = "spec"
path = ".rails/specs/PERFGATE-SPEC-9999-demo.md"
status = "implemented"
owner = "test"
linked_proposal = "PERFGATE-PROP-9999"
{closeout_artifact}
[[lane]]
id = "demo-lane"
name = "Demo lane"
path = ".rails/lanes/demo-lane/tracker.toml"
status = "implemented"
owner = "test"
"#
            ),
        );
    }

    fn append_rails_index(root: &Path, content: &str) {
        let path = root.join(".rails/index.toml");
        let mut index = fs::read_to_string(&path).expect("read rails index");
        index.push_str(content);
        fs::write(path, index).expect("append rails index");
    }

    fn replace_rails_index(root: &Path, from: &str, to: &str) {
        let path = root.join(".rails/index.toml");
        let index = fs::read_to_string(&path).expect("read rails index");
        assert!(
            index.contains(from),
            "rails index did not contain expected text: {from}"
        );
        fs::write(path, index.replace(from, to)).expect("replace rails index");
    }

    #[test]
    fn rails_check_accepts_implemented_lane_with_registered_closeout() {
        let root = unique_temp_dir("perfgate_rails_closeout_ok");
        write_minimal_rails_stack(&root, true);

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert_eq!(errors, Vec::<String>::new());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_requires_registered_closeout_for_implemented_lane() {
        let root = unique_temp_dir("perfgate_rails_closeout_missing");
        write_minimal_rails_stack(&root, false);

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors
                .iter()
                .any(|error| error.contains("implemented Rails lane demo-lane")),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_unfinished_work_items_in_implemented_lane() {
        let root = unique_temp_dir("perfgate_rails_implemented_lane_unfinished_work_item");
        write_minimal_rails_stack(&root, true);
        let tracker_path = root.join(".rails/lanes/demo-lane/tracker.toml");
        let mut tracker = fs::read_to_string(&tracker_path).expect("read tracker");
        tracker.push_str(
            r#"
[[work_item]]
id = "unfinished"
status = "ready"
proposal = "PERFGATE-PROP-9999"
spec = "PERFGATE-SPEC-9999"
adr = ""
implementation_plan = ".rails/lanes/demo-lane/implementation-plan.md"
blocks = []
blocked_by = []
proof = ["cargo +1.95.0 run -p xtask -- rails check"]
"#,
        );
        fs::write(&tracker_path, tracker).expect("write tracker");

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error.contains(
                "implemented Rails lane demo-lane work item unfinished has status `ready`"
            )),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_index_schema_version_drift() {
        let root = unique_temp_dir("perfgate_rails_schema_version_drift");
        write_minimal_rails_stack(&root, true);
        replace_rails_index(
            &root,
            "schema_version = \"1.0\"",
            "schema_version = \"2.0\"",
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors
                .iter()
                .any(|error| error
                    .contains(".rails/index.toml schema_version must be `1.0`, got `2.0`")),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_project_repo_drift() {
        let root = unique_temp_dir("perfgate_rails_project_repo_drift");
        write_minimal_rails_stack(&root, true);
        replace_rails_index(&root, "repo = \"perfgate\"", "repo = \"perfgate-swarm\"");

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error.contains(
                ".rails/index.toml project.repo must be `perfgate`, got `perfgate-swarm`"
            )),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_index_convention_prefix_drift() {
        let root = unique_temp_dir("perfgate_rails_convention_prefix_drift");
        write_minimal_rails_stack(&root, true);
        replace_rails_index(
            &root,
            "spec_prefix = \"PERFGATE-SPEC\"",
            "spec_prefix = \"OTHER-SPEC\"",
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error.contains(
                ".rails/index.toml conventions.spec_prefix must be `PERFGATE-SPEC`, got `OTHER-SPEC`"
            )),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_external_namespace_drift() {
        let root = unique_temp_dir("perfgate_rails_external_namespace_drift");
        write_minimal_rails_stack(&root, true);
        replace_rails_index(&root, "codex = \".codex\"", "codex = \".rails/codex\"");

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error.contains(
                ".rails/index.toml external_namespaces.codex must be `.codex`, got `.rails/codex`"
            )),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_linked_proposal_wrong_kind() {
        let root = unique_temp_dir("perfgate_rails_linked_proposal_wrong_kind");
        write_minimal_rails_stack(&root, true);
        replace_rails_index(
            &root,
            "linked_proposal = \"PERFGATE-PROP-9999\"",
            "linked_proposal = \"PERFGATE-SPEC-9999\"",
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error.contains(
                "linked_proposal references PERFGATE-SPEC-9999, which is kind `spec`; expected `proposal`"
            )),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_linked_spec_wrong_kind() {
        let root = unique_temp_dir("perfgate_rails_linked_spec_wrong_kind");
        write_minimal_rails_stack(&root, true);
        replace_rails_index(
            &root,
            "linked_specs = [\"PERFGATE-SPEC-9999\"]",
            "linked_specs = [\"PERFGATE-PROP-9999\"]",
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error.contains(
                "linked_specs references PERFGATE-PROP-9999, which is kind `proposal`; expected `spec`"
            )),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_linked_adr_wrong_kind() {
        let root = unique_temp_dir("perfgate_rails_linked_adr_wrong_kind");
        write_minimal_rails_stack(&root, true);
        replace_rails_index(
            &root,
            "linked_specs = [\"PERFGATE-SPEC-9999\"]",
            "linked_specs = [\"PERFGATE-SPEC-9999\"]\nlinked_adrs = [\"PERFGATE-SPEC-9999\"]",
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error.contains(
                "linked_adrs references PERFGATE-SPEC-9999, which is kind `spec`; expected `adr`"
            )),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_missing_support_claim_reference() {
        let root = unique_temp_dir("perfgate_rails_support_missing_ref");
        write_minimal_rails_stack(&root, true);
        write_test_file(
            &root,
            ".rails/support/claim-map.toml",
            r#"schema_version = "1.0"

[[claim]]
id = "PERFGATE-CLAIM-9999"
statement = "Test claim"
proof = ["cargo test"]
references = ["docs/missing.md"]
"#,
        );
        append_rails_index(
            &root,
            r#"
[[artifact]]
id = "PERFGATE-SUPPORT-9999"
kind = "support"
path = ".rails/support/claim-map.toml"
status = "accepted"
owner = "test"
"#,
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error.contains(
                "Rails support claim PERFGATE-CLAIM-9999 reference `docs/missing.md` does not exist"
            )),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_duplicate_support_claim_ids() {
        let root = unique_temp_dir("perfgate_rails_support_duplicate_claim");
        write_minimal_rails_stack(&root, true);
        write_test_file(
            &root,
            ".rails/support/claim-map.toml",
            r#"schema_version = "1.0"

[[claim]]
id = "PERFGATE-CLAIM-9999"
statement = "Test claim"
proof = ["cargo test"]
references = ["docs/rails.md"]

[[claim]]
id = "PERFGATE-CLAIM-9999"
statement = "Duplicate test claim"
proof = ["cargo test"]
references = ["docs/rails.md"]
"#,
        );
        append_rails_index(
            &root,
            r#"
[[artifact]]
id = "PERFGATE-SUPPORT-9999"
kind = "support"
path = ".rails/support/claim-map.toml"
status = "accepted"
owner = "test"
"#,
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error.contains(
                "Rails support artifact PERFGATE-SUPPORT-9999 has duplicate claim id PERFGATE-CLAIM-9999"
            )),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_support_claim_id_prefix_drift() {
        let root = unique_temp_dir("perfgate_rails_support_claim_id_prefix");
        write_minimal_rails_stack(&root, true);
        write_test_file(
            &root,
            ".rails/support/claim-map.toml",
            r#"schema_version = "1.0"

[[claim]]
id = "CLAIM-9999"
statement = "Test claim"
proof = ["cargo test"]
references = ["docs/rails.md"]
"#,
        );
        append_rails_index(
            &root,
            r#"
[[artifact]]
id = "PERFGATE-SUPPORT-9999"
kind = "support"
path = ".rails/support/claim-map.toml"
status = "accepted"
owner = "test"
"#,
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error
                .contains("Rails support claim CLAIM-9999 must use id prefix `PERFGATE-CLAIM-`")),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_empty_support_claim_proof_command() {
        let root = unique_temp_dir("perfgate_rails_support_empty_proof_command");
        write_minimal_rails_stack(&root, true);
        write_test_file(
            &root,
            ".rails/support/claim-map.toml",
            r#"schema_version = "1.0"

[[claim]]
id = "PERFGATE-CLAIM-9999"
statement = "Test claim"
proof = ["cargo test", " "]
references = ["docs/rails.md"]
"#,
        );
        append_rails_index(
            &root,
            r#"
[[artifact]]
id = "PERFGATE-SUPPORT-9999"
kind = "support"
path = ".rails/support/claim-map.toml"
status = "accepted"
owner = "test"
"#,
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error
                .contains("Rails support claim PERFGATE-CLAIM-9999 has an empty proof command")),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_missing_policy_ledger_path() {
        let root = unique_temp_dir("perfgate_rails_policy_missing_path");
        write_minimal_rails_stack(&root, true);
        write_test_file(
            &root,
            ".rails/policy/ledgers.toml",
            r#"schema_version = "1.0"

[[ledger]]
id = "missing-ledger"
path = "docs/missing-policy.md"
owner = "test"
"#,
        );
        append_rails_index(
            &root,
            r#"
[[artifact]]
id = "PERFGATE-POLICY-9999"
kind = "policy"
path = ".rails/policy/ledgers.toml"
status = "accepted"
owner = "test"
"#,
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error.contains(
                "Rails policy ledger missing-ledger path `docs/missing-policy.md` does not exist"
            )),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_duplicate_policy_ledger_ids() {
        let root = unique_temp_dir("perfgate_rails_policy_duplicate_ledger");
        write_minimal_rails_stack(&root, true);
        write_test_file(
            &root,
            ".rails/policy/ledgers.toml",
            r#"schema_version = "1.0"

[[ledger]]
id = "test-ledger"
path = "docs/rails.md"
owner = "test"

[[ledger]]
id = "test-ledger"
path = "docs/contributing/rails.md"
owner = "test"
"#,
        );
        append_rails_index(
            &root,
            r#"
[[artifact]]
id = "PERFGATE-POLICY-9999"
kind = "policy"
path = ".rails/policy/ledgers.toml"
status = "accepted"
owner = "test"
"#,
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error.contains(
                "Rails policy artifact PERFGATE-POLICY-9999 has duplicate ledger id test-ledger"
            )),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_unregistered_owned_artifacts() {
        let root = unique_temp_dir("perfgate_rails_unregistered_artifact");
        write_minimal_rails_stack(&root, true);
        write_test_file(
            &root,
            ".rails/support/claim-map.toml",
            "schema_version = \"1.0\"\n",
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error.contains(
                "Rails-owned artifact `.rails/support/claim-map.toml` must be registered"
            )),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_unregistered_owned_plan_artifacts() {
        let root = unique_temp_dir("perfgate_rails_unregistered_plan");
        write_minimal_rails_stack(&root, true);
        write_test_file(&root, ".rails/plans/PERFGATE-PLAN-9999-demo.md", "# Plan\n");

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error.contains(
                "Rails-owned artifact `.rails/plans/PERFGATE-PLAN-9999-demo.md` must be registered"
            )),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_unregistered_owned_template_artifacts() {
        let root = unique_temp_dir("perfgate_rails_unregistered_template");
        write_minimal_rails_stack(&root, true);
        write_test_file(&root, ".rails/templates/spec.md", "# Spec template\n");

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error
                .contains("Rails-owned artifact `.rails/templates/spec.md` must be registered")),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_artifact_path_identity_drift() {
        let root = unique_temp_dir("perfgate_rails_artifact_path_identity_drift");
        write_minimal_rails_stack(&root, true);
        write_test_file(&root, ".rails/proposals/demo.md", "# Proposal\n");
        fs::remove_file(root.join(".rails/proposals/PERFGATE-PROP-9999-demo.md"))
            .expect("remove original proposal artifact");
        replace_rails_index(
            &root,
            "path = \".rails/proposals/PERFGATE-PROP-9999-demo.md\"",
            "path = \".rails/proposals/demo.md\"",
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error.contains(
                "Rails artifact PERFGATE-PROP-9999 path `.rails/proposals/demo.md` filename must start with `PERFGATE-PROP-9999`"
            )),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_artifact_kind_path_drift() {
        let root = unique_temp_dir("perfgate_rails_artifact_kind_path_drift");
        write_minimal_rails_stack(&root, true);
        write_test_file(
            &root,
            ".rails/proposals/PERFGATE-SPEC-9999-demo.md",
            "# Spec in wrong directory\n",
        );
        fs::remove_file(root.join(".rails/specs/PERFGATE-SPEC-9999-demo.md"))
            .expect("remove original spec artifact");
        replace_rails_index(
            &root,
            "path = \".rails/specs/PERFGATE-SPEC-9999-demo.md\"",
            "path = \".rails/proposals/PERFGATE-SPEC-9999-demo.md\"",
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error.contains(
                "Rails artifact PERFGATE-SPEC-9999 kind `spec` path `.rails/proposals/PERFGATE-SPEC-9999-demo.md` must live under `.rails/specs/`"
            )),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_lane_tracker_status_drift() {
        let root = unique_temp_dir("perfgate_rails_tracker_status_drift");
        write_minimal_rails_stack(&root, true);
        write_test_file(
            &root,
            ".rails/lanes/demo-lane/tracker.toml",
            r#"schema_version = "1.0"

id = "demo-lane"
name = "Demo lane"
status = "active"
owner = "test"
"#,
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error.contains(
                "Rails lane demo-lane tracker status `active` must match index status `implemented`"
            )),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_lane_tracker_path_drift() {
        let root = unique_temp_dir("perfgate_rails_tracker_path_drift");
        write_minimal_rails_stack(&root, true);
        write_test_file(
            &root,
            ".rails/other/demo-lane/tracker.toml",
            r#"schema_version = "1.0"

id = "demo-lane"
name = "Demo lane"
status = "implemented"
owner = "test"
"#,
        );
        replace_rails_index(
            &root,
            "path = \".rails/lanes/demo-lane/tracker.toml\"",
            "path = \".rails/other/demo-lane/tracker.toml\"",
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error.contains(
                "Rails lane demo-lane path `.rails/other/demo-lane/tracker.toml` must be `.rails/lanes/demo-lane/tracker.toml`"
            )),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_lane_tracker_schema_version_drift() {
        let root = unique_temp_dir("perfgate_rails_tracker_schema_drift");
        write_minimal_rails_stack(&root, true);
        write_test_file(
            &root,
            ".rails/lanes/demo-lane/tracker.toml",
            r#"schema_version = "2.0"

id = "demo-lane"
name = "Demo lane"
status = "implemented"
owner = "test"
"#,
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error
                .contains("Rails lane demo-lane tracker schema_version `2.0` must be `1.0`")),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_lane_tracker_unknown_fields() {
        let root = unique_temp_dir("perfgate_rails_tracker_unknown_fields");
        write_minimal_rails_stack(&root, true);
        write_test_file(
            &root,
            ".rails/lanes/demo-lane/tracker.toml",
            r#"schema_version = "1.0"

id = "demo-lane"
name = "Demo lane"
status = "implemented"
owner = "test"
review_cadence = "weekly"
"#,
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| {
                error.contains("Rails lane demo-lane tracker")
                    && error.contains("must parse as TOML")
                    && error.contains("review_cadence")
            }),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_empty_lane_tracker_objective_and_end_state() {
        let root = unique_temp_dir("perfgate_rails_tracker_empty_goal");
        write_minimal_rails_stack(&root, true);
        write_test_file(
            &root,
            ".rails/lanes/demo-lane/tracker.toml",
            r#"schema_version = "1.0"

id = "demo-lane"
name = "Demo lane"
status = "implemented"
owner = "test"

objective = " "
end_state = [" "]
"#,
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors
                .iter()
                .any(|error| error
                    .contains("Rails lane demo-lane tracker objective must be non-empty")),
            "unexpected errors: {:?}",
            errors
        );
        assert!(
            errors
                .iter()
                .any(|error| error
                    .contains("Rails lane demo-lane tracker has an empty end_state entry")),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_lane_tracker_name_drift() {
        let root = unique_temp_dir("perfgate_rails_tracker_name_drift");
        write_minimal_rails_stack(&root, true);
        write_test_file(
            &root,
            ".rails/lanes/demo-lane/tracker.toml",
            r#"schema_version = "1.0"

id = "demo-lane"
name = "Other lane"
status = "implemented"
owner = "test"
"#,
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error.contains(
                "Rails lane demo-lane tracker name `Other lane` must match index name `Demo lane`"
            )),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_lane_tracker_owner_drift() {
        let root = unique_temp_dir("perfgate_rails_tracker_owner_drift");
        write_minimal_rails_stack(&root, true);
        write_test_file(
            &root,
            ".rails/lanes/demo-lane/tracker.toml",
            r#"schema_version = "1.0"

id = "demo-lane"
name = "Demo lane"
status = "implemented"
owner = "other-owner"
"#,
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error.contains(
                "Rails lane demo-lane tracker owner `other-owner` must match index owner `test`"
            )),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_empty_lane_work_item_proof_command() {
        let root = unique_temp_dir("perfgate_rails_work_item_empty_proof");
        write_minimal_rails_stack(&root, true);
        write_test_file(
            &root,
            ".rails/lanes/demo-lane/tracker.toml",
            r#"schema_version = "1.0"

id = "demo-lane"
name = "Demo lane"
status = "implemented"
owner = "test"

[[work_item]]
id = "demo-work"
status = "implemented"
proposal = "PERFGATE-PROP-9999"
spec = "PERFGATE-SPEC-9999"
adr = ""
implementation_plan = ".rails/lanes/demo-lane/implementation-plan.md"
proof = ["cargo test", " "]
"#,
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error
                .contains("Rails lane demo-lane work item demo-work has an empty proof command")),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_duplicate_lane_work_item_ids() {
        let root = unique_temp_dir("perfgate_rails_work_item_duplicate");
        write_minimal_rails_stack(&root, true);
        write_test_file(
            &root,
            ".rails/lanes/demo-lane/tracker.toml",
            r#"schema_version = "1.0"

id = "demo-lane"
name = "Demo lane"
status = "implemented"
owner = "test"

[[work_item]]
id = "demo-work"
status = "implemented"
proposal = "PERFGATE-PROP-9999"
spec = "PERFGATE-SPEC-9999"
adr = ""
implementation_plan = ".rails/lanes/demo-lane/implementation-plan.md"
proof = ["cargo test"]

[[work_item]]
id = "demo-work"
status = "implemented"
proposal = "PERFGATE-PROP-9999"
spec = "PERFGATE-SPEC-9999"
adr = ""
implementation_plan = ".rails/lanes/demo-lane/implementation-plan.md"
proof = ["cargo test"]
"#,
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error
                .contains("Rails lane demo-lane tracker has duplicate work item id demo-work")),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_unknown_lane_work_item_status() {
        let root = unique_temp_dir("perfgate_rails_work_item_unknown_status");
        write_minimal_rails_stack(&root, true);
        write_test_file(
            &root,
            ".rails/lanes/demo-lane/tracker.toml",
            r#"schema_version = "1.0"

id = "demo-lane"
name = "Demo lane"
status = "implemented"
owner = "test"

[[work_item]]
id = "demo-work"
status = "maybe"
proposal = "PERFGATE-PROP-9999"
spec = "PERFGATE-SPEC-9999"
adr = ""
implementation_plan = ".rails/lanes/demo-lane/implementation-plan.md"
proof = ["cargo test"]
"#,
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error
                .contains("Rails lane work item demo-lane/demo-work uses unknown status `maybe`")),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_unknown_lane_work_item_blocked_by() {
        let root = unique_temp_dir("perfgate_rails_work_item_unknown_blocked_by");
        write_minimal_rails_stack(&root, true);
        write_test_file(
            &root,
            ".rails/lanes/demo-lane/tracker.toml",
            r#"schema_version = "1.0"

id = "demo-lane"
name = "Demo lane"
status = "implemented"
owner = "test"

[[work_item]]
id = "demo-work"
status = "implemented"
proposal = "PERFGATE-PROP-9999"
spec = "PERFGATE-SPEC-9999"
adr = ""
implementation_plan = ".rails/lanes/demo-lane/implementation-plan.md"
blocked_by = ["missing-work"]
proof = ["cargo test"]
"#,
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error.contains(
                "Rails lane demo-lane work item demo-work blocked_by references unknown work item id missing-work"
            )),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_unknown_lane_work_item_blocks() {
        let root = unique_temp_dir("perfgate_rails_work_item_unknown_blocks");
        write_minimal_rails_stack(&root, true);
        write_test_file(
            &root,
            ".rails/lanes/demo-lane/tracker.toml",
            r#"schema_version = "1.0"

id = "demo-lane"
name = "Demo lane"
status = "implemented"
owner = "test"

[[work_item]]
id = "demo-work"
status = "implemented"
proposal = "PERFGATE-PROP-9999"
spec = "PERFGATE-SPEC-9999"
adr = ""
implementation_plan = ".rails/lanes/demo-lane/implementation-plan.md"
blocks = ["missing-work"]
proof = ["cargo test"]
"#,
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error.contains(
                "Rails lane demo-lane work item demo-work blocks references unknown work item id missing-work"
            )),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_self_referential_lane_work_item_dependency() {
        let root = unique_temp_dir("perfgate_rails_work_item_self_dependency");
        write_minimal_rails_stack(&root, true);
        write_test_file(
            &root,
            ".rails/lanes/demo-lane/tracker.toml",
            r#"schema_version = "1.0"

id = "demo-lane"
name = "Demo lane"
status = "implemented"
owner = "test"

[[work_item]]
id = "demo-work"
status = "implemented"
proposal = "PERFGATE-PROP-9999"
spec = "PERFGATE-SPEC-9999"
adr = ""
implementation_plan = ".rails/lanes/demo-lane/implementation-plan.md"
blocked_by = ["demo-work"]
proof = ["cargo test"]
"#,
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error.contains(
                "Rails lane demo-lane work item demo-work blocked_by references itself as demo-work"
            )),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_missing_lane_work_item_implementation_plan_path() {
        let root = unique_temp_dir("perfgate_rails_work_item_missing_plan");
        write_minimal_rails_stack(&root, true);
        write_test_file(
            &root,
            ".rails/lanes/demo-lane/tracker.toml",
            r#"schema_version = "1.0"

id = "demo-lane"
name = "Demo lane"
status = "implemented"
owner = "test"

[[work_item]]
id = "demo-work"
status = "implemented"
proposal = "PERFGATE-PROP-9999"
spec = "PERFGATE-SPEC-9999"
adr = ""
implementation_plan = ".rails/lanes/demo-lane/missing-plan.md"
proof = ["cargo test"]
"#,
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error.contains(
                "Rails lane demo-lane work item demo-work implementation_plan `.rails/lanes/demo-lane/missing-plan.md` does not exist"
            )),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_missing_lane_work_item_spec_link() {
        let root = unique_temp_dir("perfgate_rails_work_item_missing_spec");
        write_minimal_rails_stack(&root, true);
        write_test_file(
            &root,
            ".rails/lanes/demo-lane/tracker.toml",
            r#"schema_version = "1.0"

id = "demo-lane"
name = "Demo lane"
status = "implemented"
owner = "test"

[[work_item]]
id = "demo-work"
status = "implemented"
proposal = "PERFGATE-PROP-9999"
spec = ""
adr = ""
implementation_plan = ".rails/lanes/demo-lane/implementation-plan.md"
proof = ["cargo test"]
"#,
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error
                .contains("Rails lane demo-lane work item demo-work has an empty spec")),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_unknown_lane_work_item_proposal_link() {
        let root = unique_temp_dir("perfgate_rails_work_item_unknown_proposal");
        write_minimal_rails_stack(&root, true);
        write_test_file(
            &root,
            ".rails/lanes/demo-lane/tracker.toml",
            r#"schema_version = "1.0"

id = "demo-lane"
name = "Demo lane"
status = "implemented"
owner = "test"

[[work_item]]
id = "demo-work"
status = "implemented"
proposal = "PERFGATE-PROP-0000"
spec = "PERFGATE-SPEC-9999"
adr = ""
implementation_plan = ".rails/lanes/demo-lane/implementation-plan.md"
proof = ["cargo test"]
"#,
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error.contains(
                "Rails lane demo-lane work item demo-work proposal references unknown artifact id PERFGATE-PROP-0000"
            )),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rails_check_rejects_wrong_kind_lane_work_item_adr_link() {
        let root = unique_temp_dir("perfgate_rails_work_item_wrong_adr");
        write_minimal_rails_stack(&root, true);
        write_test_file(
            &root,
            ".rails/lanes/demo-lane/tracker.toml",
            r#"schema_version = "1.0"

id = "demo-lane"
name = "Demo lane"
status = "implemented"
owner = "test"

[[work_item]]
id = "demo-work"
status = "implemented"
proposal = "PERFGATE-PROP-9999"
spec = "PERFGATE-SPEC-9999"
adr = "PERFGATE-SPEC-9999"
implementation_plan = ".rails/lanes/demo-lane/implementation-plan.md"
proof = ["cargo test"]
"#,
        );

        let errors = collect_rails_errors(&root).expect("collect rails errors");

        assert!(
            errors.iter().any(|error| error.contains(
                "Rails lane demo-lane work item demo-work adr references PERFGATE-SPEC-9999, which is kind `spec`; expected `adr`"
            )),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn docs_source_check_accepts_minimal_linked_stack() {
        let root = unique_temp_dir("perfgate_docs_source_ok");
        write_test_file(&root, "policy/public_crates.txt", "perfgate\n");
        write_test_file(&root, "docs/status/PRODUCT_CLAIMS.md", "# Claims\n");
        write_test_file(
            &root,
            "docs/proposals/PERFGATE-PROP-0001-test.md",
            r#"# PERFGATE-PROP-0001: Test

Status: proposed
Owner: test
Created: 2026-05-13
Target milestone: 0.18.0
Linked specs: docs/specs/PERFGATE-SPEC-0001-test.md
Linked ADRs: docs/adr/PERFGATE-ADR-0001-test.md
Linked plan: plans/0.18.0/implementation-plan.md
Support/status impact: docs/status/PRODUCT_CLAIMS.md
Policy impact: policy/public_crates.txt

## Problem
"#,
        );
        write_test_file(
            &root,
            "docs/specs/PERFGATE-SPEC-0001-test.md",
            r#"# PERFGATE-SPEC-0001: Test

Status: accepted
Owner: test
Created: 2026-05-13
Milestone: 0.18.0
Behavior version: test.v1
Product surface: docs
CI surface: docs-source-check
Schema impact: none
Action impact: none
Server impact: none
Linked proposal: docs/proposals/PERFGATE-PROP-0001-test.md
Linked ADRs: docs/adr/PERFGATE-ADR-0001-test.md
Linked plan: plans/0.18.0/implementation-plan.md
Linked policy: policy/public_crates.txt
Support/status impact: docs/status/PRODUCT_CLAIMS.md
Proof commands: cargo +1.95.0 run -p xtask -- docs-source-check

## Problem
"#,
        );
        write_test_file(
            &root,
            "docs/adr/PERFGATE-ADR-0001-test.md",
            r#"# PERFGATE-ADR-0001: Test

Status: accepted
Date: 2026-05-13
Owner: test
Linked proposal: docs/proposals/PERFGATE-PROP-0001-test.md
Linked specs: docs/specs/PERFGATE-SPEC-0001-test.md

## Decision
"#,
        );
        write_test_file(
            &root,
            "plans/0.18.0/implementation-plan.md",
            r#"# Plan

Status: active
Owner: test
Created: 2026-05-13
Milestone: 0.18.0
Current PR: test
Linked proposal: docs/proposals/PERFGATE-PROP-0001-test.md
Linked specs: docs/specs/PERFGATE-SPEC-0001-test.md
Proof commands: cargo +1.95.0 run -p xtask -- docs-source-check
Rollback: revert

## Goal
"#,
        );
        write_test_file(
            &root,
            ".codex/goals/active.toml",
            r#"
id = "test"
linked_proposal = "docs/proposals/PERFGATE-PROP-0001-test.md"
linked_specs = ["docs/specs/PERFGATE-SPEC-0001-test.md"]
linked_adrs = ["docs/adr/PERFGATE-ADR-0001-test.md"]
linked_policy = ["policy/public_crates.txt"]
"#,
        );

        let errors = collect_docs_source_errors(&root).expect("collect source errors");
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn active_goal_rejects_missing_work_item_implementation_plan_link() {
        let root = unique_temp_dir("perfgate_active_goal_missing_implementation_plan");
        write_test_file(
            &root,
            ".codex/goals/active.toml",
            r#"
id = "test"

[[work_item]]
id = "demo-work"
proposal = ".rails/proposals/PERFGATE-PROP-9999-demo.md"
spec = ".rails/specs/PERFGATE-SPEC-9999-demo.md"
implementation_plan = ".rails/lanes/demo-lane/missing-plan.md"
"#,
        );

        let mut errors = Vec::new();
        validate_active_goal_toml(&root, &mut errors).expect("validate active goal");

        assert!(
            errors.iter().any(|error| error.contains(
                ".codex/goals/active.toml links to missing file `.rails/lanes/demo-lane/missing-plan.md`"
            )),
            "unexpected errors: {:?}",
            errors
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn docs_source_check_reports_bad_status_duplicate_id_and_missing_link() {
        let root = unique_temp_dir("perfgate_docs_source_errors");
        let spec = |title: &str| {
            format!(
                r#"# {title}

Status: invented
Owner: test
Created: 2026-05-13
Milestone: 0.18.0
Behavior version: test.v1
Product surface: docs
CI surface: docs-source-check
Schema impact: none
Action impact: none
Server impact: none
Linked proposal: docs/proposals/PERFGATE-PROP-9999-missing.md
Linked ADRs: none
Linked plan: none
Linked policy: none
Support/status impact: none
Proof commands: cargo +1.95.0 run -p xtask -- docs-source-check

## Problem
"#
            )
        };
        write_test_file(
            &root,
            "docs/specs/PERFGATE-SPEC-0001-a.md",
            &spec("PERFGATE-SPEC-0001: A"),
        );
        write_test_file(
            &root,
            "docs/specs/PERFGATE-SPEC-0001-b.md",
            &spec("PERFGATE-SPEC-0001: B"),
        );

        let errors = collect_docs_source_errors(&root).expect("collect source errors");
        assert!(
            errors.iter().any(|error| error.contains("unknown Status")),
            "expected status error, got {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("duplicate source-of-truth ID")),
            "expected duplicate ID error, got {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("links to missing file")),
            "expected missing link error, got {errors:?}"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn product_claims_check_accepts_claim_with_linked_tests() {
        let content = r###"# Product Claims

## PG-CLAIM-0001: Reviewable decisions

Tier: supported
Surface: CLI, receipts
Linked tests:
- crates/perfgate-cli/tests/decision.rs
Proof commands:

```bash
cargo +1.95.0 run -p xtask -- docs-check
```

Review after: before-release
"###;

        let errors = collect_product_claim_errors(content, &BTreeSet::new());
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    }

    #[test]
    fn product_claims_check_reports_missing_required_fields() {
        let content = r###"# Product Claims

## PG-CLAIM-0001: Broken claim

Tier: invented
Proof commands:

```bash
cargo test
```

## PG-CLAIM-0001: Duplicate claim

Tier: supported
Surface: CLI
Linked gates: docs-check
Proof commands:

```bash
cargo +1.95.0 run -p xtask -- docs-check
```

Review after: before-release
"###;

        let errors = collect_product_claim_errors(content, &BTreeSet::new());
        assert!(
            errors.iter().any(|error| error.contains("unknown tier")),
            "expected tier error, got {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("duplicate claim id")),
            "expected duplicate ID error, got {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("missing `Surface:`")),
            "expected surface error, got {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("missing `Review after:`")),
            "expected review-after error, got {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("must list at least one cargo +1.95.0")),
            "expected proof command error, got {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("linked tests, policy, or gates")),
            "expected linked evidence error, got {errors:?}"
        );
    }

    #[test]
    fn product_claims_check_reports_stale_planned_spec_links() {
        let content = r###"# Product Claims

## PG-CLAIM-0001: Guided adoption

Tier: supported
Surface: docs
Linked specs: `PERFGATE-SPEC-0007-guided-adoption-contract` planned
Linked gates: docs-check
Proof commands:

```bash
cargo +1.95.0 run -p xtask -- docs-check
```

Review after: before-release
"###;

        let errors = collect_product_claim_errors(
            content,
            &BTreeSet::from(["PERFGATE-SPEC-0007".to_string()]),
        );

        assert!(
            errors
                .iter()
                .any(|error| error.contains("references `PERFGATE-SPEC-0007` as planned")),
            "expected stale planned spec error, got {errors:?}"
        );
    }

    #[test]
    fn product_claims_check_accepts_supported_claim_with_current_freshness() {
        let content = r###"# Product Claims

## PG-CLAIM-0001: Policy posture

Tier: supported
Proof freshness: current
Surface: CLI, docs
Linked gates: product-claims-check
Proof commands:

```bash
cargo +1.95.0 run -p xtask -- product-claims-check
```

Review after: next-policy-change
"###;

        let errors = collect_product_claim_errors(content, &BTreeSet::new());
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    }

    #[test]
    fn product_claims_check_rejects_unknown_proof_freshness() {
        let content = r###"# Product Claims

## PG-CLAIM-0001: Policy posture

Tier: advisory
Proof freshness: fresh-ish
Surface: CLI, docs
Linked gates: product-claims-check
Proof commands:

```bash
cargo +1.95.0 run -p xtask -- product-claims-check
```

Review after: next-policy-change
"###;

        let errors = collect_product_claim_errors(content, &BTreeSet::new());
        assert!(
            errors
                .iter()
                .any(|error| error.contains("unknown proof freshness")),
            "expected proof freshness error, got {errors:?}"
        );
    }

    #[test]
    fn product_claims_check_rejects_supported_claim_with_stale_freshness() {
        let content = r###"# Product Claims

## PG-CLAIM-0001: Policy posture

Tier: supported
Proof freshness: stale
Surface: CLI, docs
Linked gates: product-claims-check
Proof commands:

```bash
cargo +1.95.0 run -p xtask -- product-claims-check
```

Review after: next-policy-change
"###;

        let errors = collect_product_claim_errors(content, &BTreeSet::new());
        assert!(
            errors.iter().any(|error| {
                error.contains("uses `stale` proof freshness for a `supported` claim")
            }),
            "expected stale supported-claim error, got {errors:?}"
        );
    }

    fn no_panic_identity(
        path: &str,
        family: &str,
        selector_kind: &str,
        selector_callee: &str,
        snippet: &str,
    ) -> NoPanicIdentity {
        NoPanicIdentity {
            path: path.to_string(),
            family: family.to_string(),
            selector_kind: selector_kind.to_string(),
            selector_callee: selector_callee.to_string(),
            snippet: snippet.to_string(),
        }
    }

    fn no_panic_allowance(identity: &NoPanicIdentity, count: u32) -> NoPanicAllowance {
        NoPanicAllowance {
            path: identity.path.clone(),
            family: identity.family.clone(),
            selector_kind: identity.selector_kind.clone(),
            selector_callee: identity.selector_callee.clone(),
            snippet: identity.snippet.clone(),
            count,
            owner: "test-owner".to_string(),
            reason: "test reason".to_string(),
            review_after: "0.17.0".to_string(),
        }
    }

    fn no_panic_baseline_entry(identity: &NoPanicIdentity, count: u32) -> NoPanicBaselineEntry {
        NoPanicBaselineEntry {
            path: identity.path.clone(),
            family: identity.family.clone(),
            selector_kind: identity.selector_kind.clone(),
            selector_callee: identity.selector_callee.clone(),
            snippet: identity.snippet.clone(),
            count,
        }
    }

    #[test]
    fn no_panic_scanner_uses_exact_counted_identities() {
        let root = unique_temp_dir("perfgate_no_panic_scan");
        let src = root.join("src");
        fs::create_dir_all(&src).expect("create src dir");
        fs::write(
            src.join("lib.rs"),
            r##"
fn demo(value: Option<u8>) {
    let _ = value.unwrap();
    let _ = value.unwrap();
    panic!("boom");
    let _ = ".expect(";
    let _ = r#".unwrap()"#;
    // todo!();
    /* unreachable!(); */
    let _ = core::marker::PhantomData::<&'static str>;
}
"##,
        )
        .expect("write source");

        let findings = scan_no_panic_family(&root).expect("scan no-panic family");
        let unwrap_identity = no_panic_identity(
            "src/lib.rs",
            "unwrap",
            "method",
            "unwrap",
            "let _ = value.unwrap();",
        );
        let panic_identity = no_panic_identity(
            "src/lib.rs",
            "panic",
            "macro",
            "panic!",
            "panic!(\"boom\");",
        );

        let unwrap = findings
            .iter()
            .find(|finding| finding.identity == unwrap_identity)
            .expect("unwrap identity");
        assert_eq!(unwrap.count, 2);
        let panic = findings
            .iter()
            .find(|finding| finding.identity == panic_identity)
            .expect("panic identity");
        assert_eq!(panic.count, 1);
        assert_eq!(findings.len(), 2);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn no_panic_allowlist_rejects_count_drift() {
        let identity = no_panic_identity(
            "crates/example/src/lib.rs",
            "expect",
            "method",
            "expect",
            "value.expect(\"present\")",
        );
        let findings = vec![NoPanicFinding {
            identity: identity.clone(),
            count: 2,
        }];

        let exact = NoPanicAllowlist {
            schema_version: "1.0".to_string(),
            allow: vec![no_panic_allowance(&identity, 2)],
        };
        assert!(collect_no_panic_policy_errors(&findings, &exact, None).is_empty());

        let drifted = NoPanicAllowlist {
            schema_version: "1.0".to_string(),
            allow: vec![no_panic_allowance(&identity, 1)],
        };
        let errors = collect_no_panic_policy_errors(&findings, &drifted, None);
        assert!(
            errors.iter().any(|error| error.contains("count mismatch")),
            "expected count mismatch, got {errors:?}"
        );
    }

    #[test]
    fn no_panic_baseline_rejects_new_or_increased_unallowed_debt() {
        let identity = no_panic_identity(
            "crates/example/src/lib.rs",
            "expect",
            "method",
            "expect",
            "value.expect(\"present\")",
        );
        let findings = vec![NoPanicFinding {
            identity: identity.clone(),
            count: 2,
        }];

        let allowlist = NoPanicAllowlist {
            schema_version: "1.0".to_string(),
            allow: Vec::new(),
        };
        let exact = NoPanicBaseline {
            schema_version: "1.0".to_string(),
            baseline: vec![no_panic_baseline_entry(&identity, 2)],
        };
        assert!(collect_no_panic_policy_errors(&findings, &allowlist, Some(&exact)).is_empty());

        let increased = NoPanicBaseline {
            schema_version: "1.0".to_string(),
            baseline: vec![no_panic_baseline_entry(&identity, 1)],
        };
        let errors = collect_no_panic_policy_errors(&findings, &allowlist, Some(&increased));
        assert!(
            errors.iter().any(|error| error.contains("count increased")),
            "expected baseline count increase, got {errors:?}"
        );

        let empty = NoPanicBaseline {
            schema_version: "1.0".to_string(),
            baseline: Vec::new(),
        };
        let errors = collect_no_panic_policy_errors(&findings, &allowlist, Some(&empty));
        assert!(
            errors
                .iter()
                .any(|error| error.contains("new unbaselined panic-family identity")),
            "expected new unbaselined error, got {errors:?}"
        );
    }

    #[test]
    fn no_panic_baseline_allows_decreased_or_disappeared_debt() {
        let identity = no_panic_identity(
            "crates/example/src/lib.rs",
            "expect",
            "method",
            "expect",
            "value.expect(\"present\")",
        );
        let baseline = NoPanicBaseline {
            schema_version: "1.0".to_string(),
            baseline: vec![no_panic_baseline_entry(&identity, 2)],
        };
        let allowlist = NoPanicAllowlist {
            schema_version: "1.0".to_string(),
            allow: Vec::new(),
        };

        let decreased = vec![NoPanicFinding {
            identity: identity.clone(),
            count: 1,
        }];
        assert!(collect_no_panic_policy_errors(&decreased, &allowlist, Some(&baseline)).is_empty());
        assert!(collect_no_panic_policy_errors(&[], &allowlist, Some(&baseline)).is_empty());
        assert_eq!(
            count_no_panic_baseline_refresh_candidates(&decreased, &allowlist, &baseline),
            1
        );
    }

    fn test_package(name: &str, publish: Option<Vec<String>>) -> MetadataPackage {
        test_package_with_deps(name, publish, Vec::new())
    }

    fn test_package_with_deps(
        name: &str,
        publish: Option<Vec<String>>,
        dependencies: Vec<MetadataDependency>,
    ) -> MetadataPackage {
        MetadataPackage {
            name: name.to_string(),
            manifest_path: PathBuf::from(format!("crates/{name}/Cargo.toml")),
            publish,
            readme: None,
            dependencies,
        }
    }

    fn workspace_dep(name: &str) -> MetadataDependency {
        MetadataDependency {
            name: name.to_string(),
            kind: None,
            path: Some(PathBuf::from(format!("crates/{name}"))),
        }
    }

    fn dev_workspace_dep(name: &str) -> MetadataDependency {
        MetadataDependency {
            name: name.to_string(),
            kind: Some("dev".to_string()),
            path: Some(PathBuf::from(format!("crates/{name}"))),
        }
    }

    fn arch_metadata(mut packages: Vec<MetadataPackage>) -> CargoMetadata {
        let mut package_names: BTreeSet<String> = packages
            .iter()
            .map(|package| package.name.clone())
            .collect();

        for rule in ARCH_RULES {
            for package_name in rule.sources.iter().chain(rule.forbidden.iter()) {
                if package_names.insert((*package_name).to_string()) {
                    packages.push(test_package(package_name, None));
                }
            }
        }

        CargoMetadata { packages }
    }

    #[test]
    fn collect_publish_errors_reports_missing_readme() {
        let metadata = CargoMetadata {
            packages: vec![MetadataPackage {
                name: "perfgate-missing-readme".to_string(),
                manifest_path: PathBuf::from("crates/perfgate-missing-readme/Cargo.toml"),
                publish: None,
                readme: Some(PathBuf::from("README.md")),
                dependencies: Vec::new(),
            }],
        };

        let errors = collect_publish_errors(&metadata);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("declares readme"));
    }

    #[test]
    fn collect_publish_errors_reports_publish_false_workspace_dependency() {
        let metadata = CargoMetadata {
            packages: vec![
                MetadataPackage {
                    name: "perfgate-cli".to_string(),
                    manifest_path: PathBuf::from("crates/perfgate-cli/Cargo.toml"),
                    publish: None,
                    readme: None,
                    dependencies: vec![MetadataDependency {
                        name: "perfgate-profile".to_string(),
                        kind: None,
                        path: Some(PathBuf::from("crates/perfgate-profile")),
                    }],
                },
                MetadataPackage {
                    name: "perfgate-profile".to_string(),
                    manifest_path: PathBuf::from("crates/perfgate-profile/Cargo.toml"),
                    publish: Some(Vec::new()),
                    readme: None,
                    dependencies: Vec::new(),
                },
            ],
        };

        let errors = collect_publish_errors(&metadata);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("depends on workspace crate perfgate-profile"));
    }

    #[test]
    fn collect_publish_errors_ignores_dev_dependencies() {
        let metadata = CargoMetadata {
            packages: vec![
                MetadataPackage {
                    name: "perfgate-cli".to_string(),
                    manifest_path: PathBuf::from("crates/perfgate-cli/Cargo.toml"),
                    publish: None,
                    readme: None,
                    dependencies: vec![MetadataDependency {
                        name: "perfgate-selfbench".to_string(),
                        kind: Some("dev".to_string()),
                        path: Some(PathBuf::from("crates/perfgate-selfbench")),
                    }],
                },
                MetadataPackage {
                    name: "perfgate-selfbench".to_string(),
                    manifest_path: PathBuf::from("crates/perfgate-selfbench/Cargo.toml"),
                    publish: Some(Vec::new()),
                    readme: None,
                    dependencies: Vec::new(),
                },
            ],
        };

        let errors = collect_publish_errors(&metadata);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
    }

    #[test]
    fn ordered_publishable_packages_uses_release_order_then_extra_names() {
        let metadata = CargoMetadata {
            packages: vec![
                test_package("perfgate-cli", None),
                test_package("perfgate-extra", None),
                test_package("perfgate-types", None),
                test_package("perfgate-internal", Some(Vec::new())),
                test_package("perfgate", None),
            ],
        };

        assert_eq!(
            ordered_publishable_packages(&metadata),
            vec![
                "perfgate-types",
                "perfgate",
                "perfgate-cli",
                "perfgate-extra"
            ]
        );
    }

    #[test]
    fn select_publishable_packages_filters_requested_packages_in_release_order() {
        let metadata = CargoMetadata {
            packages: vec![
                test_package("perfgate-cli", None),
                test_package("perfgate-types", None),
                test_package("perfgate", None),
            ],
        };
        let requested = vec!["perfgate-cli".to_string(), "perfgate-types".to_string()];

        assert_eq!(
            select_publishable_packages(&metadata, &requested).expect("selected packages"),
            vec!["perfgate-types", "perfgate-cli"]
        );
    }

    #[test]
    fn select_publishable_packages_rejects_unknown_packages() {
        let metadata = CargoMetadata {
            packages: vec![test_package("perfgate-types", None)],
        };
        let requested = vec!["perfgate-domain".to_string()];

        let err = select_publishable_packages(&metadata, &requested).unwrap_err();
        assert!(err.to_string().contains("not publishable"));
    }

    #[test]
    fn cargo_packaging_args_include_optional_allow_dirty() {
        assert_eq!(
            cargo_package_list_args("perfgate-cli", false),
            vec!["package", "-p", "perfgate-cli", "--list"]
        );
        assert_eq!(
            cargo_package_list_args("perfgate-cli", true),
            vec!["package", "-p", "perfgate-cli", "--list", "--allow-dirty"]
        );
        assert_eq!(
            cargo_publish_dry_run_args("perfgate-cli", true, None),
            vec![
                "publish",
                "-p",
                "perfgate-cli",
                "--dry-run",
                "--allow-dirty"
            ]
        );
    }

    fn valid_action_install_surface() -> &'static str {
        r####"
inputs:
  version:
    description: "Optional perfgate-cli crate version from crates.io"
  out_dir:
    description: "Artifact output directory"
    default: ""
  decision:
    description: "Run structured decision evaluation"
    default: "false"
  review_required:
    description: "How decision=true handles review-required decisions"
    default: "warn"
runs:
  using: "composite"
  steps:
    - name: Install perfgate (pre-built binary)
      run: |
        url="https://github.com/EffortlessMetrics/perfgate/releases/download/v${version}/perfgate-${target}.${ext}"
    - name: Install perfgate (cargo install fallback)
      run: |
        if [[ -n "${{ inputs.version }}" ]]; then
          cargo install perfgate-cli --locked --force --version "${{ inputs.version }}"
        else
          cargo install --path "${GITHUB_ACTION_PATH}/crates/perfgate-cli" --locked --force
        fi
    - name: Verify perfgate installation
      run: |
        perfgate --version
        perfgate doctor --help
    - name: Resolve artifact directory
      run: |
        default_out_dir="artifacts/perfgate"
        echo "out_dir=${out_dir}" >> "${GITHUB_OUTPUT}"
    - name: Run perfgate check
      run: |
        args=(check)
        if [[ -n "${{ inputs.out_dir }}" ]]; then
          args+=(--out-dir "${{ inputs.out_dir }}")
        fi
        if [[ "${{ inputs.decision }}" == "true" && "${status}" == "2" ]]; then
          echo "policy_failure_deferred=true" >> "${GITHUB_OUTPUT}"
        fi
    - name: Run perfgate decision
      if: always() && inputs.decision == 'true' && (steps.run_check.outputs.exit_code == '0' || steps.run_check.outputs.exit_code == '2')
      run: |
        args=(decision evaluate --config "${{ inputs.config }}")
        if [[ -n "${{ inputs.out_dir }}" ]]; then
          args+=(--out-dir "${{ inputs.out_dir }}")
        fi
        echo "exit_code=${status}" >> "${GITHUB_OUTPUT}"
    - id: handle_review_required
      name: Handle review-required decision
      if: always() && inputs.decision == 'true' && steps.run_decision.outputs.exit_code == '0'
      run: |
        policy="${{ inputs.review_required }}"
        case "${policy}" in
          pass|warn|fail) ;;
        esac
        out="${{ steps.resolve_out_dir.outputs.out_dir }}"
        tradeoff="${out}/tradeoff.json"
        review_required="$(python - <<'PY'
        decision.get("review_required")
        PY
        )"
        review_reason="review required"
        echo "review_required=${review_required}" >> "${GITHUB_OUTPUT}"
        echo "review_required_reason=${review_reason}" >> "${GITHUB_OUTPUT}"
        echo "exit_code=0" >> "${GITHUB_OUTPUT}"
        echo "exit_code=2" >> "${GITHUB_OUTPUT}"
    - name: Append perfgate decision summary
      if: always() && inputs.decision == 'true'
      run: |
        out="${{ steps.resolve_out_dir.outputs.out_dir }}"
        if [[ -f "${out}/decision.md" && -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
          cat "${out}/decision.md"
        fi
    - name: Append perfgate policy posture summary
      if: always() && steps.run_check.outputs.exit_code != ''
      run: |
        out="${{ steps.resolve_out_dir.outputs.out_dir }}"
        review_required="${{ steps.handle_review_required.outputs.review_required }}"
        policy_args=(policy doctor --config "${{ inputs.config }}")
        review_packet_args=(policy review-packet --config "${{ inputs.config }}" --bench "${{ inputs.bench }}")
        review_packet_output=""
        review_packet_status="skipped"
        if ! perfgate "${review_packet_args[@]}" > "${review_packet_output}" 2>&1; then
          review_packet_status="unavailable"
        fi
        {
          echo "### perfgate policy posture"
          echo "Blocking behavior: this action preserves existing perfgate exit-code behavior; maturity guidance is advisory unless your config already makes it blocking."
          echo "Advisory signal: missing baselines remain setup guidance unless this workflow enables required-baseline mode."
          echo "Blocking gate: required-baseline mode is enabled."
          echo "Imported evidence: policy doctor output includes source kind, source path, metric mapping, maturity limits, and advisory boundaries when receipts expose them."
          echo "Policy review required: ${review_reason}"
          echo "Benchmark passport (${review_packet_status}):"
          printf '%s\n' '```bash'
          printf '%s\n' '```text'
          awk '/^## Benchmark Passport/{flag=1; print; next} /^## / && flag{exit} flag{print}' "${review_packet_output}"
          printf '%s\n' '```'
          echo "Do not: make advisory maturity output blocking, loosen thresholds, promote baselines, or require server ledger mode from this summary alone."
        } >> "${GITHUB_STEP_SUMMARY}"
    - name: Print perfgate failure summary
      if: always() && ((steps.run_check.outputs.exit_code != '0' && steps.run_check.outputs.policy_failure_deferred != 'true') || (inputs.decision == 'true' && steps.run_decision.outputs.exit_code != '' && steps.run_decision.outputs.exit_code != '0') || (inputs.decision == 'true' && steps.handle_review_required.outputs.exit_code != '' && steps.handle_review_required.outputs.exit_code != '0'))
      run: |
        out="${{ steps.resolve_out_dir.outputs.out_dir }}"
        exit_code="${{ steps.run_check.outputs.exit_code }}"
        review_exit_code="${{ steps.handle_review_required.outputs.exit_code }}"
        verdict="${{ steps.run_check.outputs.verdict }}"
        pass_count="${{ steps.run_check.outputs.pass_count }}"
        warn_count="${{ steps.run_check.outputs.warn_count }}"
        fail_count="${{ steps.run_check.outputs.fail_count }}"
        bench_count="${{ steps.run_check.outputs.bench_count }}"
        review_reason="${{ steps.handle_review_required.outputs.review_required_reason }}"
        artifact_name="${{ inputs.artifact_name }}-${{ github.run_id }}-${{ github.run_attempt }}"
        repro=(perfgate check --config "${{ inputs.config }}")
        decision_repro=(perfgate decision evaluate --config "${{ inputs.config }}")
        decision_repro_line=""
        decision_repro_line="$(format_command "${decision_repro[@]}")"
        has_no_baseline_reason() {
          grep -R -q -e "no_baseline" "${out}" 2>/dev/null
        }
        {
          echo "Verdict: ${verdict} (pass=${pass_count:-0}, warn=${warn_count:-0}, fail=${fail_count:-0}, benches=${bench_count:-unknown})"
          echo "Review required: ${review_reason}"
          echo "Reproduce locally:"
          echo "  ${decision_repro_line}"
          echo "### perfgate local reproduction"
          printf '%s\n' '```bash'
          echo "${decision_repro_line}"
          printf '%s\n' '```'
          printf '%s\n' '```text'
          printf '%s\n' '```'
          echo "  perfgate baseline promote --config ${{ inputs.config }} --all"
          printf '%s\n' '```bash'
          printf '%s\n' '```'
          echo "Uploaded artifact: ${artifact_name}"
          find "${out}" -type f \( -name run.json -o -name compare.json -o -name report.json -o -name probe-compare.json -o -name scenario.json -o -name tradeoff.json -o -name decision.md -o -name decision.index.json -o -name comment.md -o -name 'perfgate.*.json' \) | sort
        } >> "${GITHUB_STEP_SUMMARY}"
    - name: Post PR comment
      run: |
        out="${{ steps.resolve_out_dir.outputs.out_dir }}"
    - name: Upload perfgate artifacts
      with:
        path: ${{ steps.resolve_out_dir.outputs.out_dir }}
"####
    }

    fn valid_cli_binstall_metadata() -> &'static str {
        r#"
[package.metadata.binstall]
pkg-url = "{ repo }/releases/download/v{ version }/perfgate-{ target }.tar.gz"
pkg-fmt = "tgz"
bin-dir = "perfgate{ binary-ext }"

[package.metadata.binstall.overrides.x86_64-pc-windows-msvc]
pkg-url = "{ repo }/releases/download/v{ version }/perfgate-{ target }.zip"
pkg-fmt = "zip"
"#
    }

    #[test]
    fn action_check_accepts_expected_install_surface() {
        let errors = collect_action_check_errors(
            valid_action_install_surface(),
            valid_cli_binstall_metadata(),
        );

        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
    }

    #[test]
    fn workflow_policy_rejects_generated_pr_auto_merge() {
        let errors = collect_workflow_policy_errors_from_entries([(
            ".github/workflows/perfgate-nightly.yml",
            r#"
name: perfgate-nightly
jobs:
  nightly:
    steps:
      - name: Merge generated PR
        run: |
          gh pr merge "$BRANCH_NAME" --auto --merge
"#,
        )]);

        assert!(
            errors
                .iter()
                .any(|error| error.contains("must not run `gh pr merge`")),
            "errors should mention forbidden gh pr merge: {:?}",
            errors
        );
    }

    #[test]
    fn workflow_policy_ignores_commented_generated_pr_auto_merge() {
        let errors = collect_workflow_policy_errors_from_entries([(
            ".github/workflows/perfgate-nightly.yml",
            r#"
name: perfgate-nightly
jobs:
  nightly:
    steps:
      - name: Explain generated PR handling
        run: |
          # gh pr merge "$BRANCH_NAME" --auto --merge
          echo "maintainer review required"
"#,
        )]);

        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
    }

    #[test]
    fn action_summary_examples_cover_expected_failure_shapes() {
        let errors = collect_action_summary_example_errors(include_str!(
            "../../docs/examples/action-failure-summaries.md"
        ));

        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
    }

    #[test]
    fn action_summary_examples_reject_missing_review_required_shape() {
        let examples = include_str!("../../docs/examples/action-failure-summaries.md")
            .replace("## Review Required", "## Human Review");
        let errors = collect_action_summary_example_errors(&examples);

        assert!(
            errors
                .iter()
                .any(|error| error.contains("review required golden example")),
            "errors should mention missing review-required example: {:?}",
            errors
        );
    }

    #[test]
    fn action_summary_examples_reject_missing_ugly_failure_shape() {
        let examples = include_str!("../../docs/examples/action-failure-summaries.md")
            .replace("## Missing Benchmark Command", "## Missing Command");
        let errors = collect_action_summary_example_errors(&examples);

        assert!(
            errors
                .iter()
                .any(|error| error.contains("missing benchmark command golden example")),
            "errors should mention missing ugly failure example: {:?}",
            errors
        );
    }

    #[test]
    fn action_summary_examples_reject_missing_policy_posture_copy() {
        let examples = include_str!("../../docs/examples/action-failure-summaries.md").replace(
            "perfgate policy doctor --config perfgate.toml",
            "perfgate doctor",
        );
        let errors = collect_action_summary_example_errors(&examples);

        assert!(
            errors
                .iter()
                .any(|error| error.contains("policy doctor command")),
            "errors should mention missing policy posture copy: {:?}",
            errors
        );
    }

    #[test]
    fn action_check_rejects_missing_decision_input() {
        let action = valid_action_install_surface().replace(
            "  decision:\n    description: \"Run structured decision evaluation\"\n    default: \"false\"\n",
            "",
        );
        let errors = collect_action_check_errors(&action, valid_cli_binstall_metadata());

        assert!(
            errors.iter().any(|error| error.contains("decision input")),
            "errors should mention missing decision input: {:?}",
            errors
        );
    }

    #[test]
    fn action_check_rejects_missing_policy_posture_summary_step() {
        let action = valid_action_install_surface().replace(
            "    - name: Append perfgate policy posture summary",
            "    - name: Append posture summary",
        );
        let errors = collect_action_check_errors(&action, valid_cli_binstall_metadata());

        assert!(
            errors.iter().any(|error| error.contains("policy posture")),
            "errors should mention missing policy posture summary: {:?}",
            errors
        );
    }

    #[test]
    fn action_check_rejects_missing_review_required_input() {
        let action = valid_action_install_surface().replace(
            "  review_required:\n    description: \"How decision=true handles review-required decisions\"\n    default: \"warn\"\n",
            "",
        );
        let errors = collect_action_check_errors(&action, valid_cli_binstall_metadata());

        assert!(
            errors
                .iter()
                .any(|error| error.contains("review_required input")),
            "errors should mention review_required input: {:?}",
            errors
        );
    }

    #[test]
    fn action_check_rejects_missing_decision_step() {
        let action = valid_action_install_surface().replace(
            "    - name: Run perfgate decision",
            "    - name: Run decision",
        );
        let errors = collect_action_check_errors(&action, valid_cli_binstall_metadata());

        assert!(
            errors
                .iter()
                .any(|error| error.contains("decision evaluation step")),
            "errors should mention missing decision step: {:?}",
            errors
        );
    }

    #[test]
    fn action_check_rejects_missing_review_required_step() {
        let action = valid_action_install_surface().replace(
            "      name: Handle review-required decision",
            "      name: Handle review decision",
        );
        let errors = collect_action_check_errors(&action, valid_cli_binstall_metadata());

        assert!(
            errors
                .iter()
                .any(|error| error.contains("review-required decision handling")),
            "errors should mention review-required handling: {:?}",
            errors
        );
    }

    #[test]
    fn action_check_rejects_review_required_step_without_output_id() {
        let action = valid_action_install_surface().replace(
            "    - id: handle_review_required\n      name: Handle review-required decision",
            "    - name: Handle review-required decision",
        );
        let errors = collect_action_check_errors(&action, valid_cli_binstall_metadata());

        assert!(
            errors
                .iter()
                .any(|error| error.contains("id handle_review_required")),
            "errors should mention review-required step id: {:?}",
            errors
        );
    }

    #[test]
    fn action_check_rejects_missing_decision_summary_step() {
        let action = valid_action_install_surface().replace(
            "    - name: Append perfgate decision summary",
            "    - name: Append generic summary",
        );
        let errors = collect_action_check_errors(&action, valid_cli_binstall_metadata());

        assert!(
            errors.iter().any(|error| error.contains("decision.md")),
            "errors should mention missing decision summary: {:?}",
            errors
        );
    }

    #[test]
    fn action_check_rejects_versioned_installing_facade_crate() {
        let action =
            valid_action_install_surface().replace("perfgate-cli --locked", "perfgate --locked");
        let errors = collect_action_check_errors(&action, valid_cli_binstall_metadata());

        assert!(
            errors
                .iter()
                .any(|error| error.contains("installs `perfgate`")),
            "errors should mention facade install mismatch: {:?}",
            errors
        );
    }

    #[test]
    fn action_check_ignores_good_install_string_in_shell_comment() {
        let action = valid_action_install_surface().replace(
            "cargo install perfgate-cli --locked --force --version \"${{ inputs.version }}\"",
            "# cargo install perfgate-cli --locked --force --version \"${{ inputs.version }}\"\n          cargo install perfgate --locked --force --version \"${{ inputs.version }}\"",
        );
        let errors = collect_action_check_errors(&action, valid_cli_binstall_metadata());

        assert!(
            errors
                .iter()
                .any(|error| error.contains("installs `perfgate`")),
            "errors should mention active facade install mismatch: {:?}",
            errors
        );
    }

    #[test]
    fn action_check_rejects_missing_local_cli_path_fallback() {
        let action =
            valid_action_install_surface().replace("/crates/perfgate-cli", "/crates/perfgate");
        let errors = collect_action_check_errors(&action, valid_cli_binstall_metadata());

        assert!(
            errors
                .iter()
                .any(|error| error.contains("local crates/perfgate-cli package")),
            "errors should mention local CLI package fallback: {:?}",
            errors
        );
    }

    #[test]
    fn action_check_rejects_missing_failure_summary_step() {
        let action = valid_action_install_surface().replace(
            "    - name: Print perfgate failure summary",
            "    - name: Print generic failure summary",
        );
        let errors = collect_action_check_errors(&action, valid_cli_binstall_metadata());

        assert!(
            errors
                .iter()
                .any(|error| error.contains("local reproduction command")),
            "errors should mention local reproduction output: {:?}",
            errors
        );
    }

    #[test]
    fn action_check_rejects_failure_summary_without_resolved_artifacts() {
        let action = valid_action_install_surface().replace(
            "out=\"${{ steps.resolve_out_dir.outputs.out_dir }}\"",
            "out=\"artifacts/perfgate\"",
        );
        let errors = collect_action_check_errors(&action, valid_cli_binstall_metadata());

        assert!(
            errors
                .iter()
                .any(|error| error.contains("resolved artifact directory")),
            "errors should mention resolved artifact directory: {:?}",
            errors
        );
    }

    #[test]
    fn action_check_rejects_failure_summary_without_step_summary() {
        let action =
            valid_action_install_surface().replace("} >> \"${GITHUB_STEP_SUMMARY}\"", "echo done");
        let errors = collect_action_check_errors(&action, valid_cli_binstall_metadata());

        assert!(
            errors
                .iter()
                .any(|error| error.contains("GITHUB_STEP_SUMMARY")),
            "errors should mention GitHub step summary: {:?}",
            errors
        );
    }

    #[test]
    fn action_check_rejects_failure_summary_without_decision_repro() {
        let action = valid_action_install_surface().replace(
            "        decision_repro=(perfgate decision evaluate --config \"${{ inputs.config }}\")\n        decision_repro_line=\"\"\n        decision_repro_line=\"$(format_command \"${decision_repro[@]}\")\"\n",
            "",
        );
        let errors = collect_action_check_errors(&action, valid_cli_binstall_metadata());

        assert!(
            errors
                .iter()
                .any(|error| error.contains("decision reproduction command")),
            "errors should mention decision reproduction output: {:?}",
            errors
        );
    }

    #[test]
    fn action_check_rejects_failure_summary_with_unsafe_markdown_fences() {
        let action =
            valid_action_install_surface().replace("printf '%s\\n' '```bash'", "echo \"```bash\"");
        let errors = collect_action_check_errors(&action, valid_cli_binstall_metadata());

        assert!(
            errors
                .iter()
                .any(|error| error.contains("Markdown code fences")),
            "errors should mention shell-safe Markdown fences: {:?}",
            errors
        );
    }

    #[test]
    fn action_check_rejects_failure_summary_without_probe_compare_artifacts() {
        let action = valid_action_install_surface().replace(" -o -name probe-compare.json", "");
        let errors = collect_action_check_errors(&action, valid_cli_binstall_metadata());

        assert!(
            errors.iter().any(|error| error.contains("probe evidence")),
            "errors should mention probe evidence artifacts: {:?}",
            errors
        );
    }

    #[test]
    fn action_check_rejects_missing_binstall_release_asset_url() {
        let manifest = valid_cli_binstall_metadata().replace(
            "perfgate-{ target }.tar.gz",
            "perfgate-cli-{ target }.tar.gz",
        );
        let errors = collect_action_check_errors(valid_action_install_surface(), &manifest);

        assert!(
            errors
                .iter()
                .any(|error| error.contains("perfgate-{target}.tar.gz")),
            "errors should mention cargo-binstall asset naming: {:?}",
            errors
        );
    }

    #[test]
    fn action_check_ignores_good_binstall_string_in_toml_comment() {
        let manifest = valid_cli_binstall_metadata().replace(
            "pkg-url = \"{ repo }/releases/download/v{ version }/perfgate-{ target }.tar.gz\"",
            "# pkg-url = \"{ repo }/releases/download/v{ version }/perfgate-{ target }.tar.gz\"\npkg-url = \"{ repo }/releases/download/v{ version }/perfgate-cli-{ target }.tar.gz\"",
        );
        let errors = collect_action_check_errors(valid_action_install_surface(), &manifest);

        assert!(
            errors
                .iter()
                .any(|error| error.contains("perfgate-{target}.tar.gz")),
            "errors should mention active cargo-binstall asset naming: {:?}",
            errors
        );
    }

    #[test]
    fn public_surface_allows_publishable_packages_with_transition_dispositions() {
        let metadata = CargoMetadata {
            packages: vec![
                test_package("perfgate", None),
                test_package("perfgate-render", None),
                test_package("perfgate-tests", Some(Vec::new())),
            ],
        };
        let public_crates = ["perfgate"].into_iter().map(String::from).collect();
        let absorbed_crates = [(
            "perfgate-render".to_string(),
            "perfgate::presentation::render".to_string(),
        )]
        .into_iter()
        .collect();

        let errors =
            collect_public_surface_errors(&metadata, &public_crates, &absorbed_crates, false);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
    }

    #[test]
    fn public_surface_strict_rejects_publishable_absorbed_packages() {
        let metadata = CargoMetadata {
            packages: vec![
                test_package("perfgate", None),
                test_package("perfgate-render", None),
            ],
        };
        let public_crates = ["perfgate"].into_iter().map(String::from).collect();
        let absorbed_crates = [(
            "perfgate-render".to_string(),
            "perfgate::presentation::render".to_string(),
        )]
        .into_iter()
        .collect();

        let errors =
            collect_public_surface_errors(&metadata, &public_crates, &absorbed_crates, true);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("still publishable"));
    }

    #[test]
    fn public_surface_strict_rejects_public_deps_on_absorbed_packages() {
        let metadata = CargoMetadata {
            packages: vec![
                test_package_with_deps("perfgate", None, vec![workspace_dep("perfgate-app")]),
                test_package("perfgate-app", Some(Vec::new())),
            ],
        };
        let public_crates = ["perfgate"].into_iter().map(String::from).collect();
        let absorbed_crates = [("perfgate-app".to_string(), "perfgate::app".to_string())]
            .into_iter()
            .collect();

        let errors =
            collect_public_surface_errors(&metadata, &public_crates, &absorbed_crates, true);
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].contains(
                "perfgate is a target public crate but depends on absorbed/internal package perfgate-app"
            ),
            "unexpected errors: {:?}",
            errors
        );
    }

    #[test]
    fn public_surface_rejects_unclassified_publishable_packages() {
        let metadata = CargoMetadata {
            packages: vec![
                test_package("perfgate", None),
                test_package("perfgate-surprise", None),
            ],
        };
        let public_crates = ["perfgate"].into_iter().map(String::from).collect();
        let absorbed_crates = BTreeMap::new();

        let errors =
            collect_public_surface_errors(&metadata, &public_crates, &absorbed_crates, false);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("perfgate-surprise is publishable"));
    }

    #[test]
    fn public_surface_rejects_workspace_dependencies_on_compatibility_wrappers() {
        let metadata = CargoMetadata {
            packages: vec![
                test_package("perfgate", None),
                test_package_with_deps("perfgate-app", None, vec![workspace_dep("perfgate-error")]),
                test_package_with_deps(
                    "perfgate-error",
                    None,
                    vec![workspace_dep("perfgate-types")],
                ),
                test_package("perfgate-types", None),
            ],
        };
        let public_crates = ["perfgate", "perfgate-types"]
            .into_iter()
            .map(String::from)
            .collect();
        let absorbed_crates = [
            ("perfgate-app".to_string(), "perfgate::app".to_string()),
            (
                "perfgate-error".to_string(),
                "perfgate_types::error [compatibility wrapper]".to_string(),
            ),
        ]
        .into_iter()
        .collect();

        let errors =
            collect_public_surface_errors(&metadata, &public_crates, &absorbed_crates, false);
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].contains("perfgate-app depends on compatibility wrapper perfgate-error"),
            "unexpected errors: {:?}",
            errors
        );
        assert!(errors[0].contains("use perfgate_types::error directly"));
        assert!(!errors[0].contains(COMPATIBILITY_WRAPPER_DISPOSITION));
    }

    #[test]
    fn public_surface_allows_dev_dependencies_on_compatibility_wrappers() {
        let metadata = CargoMetadata {
            packages: vec![
                test_package("perfgate", None),
                test_package_with_deps(
                    "perfgate-app",
                    None,
                    vec![dev_workspace_dep("perfgate-error")],
                ),
                test_package_with_deps(
                    "perfgate-error",
                    None,
                    vec![workspace_dep("perfgate-types")],
                ),
                test_package("perfgate-types", None),
            ],
        };
        let public_crates = ["perfgate", "perfgate-types"]
            .into_iter()
            .map(String::from)
            .collect();
        let absorbed_crates = [
            ("perfgate-app".to_string(), "perfgate::app".to_string()),
            (
                "perfgate-error".to_string(),
                "perfgate_types::error [compatibility wrapper]".to_string(),
            ),
        ]
        .into_iter()
        .collect();

        let errors =
            collect_public_surface_errors(&metadata, &public_crates, &absorbed_crates, false);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
    }

    #[test]
    fn arch_allows_lower_layer_dependencies() {
        let metadata = arch_metadata(vec![
            test_package("perfgate-types", None),
            test_package_with_deps(
                "perfgate-error",
                None,
                vec![workspace_dep("perfgate-types")],
            ),
            test_package_with_deps(
                "perfgate-domain",
                None,
                vec![workspace_dep("perfgate-types")],
            ),
            test_package_with_deps(
                "perfgate-render",
                None,
                vec![workspace_dep("perfgate-types")],
            ),
            test_package_with_deps("perfgate-app", None, vec![workspace_dep("perfgate-domain")]),
            test_package_with_deps(
                "perfgate-client",
                None,
                vec![workspace_dep("perfgate-types")],
            ),
            test_package_with_deps(
                "perfgate-server",
                None,
                vec![workspace_dep("perfgate-types")],
            ),
            test_package_with_deps(
                "perfgate-cli",
                None,
                vec![
                    workspace_dep("perfgate-app"),
                    workspace_dep("perfgate-client"),
                    workspace_dep("perfgate-server"),
                ],
            ),
            test_package("perfgate-api", None),
        ]);

        let errors = collect_arch_dependency_errors(&metadata);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
    }

    #[test]
    fn arch_rejects_transitive_forbidden_dependencies() {
        let metadata = arch_metadata(vec![
            test_package_with_deps("perfgate", None, vec![workspace_dep("perfgate-helper")]),
            test_package_with_deps(
                "perfgate-helper",
                None,
                vec![workspace_dep("perfgate-client")],
            ),
            test_package("perfgate-client", None),
        ]);

        let errors = collect_arch_dependency_errors(&metadata);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("perfgate must not depend on perfgate-client"));
    }

    #[test]
    fn arch_ignores_dev_dependencies() {
        let metadata = arch_metadata(vec![
            test_package_with_deps(
                "perfgate-domain",
                None,
                vec![dev_workspace_dep("perfgate-client")],
            ),
            test_package("perfgate-client", None),
        ]);

        let errors = collect_arch_dependency_errors(&metadata);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
    }

    #[test]
    fn arch_rejects_client_to_server_dependency() {
        let metadata = arch_metadata(vec![
            test_package_with_deps(
                "perfgate-client",
                None,
                vec![workspace_dep("perfgate-server")],
            ),
            test_package("perfgate-server", None),
        ]);

        let errors = collect_arch_dependency_errors(&metadata);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("perfgate-client must not depend on perfgate-server"));
    }

    #[test]
    fn cmd_schema_writes_expected_files() {
        let out_dir = unique_temp_dir("perfgate_schema");
        with_repo_cwd(|| {
            cmd_schema(&out_dir).expect("schema command");
        });

        for name in SCHEMA_FILES {
            let path = out_dir.join(name);
            assert!(path.exists(), "expected schema file {}", name);
            let bytes = fs::read(&path).expect("read schema");
            assert!(
                !bytes.is_empty(),
                "schema file {} should not be empty",
                name
            );
        }

        let _ = fs::remove_dir_all(&out_dir);
    }

    #[test]
    fn cmd_conform_accepts_valid_single_file() {
        with_repo_cwd(|| {
            let path = PathBuf::from("contracts/fixtures/sensor_report_pass.json");
            cmd_conform(None, Some(path)).expect("conform should succeed");
        });
    }

    #[test]
    fn cmd_conform_rejects_invalid_file() {
        let temp_dir = unique_temp_dir("perfgate_invalid_fixture");
        let bad_path = temp_dir.join("bad.json");
        fs::write(&bad_path, r#"{"schema":"sensor.report.v1"}"#).expect("write bad file");
        with_repo_cwd(|| {
            let result = cmd_conform(None, Some(bad_path.clone()));
            assert!(result.is_err(), "expected schema validation to fail");
        });

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn cmd_conform_accepts_fixtures_dir_without_sensor_prefix() {
        let temp_dir = unique_temp_dir("perfgate_fixtures_generic");
        with_repo_cwd(|| {
            let valid = fs::read_to_string("contracts/fixtures/sensor_report_pass.json")
                .expect("read canonical fixture");
            fs::write(temp_dir.join("third_party_report.json"), valid).expect("write fixture");

            cmd_conform(Some(temp_dir.clone()), None).expect("fixtures dir should validate");
        });

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn cmd_conform_rejects_invalid_generic_json_in_fixtures_dir() {
        let temp_dir = unique_temp_dir("perfgate_fixtures_invalid");
        with_repo_cwd(|| {
            fs::write(
                temp_dir.join("third_party_bad.json"),
                r#"{"schema":"sensor.report.v1"}"#,
            )
            .expect("write bad fixture");

            let err = cmd_conform(Some(temp_dir.clone()), None).unwrap_err();
            let msg = err.to_string();
            assert!(
                msg.contains("failed schema validation"),
                "unexpected: {}",
                msg
            );
        });

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn dogfood_run_receipt_validator_accepts_successful_samples() {
        let temp_dir = unique_temp_dir("perfgate_dogfood_ok");
        let path = temp_dir.join("perfgate.run.v1.json");
        write_test_run_receipt(&path, 0, false, None);

        validate_dogfood_run_receipt(&path).expect("successful samples should validate");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn dogfood_run_receipt_validator_rejects_failed_samples() {
        let temp_dir = unique_temp_dir("perfgate_dogfood_failed");
        let path = temp_dir.join("perfgate.run.v1.json");
        write_test_run_receipt(&path, 1, false, Some("missing workload"));

        let err = validate_dogfood_run_receipt(&path).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("failed or timed-out sample"),
            "unexpected: {}",
            msg
        );
        assert!(msg.contains("missing workload"), "unexpected: {}", msg);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn dogfood_run_receipt_validator_rejects_timed_out_samples() {
        let temp_dir = unique_temp_dir("perfgate_dogfood_timeout");
        let path = temp_dir.join("perfgate.run.v1.json");
        write_test_run_receipt(&path, 0, true, None);

        let err = validate_dogfood_run_receipt(&path).unwrap_err();
        assert!(
            err.to_string().contains("timed-out sample"),
            "unexpected: {}",
            err
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn dogfood_receipt_pattern_uses_glob_safe_separators() {
        let pattern = dogfood_receipt_pattern(
            Path::new(r"artifacts\perfgate\extras"),
            "perfgate.run.v1.json",
        );

        assert_eq!(pattern, "artifacts/perfgate/extras/**/perfgate.run.v1.json");
    }

    #[test]
    fn dogfood_bench_slug_joins_nested_receipt_components() {
        let extras = Path::new("artifacts").join("perfgate").join("extras");
        let receipt = extras
            .join("cli")
            .join("compare-small")
            .join("perfgate.run.v1.json");

        let slug = dogfood_bench_slug(&extras, &receipt, "perfgate.run.v1.json")
            .expect("nested dogfood receipt should produce slug");

        assert_eq!(slug, "cli-compare-small");
    }

    #[test]
    fn dogfood_export_trends_rejects_missing_receipts_before_requiring_binary() {
        let temp_dir = unique_temp_dir("perfgate_dogfood_export_empty");
        let artifacts_dir = temp_dir.join("artifacts").join("perfgate");
        let out_dir = temp_dir.join("trends");

        let err = export_dogfood_trends(&artifacts_dir, &out_dir).unwrap_err();
        let msg = err.to_string();

        assert!(
            msg.contains("no dogfooding receipts found"),
            "unexpected: {}",
            msg
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    fn write_test_run_receipt(path: &Path, exit_code: i32, timed_out: bool, stderr: Option<&str>) {
        let mut sample = serde_json::json!({
            "wall_ms": 1,
            "exit_code": exit_code,
            "timed_out": timed_out
        });
        if let Some(stderr) = stderr {
            sample["stderr"] = serde_json::Value::String(stderr.to_string());
        }

        let receipt = serde_json::json!({
            "schema": "perfgate.run.v1",
            "tool": { "name": "perfgate", "version": "test" },
            "run": {
                "id": "test",
                "started_at": "1970-01-01T00:00:00Z",
                "ended_at": "1970-01-01T00:00:01Z",
                "host": { "os": "linux", "arch": "x86_64" }
            },
            "bench": {
                "name": "dogfood/test",
                "command": ["true"],
                "repeat": 1,
                "warmup": 0
            },
            "samples": [sample],
            "stats": {
                "wall_ms": { "median": 1, "min": 1, "max": 1 }
            }
        });

        fs::write(
            path,
            serde_json::to_vec(&receipt).expect("serialize receipt"),
        )
        .expect("write receipt");
    }

    #[test]
    fn mutation_summary_no_results_is_ok() {
        with_temp_cwd(|_dir| {
            let result = generate_mutation_summary(None);
            assert!(result.is_ok());
        });
    }

    #[test]
    fn mutation_summary_parses_outcomes() {
        with_temp_cwd(|dir| {
            let outcomes_dir = dir.join("mutants.out");
            fs::create_dir_all(&outcomes_dir).expect("create mutants.out");
            fs::write(
                outcomes_dir.join("outcomes.json"),
                r#"[{"summary":"CaughtMutant"},{"summary":"MissedMutant"},{"summary":"Timeout"},{"summary":"Unviable"}]"#,
            )
            .expect("write outcomes");
            fs::write(outcomes_dir.join("missed.txt"), "missed-1\nmissed-2\n")
                .expect("write missed");

            let result = generate_mutation_summary(Some(MutantsCrate::Domain));
            assert!(result.is_ok());
        });
    }

    #[test]
    fn sync_fixtures_copies_sensor_reports_only() {
        let root = unique_temp_dir("perfgate_sync");
        let golden = root.join("golden");
        let contracts = root.join("contracts");
        fs::create_dir_all(&golden).expect("create golden dir");
        fs::create_dir_all(&contracts).expect("create contracts dir");

        fs::write(golden.join("sensor_report_a.json"), "a").expect("write a");
        fs::write(golden.join("sensor_report_b.json"), "b").expect("write b");
        fs::write(golden.join("not_sensor.json"), "no").expect("write other");
        fs::write(golden.join("sensor_report.txt"), "no").expect("write txt");

        let count = sync_fixtures(&golden, &contracts).expect("sync fixtures");
        assert_eq!(count, 2);
        assert_eq!(
            fs::read_to_string(contracts.join("sensor_report_a.json")).unwrap(),
            "a"
        );
        assert_eq!(
            fs::read_to_string(contracts.join("sensor_report_b.json")).unwrap(),
            "b"
        );
        assert!(!contracts.join("not_sensor.json").exists());
        assert!(!contracts.join("sensor_report.txt").exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn check_fixture_mirror_at_ok_when_matching() {
        let root = unique_temp_dir("perfgate_mirror_ok");
        let golden = root.join("golden");
        let contracts = root.join("contracts");
        fs::create_dir_all(&golden).expect("create golden dir");
        fs::create_dir_all(&contracts).expect("create contracts dir");

        fs::write(golden.join("sensor_report_ok.json"), "same").expect("write golden");
        fs::write(contracts.join("sensor_report_ok.json"), "same").expect("write contracts");

        check_fixture_mirror_at(&golden, &contracts).expect("mirror check ok");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn check_fixture_mirror_at_requires_contracts_dir() {
        let root = unique_temp_dir("perfgate_mirror_missing");
        let golden = root.join("golden");
        fs::create_dir_all(&golden).expect("create golden dir");
        fs::write(golden.join("sensor_report_ok.json"), "same").expect("write golden");

        let missing_contracts = root.join("contracts_missing");
        let err = check_fixture_mirror_at(&golden, &missing_contracts).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("does not exist"), "unexpected error: {}", msg);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn check_fixture_mirror_at_reports_missing_and_different() {
        let root = unique_temp_dir("perfgate_mirror_drift");
        let golden = root.join("golden");
        let contracts = root.join("contracts");
        fs::create_dir_all(&golden).expect("create golden dir");
        fs::create_dir_all(&contracts).expect("create contracts dir");

        fs::write(golden.join("sensor_report_missing.json"), "one").expect("write missing");
        fs::write(golden.join("sensor_report_diff.json"), "golden").expect("write golden");
        fs::write(contracts.join("sensor_report_diff.json"), "contracts").expect("write contracts");

        let err = check_fixture_mirror_at(&golden, &contracts).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("fixture(s) drifted"),
            "unexpected error: {}",
            msg
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cmd_schema_check_accepts_matching_schemas() {
        let out_dir = unique_temp_dir("perfgate_schema_check_ok");
        with_repo_cwd(|| {
            cmd_schema(&out_dir).expect("schema command");
            cmd_schema_check(&out_dir).expect("schema check should pass");
        });
        let _ = fs::remove_dir_all(&out_dir);
    }

    #[test]
    fn cmd_schema_check_reports_missing_file() {
        let out_dir = unique_temp_dir("perfgate_schema_check_missing");
        with_repo_cwd(|| {
            cmd_schema(&out_dir).expect("schema command");
            fs::remove_file(out_dir.join(SCHEMA_FILES[0])).expect("remove file");

            let err = cmd_schema_check(&out_dir).expect_err("schema check should fail");
            let msg = err.to_string();
            assert!(
                msg.contains("schema file(s) drifted"),
                "unexpected: {}",
                msg
            );
        });
        let _ = fs::remove_dir_all(&out_dir);
    }

    #[test]
    fn cmd_schema_check_reports_extra_file() {
        let out_dir = unique_temp_dir("perfgate_schema_check_extra");
        with_repo_cwd(|| {
            cmd_schema(&out_dir).expect("schema command");
            fs::write(out_dir.join("unexpected.schema.json"), "{}").expect("write extra");

            let err = cmd_schema_check(&out_dir).expect_err("schema check should fail");
            let msg = err.to_string();
            assert!(
                msg.contains("schema file(s) drifted"),
                "unexpected: {}",
                msg
            );
        });
        let _ = fs::remove_dir_all(&out_dir);
    }

    #[test]
    fn cmd_schema_check_reports_different_file() {
        let out_dir = unique_temp_dir("perfgate_schema_check_diff");
        with_repo_cwd(|| {
            cmd_schema(&out_dir).expect("schema command");
            fs::write(out_dir.join(SCHEMA_FILES[1]), "{}").expect("rewrite schema");

            let err = cmd_schema_check(&out_dir).expect_err("schema check should fail");
            let msg = err.to_string();
            assert!(
                msg.contains("schema file(s) drifted"),
                "unexpected: {}",
                msg
            );
        });
        let _ = fs::remove_dir_all(&out_dir);
    }

    #[test]
    fn cmd_schema_compat_accepts_historical_run_fixture() {
        let root = unique_temp_dir("perfgate_schema_compat");
        let version_dir = root.join("v0.15");
        fs::create_dir_all(&version_dir).expect("create fixture dir");
        fs::write(
            version_dir.join("perfgate.run.v1.json"),
            r#"{
  "schema": "perfgate.run.v1",
  "tool": {"name": "perfgate", "version": "0.15.1"},
  "run": {
    "id": "compat-run-1",
    "started_at": "2026-01-01T00:00:00Z",
    "ended_at": "2026-01-01T00:00:01Z",
    "host": {"os": "linux", "arch": "x86_64"}
  },
  "bench": {
    "name": "compat-bench",
    "command": ["echo", "compat"],
    "repeat": 1,
    "warmup": 0
  },
  "samples": [{"wall_ms": 100, "exit_code": 0}],
  "stats": {"wall_ms": {"median": 100, "min": 100, "max": 100}}
}"#,
        )
        .expect("write fixture");

        cmd_schema_compat(&root).expect("compat fixture should deserialize");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cmd_schema_compat_accepts_baseline_service_fixture() {
        let root = unique_temp_dir("perfgate_schema_compat_baseline_service");
        let version_dir = root.join("v0.16");
        fs::create_dir_all(&version_dir).expect("create fixture dir");
        fs::write(
            version_dir.join("perfgate.verdict.v1.json"),
            r#"{
  "schema": "perfgate.verdict.v1",
  "id": "verdict-compat-1",
  "project": "compat-project",
  "benchmark": "compat-bench",
  "run_id": "run-1",
  "status": "warn",
  "counts": {"pass": 0, "warn": 1, "fail": 0, "skip": 0},
  "reasons": ["wall_ms.noisy"],
  "created_at": "2026-05-07T00:00:00Z"
}"#,
        )
        .expect("write fixture");

        cmd_schema_compat(&root).expect("baseline service compat fixture should deserialize");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cmd_schema_compat_accepts_schema_less_audit_fixture_by_filename() {
        let root = unique_temp_dir("perfgate_schema_compat_audit");
        let version_dir = root.join("v0.16");
        fs::create_dir_all(&version_dir).expect("create fixture dir");
        fs::write(
            version_dir.join("perfgate.audit.v1.json"),
            r#"{
  "id": "audit-compat-1",
  "timestamp": "2026-05-07T00:00:00Z",
  "actor": "key-admin",
  "action": "create",
  "resource_type": "key",
  "resource_id": "key-1",
  "project": "compat-project",
  "metadata": {"source": "api_key"}
}"#,
        )
        .expect("write fixture");

        cmd_schema_compat(&root).expect("schema-less audit fixture should deserialize");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cmd_schema_compat_accepts_schema_less_health_fixtures_by_filename() {
        let root = unique_temp_dir("perfgate_schema_compat_health");
        let legacy_dir = root.join("v0.15");
        let current_dir = root.join("v0.16");
        fs::create_dir_all(&legacy_dir).expect("create legacy fixture dir");
        fs::create_dir_all(&current_dir).expect("create current fixture dir");
        fs::write(
            legacy_dir.join("perfgate.health.v1.json"),
            r#"{
  "status": "healthy",
  "version": "0.15.1",
  "storage": {
    "backend": "memory",
    "status": "healthy"
  }
}"#,
        )
        .expect("write legacy fixture");
        fs::write(
            current_dir.join("perfgate.health.v1.json"),
            r#"{
  "status": "degraded",
  "version": "0.16.0",
  "storage": {
    "backend": "postgres",
    "status": "unhealthy",
    "detail": "query_error"
  },
  "pool": {
    "idle": 0,
    "active": 1,
    "max": 20
  }
}"#,
        )
        .expect("write current fixture");

        cmd_schema_compat(&root).expect("schema-less health fixtures should deserialize");
        let _ = fs::remove_dir_all(&root);
    }

    // --- doc-test unit tests ---

    #[test]
    fn extract_commands_from_bash_block() {
        let md = r#"
# Example

```bash
perfgate run --name bench --out out.json -- echo hello
perfgate compare --baseline base.json --current cur.json
```
"#;
        let cmds = extract_commands(Path::new("test.md"), md);
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0].subcommand, vec!["run"]);
        assert!(cmds[0].flags.contains(&"--name".to_string()));
        assert!(cmds[0].flags.contains(&"--out".to_string()));
        assert_eq!(cmds[1].subcommand, vec!["compare"]);
        assert!(cmds[1].flags.contains(&"--baseline".to_string()));
        assert!(cmds[1].flags.contains(&"--current".to_string()));
    }

    #[test]
    fn extract_commands_cargo_run_form() {
        let md = r#"
```bash
cargo run -p perfgate-cli -- check --config perfgate.toml --bench my-bench
```
"#;
        let cmds = extract_commands(Path::new("test.md"), md);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].subcommand, vec!["check"]);
        assert!(cmds[0].flags.contains(&"--config".to_string()));
        assert!(cmds[0].flags.contains(&"--bench".to_string()));
    }

    #[test]
    fn extract_commands_multiline_continuation() {
        let md = r#"
```bash
perfgate check --config perfgate.toml \
  --bench my-bench \
  --mode cockpit
```
"#;
        let cmds = extract_commands(Path::new("test.md"), md);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].subcommand, vec!["check"]);
        assert!(cmds[0].flags.contains(&"--config".to_string()));
        assert!(cmds[0].flags.contains(&"--bench".to_string()));
        assert!(cmds[0].flags.contains(&"--mode".to_string()));
    }

    #[test]
    fn extract_commands_ignores_non_perfgate() {
        let md = r#"
```bash
echo hello
cargo build --release
ls -la
```
"#;
        let cmds = extract_commands(Path::new("test.md"), md);
        assert!(cmds.is_empty());
    }

    #[test]
    fn extract_commands_ignores_non_shell_fences() {
        let md = r#"
```toml
[dependencies]
perfgate = "0.16"
```

```rust
let command = "perfgate run --name bench";
```
"#;
        let cmds = extract_commands(Path::new("test.md"), md);
        assert!(cmds.is_empty());
    }

    #[test]
    fn extract_data_snippets_from_toml_and_json_blocks() {
        let md = r#"
```toml
[defaults]
repeat = 3
```

```json
{"status": "healthy"}
```

```yaml
name: perfgate
on:
  pull_request:
```

```bash
perfgate --help
```
"#;
        let snippets = extract_data_snippets(Path::new("test.md"), md);
        assert_eq!(snippets.len(), 3);
        assert_eq!(snippets[0].kind, DocDataKind::Toml);
        assert_eq!(snippets[1].kind, DocDataKind::Json);
        assert_eq!(snippets[2].kind, DocDataKind::Yaml);
    }

    #[test]
    fn validate_data_snippet_rejects_invalid_toml() {
        let snippet = DocDataSnippet {
            file: PathBuf::from("test.md"),
            line: 1,
            kind: DocDataKind::Toml,
            raw: "[defaults\nrepeat = 3\n".to_string(),
        };

        let err = validate_data_snippet(&snippet).expect_err("invalid TOML should fail");
        assert!(err.to_string().contains("parse TOML"));
    }

    #[test]
    fn validate_data_snippet_rejects_invalid_yaml() {
        let snippet = DocDataSnippet {
            file: PathBuf::from("test.md"),
            line: 1,
            kind: DocDataKind::Yaml,
            raw: "name: perfgate\njobs:\n  - broken: [unterminated\n".to_string(),
        };

        let err = validate_data_snippet(&snippet).expect_err("invalid YAML should fail");
        assert!(err.to_string().contains("parse YAML"));
    }

    #[test]
    fn validate_versioned_json_example_deserializes_run_receipt() {
        let value = serde_json::json!({
            "schema": perfgate_types::RUN_SCHEMA_V1,
            "tool": {"name": "perfgate", "version": "0.16.0"},
            "run": {
                "id": "run-1",
                "started_at": "2026-01-01T00:00:00Z",
                "ended_at": "2026-01-01T00:00:01Z",
                "host": {"os": "linux", "arch": "x86_64"}
            },
            "bench": {
                "name": "bench",
                "command": ["echo", "ok"],
                "repeat": 1,
                "warmup": 0
            },
            "samples": [
                {"wall_ms": 1, "exit_code": 0, "warmup": false, "timed_out": false}
            ],
            "stats": {"wall_ms": {"median": 1, "min": 1, "max": 1}}
        });

        assert_eq!(
            validate_versioned_json_example(value).expect("valid run receipt"),
            Some(perfgate_types::RUN_SCHEMA_V1)
        );
    }

    #[test]
    fn validate_versioned_json_example_rejects_schema_shape_mismatch() {
        let value = serde_json::json!({
            "schema": perfgate_types::RUN_SCHEMA_V1,
            "tool": {"name": "perfgate", "version": "0.16.0"}
        });

        let err =
            validate_versioned_json_example(value).expect_err("incomplete run receipt should fail");
        assert!(
            err.to_string()
                .contains("deserialize perfgate.run.v1 example")
        );
    }

    #[test]
    fn extract_commands_ignores_outside_code_blocks() {
        let md = r#"
Run `perfgate check --config perfgate.toml` to validate.

perfgate run --name bench -- echo hello
"#;
        let cmds = extract_commands(Path::new("test.md"), md);
        assert!(cmds.is_empty());
    }

    #[test]
    fn extract_commands_baseline_subsubcommand() {
        let md = r#"
```bash
perfgate baseline list --project my-project
```
"#;
        let cmds = extract_commands(Path::new("test.md"), md);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].subcommand, vec!["baseline", "list"]);
        assert!(cmds[0].flags.contains(&"--project".to_string()));
    }

    #[test]
    fn shell_tokenize_basic() {
        let tokens = shell_tokenize("--name bench --out out.json -- echo hello");
        assert_eq!(
            tokens,
            vec![
                "--name", "bench", "--out", "out.json", "--", "echo", "hello"
            ]
        );
    }

    #[test]
    fn shell_tokenize_with_quotes() {
        let tokens = shell_tokenize(r#"--name "my bench" --out out.json"#);
        assert_eq!(tokens, vec!["--name", "my bench", "--out", "out.json"]);
    }

    #[test]
    fn parse_subcommands_from_help_output() {
        let help = r#"Usage: perfgate [OPTIONS] <COMMAND>

Commands:
  run                 Run a command
  compare             Compare receipts
  check               Config-driven workflow
  baseline            Manage baselines
  help                Print help

Options:
  -h, --help     Print help
"#;
        let subs = parse_subcommands_from_help(help).unwrap();
        assert!(subs.contains("run"));
        assert!(subs.contains("compare"));
        assert!(subs.contains("check"));
        assert!(subs.contains("baseline"));
        assert!(!subs.contains("help"));
    }

    #[test]
    fn parse_flags_from_help_output() {
        let help = r#"Usage: perfgate run [OPTIONS] -- <COMMAND>...

Options:
      --name <NAME>      Benchmark name
      --repeat <REPEAT>  Number of repetitions [default: 7]
      --out <OUT>        Output file path
  -h, --help             Print help
"#;
        let flags = parse_flags_from_help(help).unwrap();
        assert!(flags.contains("--name"));
        assert!(flags.contains("--repeat"));
        assert!(flags.contains("--out"));
        assert!(flags.contains("--help"));
    }

    #[test]
    fn strip_cargo_run_prefix_variants() {
        assert_eq!(
            strip_cargo_run_prefix("cargo run -p perfgate-cli -- check --config foo.toml"),
            Some("check --config foo.toml")
        );
        assert_eq!(
            strip_cargo_run_prefix("cargo run -p perfgate-cli --bin perfgate -- run --name bench"),
            Some("run --name bench")
        );
        assert_eq!(
            strip_cargo_run_prefix("cargo run --release -p perfgate-cli -- run --name bench"),
            Some("run --name bench")
        );
        assert_eq!(strip_cargo_run_prefix("echo hello"), None);
    }

    #[test]
    fn extract_commands_with_dollar_prefix() {
        let md = r#"
```bash
$ perfgate run --name bench --out out.json -- echo hello
```
"#;
        let cmds = extract_commands(Path::new("test.md"), md);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].subcommand, vec!["run"]);
    }
}
