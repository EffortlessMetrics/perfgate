use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use glob::glob;
use regex::Regex;
use schemars::schema_for;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const SCHEMA_FILES: [&str; 8] = [
    "perfgate.run.v1.schema.json",
    "perfgate.compare.v1.schema.json",
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
    #[value(name = "perfgate-domain", alias = "perfgate-stats")]
    Domain,
    #[value(name = "perfgate-types", alias = "perfgate-validation")]
    Types,
    #[value(name = "perfgate-app")]
    App,
    #[value(name = "perfgate-api", alias = "perfgate-auth")]
    Api,
    #[value(name = "perfgate-adapters")]
    Adapters,
    #[value(name = "perfgate-cli")]
    Cli,
    #[value(name = "perfgate-sha256")]
    Sha256,
    #[value(name = "perfgate-host-detect")]
    HostDetect,
    #[value(name = "perfgate-export")]
    Export,
    #[value(name = "perfgate-render", alias = "perfgate-summary")]
    Render,
    #[value(name = "perfgate-sensor")]
    Sensor,
    #[value(name = "perfgate-paired")]
    Paired,
    #[value(name = "perfgate-fake")]
    Fake,
}

impl MutantsCrate {
    fn as_package_name(&self) -> &'static str {
        match self {
            MutantsCrate::Domain => "perfgate-domain",
            MutantsCrate::Types => "perfgate-types",
            MutantsCrate::App => "perfgate-app",
            MutantsCrate::Api => "perfgate-api",
            MutantsCrate::Adapters => "perfgate-adapters",
            MutantsCrate::Cli => "perfgate-cli",
            MutantsCrate::Sha256 => "perfgate-sha256",
            MutantsCrate::HostDetect => "perfgate-host-detect",
            MutantsCrate::Export => "perfgate-export",
            MutantsCrate::Render => "perfgate-render",
            MutantsCrate::Sensor => "perfgate-sensor",
            MutantsCrate::Paired => "perfgate-paired",
            MutantsCrate::Fake => "perfgate-fake",
        }
    }

    fn target_kill_rate(&self) -> u8 {
        match self {
            MutantsCrate::Domain => 100,
            MutantsCrate::Types => 95,
            MutantsCrate::App => 90,
            MutantsCrate::Api => 90,
            MutantsCrate::Adapters => 80,
            MutantsCrate::Cli => 70,
            MutantsCrate::Sha256 => 100,
            MutantsCrate::HostDetect => 100,
            MutantsCrate::Export => 90,
            MutantsCrate::Render => 90,
            MutantsCrate::Sensor => 90,
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
    PublishCheck,

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

    /// Validate CLI examples in documentation against actual --help output.
    DocTest {
        /// Additional markdown files to scan in addition to the current-doc default set
        #[arg(long)]
        files: Vec<PathBuf>,
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
    /// Generate a compact Markdown/JSON summary of drift, noise, and recommendations.
    Summarize {
        /// Directory containing perfgate export trends
        #[arg(long, default_value = "artifacts/trends")]
        dir: PathBuf,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.cmd {
        Command::Schema { out_dir } => cmd_schema(&out_dir),
        Command::SchemaCheck { schemas_dir } => cmd_schema_check(&schemas_dir),
        Command::SchemaCompat { fixtures_dir } => cmd_schema_compat(&fixtures_dir),
        Command::Ci => cmd_ci(),
        Command::PublishCheck => cmd_publish_check(),
        Command::PublicSurface {
            public_policy,
            absorbed_policy,
            strict,
        } => cmd_public_surface(&public_policy, &absorbed_policy, strict),
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
        Command::DocTest { files } => cmd_doc_test(files),
    }
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
    cmd_publish_check()?;
    cmd_public_surface(
        Path::new("policy/public_crates.txt"),
        Path::new("policy/absorbed_crates.txt"),
        false,
    )?;
    cmd_arch()?;
    cmd_doc_test(Vec::new())?;
    Ok(())
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

fn cmd_publish_check() -> anyhow::Result<()> {
    let metadata = load_cargo_metadata()?;
    let errors = collect_publish_errors(&metadata);

    if errors.is_empty() {
        println!("  OK  publishable workspace packages pass static packaging checks");
        return Ok(());
    }

    println!("Found {} publish metadata error(s):", errors.len());
    for error in &errors {
        println!("  - {}", error);
    }

    anyhow::bail!(
        "{} publish metadata issue(s) found. Fix packaging before release.",
        errors.len()
    );
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
        sources: &["perfgate-types", "perfgate-api"],
        forbidden: &[
            "perfgate-adapters",
            "perfgate-app",
            "perfgate-client",
            "perfgate-server",
            "perfgate-cli",
            "perfgate",
        ],
    },
    ArchRule {
        name: "core/domain packages stay below I/O, presentation, and entrypoints",
        sources: &[
            "perfgate-budget",
            "perfgate-domain",
            "perfgate-host-detect",
            "perfgate-paired",
            "perfgate-scaling",
            "perfgate-sha256",
            "perfgate-significance",
        ],
        forbidden: &[
            "perfgate-adapters",
            "perfgate-app",
            "perfgate-client",
            "perfgate-config",
            "perfgate-export",
            "perfgate-github",
            "perfgate-render",
            "perfgate-sensor",
            "perfgate-server",
            "perfgate-cli",
            "perfgate",
        ],
    },
    ArchRule {
        name: "presentation packages stay below runtime/app/entrypoints",
        sources: &["perfgate-export", "perfgate-render", "perfgate-sensor"],
        forbidden: &[
            "perfgate-adapters",
            "perfgate-app",
            "perfgate-client",
            "perfgate-server",
            "perfgate-cli",
            "perfgate",
        ],
    },
    ArchRule {
        name: "runtime packages stay below service/client/cli entrypoints",
        sources: &["perfgate-adapters"],
        forbidden: &[
            "perfgate-client",
            "perfgate-server",
            "perfgate-cli",
            "perfgate",
        ],
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
    label: &'static str,
    banned_patterns: &'static [&'static str],
}

const CORE_DOMAIN_ARCH_PACKAGES: &[&str] = &[
    "perfgate-budget",
    "perfgate-domain",
    "perfgate-host-detect",
    "perfgate-paired",
    "perfgate-scaling",
    "perfgate-sha256",
    "perfgate-significance",
];

const CORE_DOMAIN_BANNED_SOURCE_PATTERNS: &[&str] = &[
    "std::fs",
    "std::process",
    "tokio::fs",
    "tokio::process",
    "Command::new",
];

const PRESENTATION_ARCH_PACKAGES: &[&str] =
    &["perfgate-export", "perfgate-render", "perfgate-sensor"];

const PRESENTATION_BANNED_SOURCE_PATTERNS: &[&str] =
    &["std::process", "tokio::process", "Command::new"];

const SOURCE_ARCH_RULES: &[SourceArchRule] = &[
    SourceArchRule {
        packages: CORE_DOMAIN_ARCH_PACKAGES,
        label: "core/domain source must stay filesystem/process free",
        banned_patterns: CORE_DOMAIN_BANNED_SOURCE_PATTERNS,
    },
    SourceArchRule {
        packages: PRESENTATION_ARCH_PACKAGES,
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
                let content = fs::read_to_string(&path)
                    .with_context(|| format!("read {}", path.display()))?;
                for (line_idx, line) in content.lines().enumerate() {
                    let searchable = rust_code_before_comment(line);
                    for pattern in rule.banned_patterns {
                        if searchable.contains(pattern) {
                            errors.push(format!(
                                "{}: {} uses `{}` at {}:{}",
                                rule.label,
                                package_name,
                                pattern,
                                path.display(),
                                line_idx + 1
                            ));
                        }
                    }
                }
            }
        }
    }

    Ok(errors)
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
    match &package.publish {
        None => true,
        Some(registries) => !registries.is_empty(),
    }
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
        println!("  • perfgate-app:       90%");
        println!("  • perfgate-adapters:  80%");
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
        schema_for!(perfgate_types::ConfigFile),
    )?;

    write_schema(
        out_dir,
        SCHEMA_FILES[3],
        schema_for!(perfgate_types::PerfgateReport),
    )?;

    write_schema(
        out_dir,
        SCHEMA_FILES[4],
        schema_for!(perfgate_types::AggregateReceipt),
    )?;

    write_schema(
        out_dir,
        SCHEMA_FILES[5],
        schema_for!(perfgate_types::RatchetReceipt),
    )?;

    write_schema(
        out_dir,
        SCHEMA_FILES[6],
        schema_for!(perfgate_types::RepairContextReceipt),
    )?;

    // Sensor report schema is vendored from contracts/, not generated.
    let vendored_schema = PathBuf::from("contracts/schemas/sensor.report.v1.schema.json");
    let dest = out_dir.join(SCHEMA_FILES[7]);
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
            .map(str::to_string);

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
            "perfgate.report.v1" => {
                serde_json::from_value::<perfgate_types::PerfgateReport>(value).map(|_| ())
            }
            "sensor.report.v1" => {
                serde_json::from_value::<perfgate_types::SensorReport>(value).map(|_| ())
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

    let microcrates = [
        (
            "perfgate-error",
            "Compatibility wrapper for perfgate_types::error",
            100,
        ),
        (
            "perfgate-sha256",
            "Minimal SHA-256 implementation (no_std compatible)",
            100,
        ),
        (
            "perfgate-host-detect",
            "Host mismatch detection for CI noise reduction",
            100,
        ),
        (
            "perfgate-budget",
            "Budget evaluation logic for performance thresholds",
            100,
        ),
        (
            "perfgate-significance",
            "Statistical significance testing (Welch's t-test)",
            100,
        ),
        (
            "perfgate-export",
            "Export formats (CSV, JSONL, HTML, Prometheus)",
            90,
        ),
        (
            "perfgate-render",
            "Markdown and GitHub annotations rendering",
            90,
        ),
        (
            "perfgate-sensor",
            "Sensor report builder for cockpit integration",
            90,
        ),
        (
            "perfgate-paired",
            "Paired benchmarking statistics (A/B testing)",
            100,
        ),
        (
            "perfgate-fake",
            "Test utilities and fake implementations",
            70,
        ),
    ];

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
        ("perfgate-domain", "Pure math/policy (I/O-free)", 100),
        (
            "perfgate-adapters",
            "Platform I/O (process execution, host probing)",
            80,
        ),
        (
            "perfgate-app",
            "Use-cases, rendering, sensor report builder",
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
    println!("  perfgate-error (compatibility wrapper)");
    println!("         ↓");
    println!("  perfgate-sha256 (standalone, no_std)");
    println!("         ↓");
    println!("  perfgate-domain::stats (pure math)");
    println!("         ↓");
    println!("  perfgate-types::validation, perfgate-host-detect (pure logic)");
    println!("         ↓");
    println!("  perfgate-types (data contracts)");
    println!("         ↓");
    println!("  perfgate-budget, perfgate-significance");
    println!("         ↓");
    println!("  perfgate-export, perfgate-render, perfgate-sensor, perfgate-paired");
    println!("         ↓");
    println!("  perfgate-domain (policy)");
    println!("         ↓");
    println!("  perfgate-adapters (platform I/O)");
    println!("         ↓");
    println!("  perfgate-app (use cases)");
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
            println!("Promoted {} baselines.", count);
            Ok(())
        }
        DogfoodAction::Summarize { dir } => {
            println!("Generating trend variance summary...");
            let pattern = format!("{}/**/*.jsonl", dir.display());

            let mut all_rows: Vec<perfgate_export::RunExportRow> = Vec::new();
            for entry in glob(&pattern)? {
                let path = entry?;
                let content = fs::read_to_string(&path)?;
                for line in content.lines() {
                    if let Ok(row) = serde_json::from_str::<perfgate_export::RunExportRow>(line) {
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
                    perfgate_domain::stats::mean_and_variance(&vals).unwrap_or((0.0, 0.0));
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

    let microcrates = [
        (
            "perfgate-error",
            "Compatibility wrapper for perfgate_types::error",
            100,
        ),
        (
            "perfgate-sha256",
            "Minimal SHA-256 implementation (no_std compatible)",
            100,
        ),
        (
            "perfgate-host-detect",
            "Host mismatch detection for CI noise reduction",
            100,
        ),
        (
            "perfgate-budget",
            "Budget evaluation logic for performance thresholds",
            100,
        ),
        (
            "perfgate-significance",
            "Statistical significance testing (Welch's t-test)",
            100,
        ),
        (
            "perfgate-export",
            "Export formats (CSV, JSONL, HTML, Prometheus)",
            90,
        ),
        (
            "perfgate-render",
            "Markdown and GitHub annotations rendering",
            90,
        ),
        (
            "perfgate-sensor",
            "Sensor report builder for cockpit integration",
            90,
        ),
        (
            "perfgate-paired",
            "Paired benchmarking statistics (A/B testing)",
            100,
        ),
        (
            "perfgate-fake",
            "Test utilities and fake implementations",
            70,
        ),
    ];

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
        ("perfgate-domain", "Pure math/policy (I/O-free)", 100),
        (
            "perfgate-adapters",
            "Platform I/O (process execution, host probing)",
            80,
        ),
        ("perfgate-app", "Use-cases, orchestration layer", 90),
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
        ("perfgate", "Unified facade library", 90),
    ];

    for (name, desc, rate) in &core_crates {
        md.push_str(&format!("| `{}` | {} | {}% |\n", name, desc, rate));
    }

    md.push_str("\n## Dependency Flow\n\n");
    md.push_str("```mermaid\ngraph TD\n");
    md.push_str("  error[perfgate-error compatibility wrapper] --> types[perfgate-types]\n");
    md.push_str("  sha[perfgate-sha256] --> types\n");
    md.push_str("  domain --> stats[perfgate-domain::stats]\n");
    md.push_str("  types --> val[perfgate-types::validation]\n");
    md.push_str("  host[perfgate-host-detect] --> types\n");
    md.push_str("  types --> budget[perfgate-budget]\n");
    md.push_str("  types --> sig[perfgate-significance]\n");
    md.push_str("  budget --> domain[perfgate-domain]\n");
    md.push_str("  sig --> domain\n");
    md.push_str("  domain --> adapters[perfgate-adapters]\n");
    md.push_str("  adapters --> app[perfgate-app]\n");
    md.push_str("  app --> cli[perfgate-cli]\n");
    md.push_str("  app --> facade[perfgate]\n");
    md.push_str("  types --> client[perfgate-client]\n");
    md.push_str("  client --> app\n");
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
        assert_eq!(MutantsCrate::Domain.as_package_name(), "perfgate-domain");
        assert_eq!(MutantsCrate::Types.as_package_name(), "perfgate-types");
        assert_eq!(MutantsCrate::App.as_package_name(), "perfgate-app");
        assert_eq!(
            MutantsCrate::Adapters.as_package_name(),
            "perfgate-adapters"
        );
        assert_eq!(MutantsCrate::Cli.as_package_name(), "perfgate-cli");

        assert_eq!(MutantsCrate::Domain.target_kill_rate(), 100);
        assert_eq!(MutantsCrate::Types.target_kill_rate(), 95);
        assert_eq!(MutantsCrate::App.target_kill_rate(), 90);
        assert_eq!(MutantsCrate::Adapters.target_kill_rate(), 80);
        assert_eq!(MutantsCrate::Cli.target_kill_rate(), 70);
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
    fn public_surface_allows_publishable_packages_with_transition_dispositions() {
        let metadata = CargoMetadata {
            packages: vec![
                test_package("perfgate", None),
                test_package("perfgate-budget", None),
                test_package("perfgate-tests", Some(Vec::new())),
            ],
        };
        let public_crates = ["perfgate"].into_iter().map(String::from).collect();
        let absorbed_crates = [(
            "perfgate-budget".to_string(),
            "perfgate::core::budget".to_string(),
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
                test_package("perfgate-budget", None),
            ],
        };
        let public_crates = ["perfgate"].into_iter().map(String::from).collect();
        let absorbed_crates = [(
            "perfgate-budget".to_string(),
            "perfgate::core::budget".to_string(),
        )]
        .into_iter()
        .collect();

        let errors =
            collect_public_surface_errors(&metadata, &public_crates, &absorbed_crates, true);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("still publishable"));
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
                vec![
                    workspace_dep("perfgate-budget"),
                    workspace_dep("perfgate-types"),
                ],
            ),
            test_package_with_deps(
                "perfgate-budget",
                None,
                vec![workspace_dep("perfgate-types")],
            ),
            test_package_with_deps(
                "perfgate-render",
                None,
                vec![workspace_dep("perfgate-types")],
            ),
            test_package_with_deps(
                "perfgate-adapters",
                None,
                vec![workspace_dep("perfgate-types")],
            ),
            test_package_with_deps(
                "perfgate-app",
                None,
                vec![
                    workspace_dep("perfgate-adapters"),
                    workspace_dep("perfgate-domain"),
                ],
            ),
            test_package_with_deps(
                "perfgate-client",
                None,
                vec![workspace_dep("perfgate-types")],
            ),
            test_package_with_deps("perfgate-server", None, vec![workspace_dep("perfgate-api")]),
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
            test_package_with_deps(
                "perfgate-domain",
                None,
                vec![workspace_dep("perfgate-helper")],
            ),
            test_package_with_deps(
                "perfgate-helper",
                None,
                vec![workspace_dep("perfgate-client")],
            ),
            test_package("perfgate-client", None),
        ]);

        let errors = collect_arch_dependency_errors(&metadata);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("perfgate-domain must not depend on perfgate-client"));
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
